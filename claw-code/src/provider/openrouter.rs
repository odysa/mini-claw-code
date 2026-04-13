use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

use super::{Provider, StreamEvent, StreamProvider};
use crate::types::*;

// ---------------------------------------------------------------------------
// OpenAI-compatible API types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ApiMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ApiTool>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
}

#[derive(Serialize, Deserialize, Clone)]
struct ApiMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ApiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
struct ApiToolCall {
    id: String,
    #[serde(rename = "type")]
    type_: String,
    function: ApiFunction,
}

#[derive(Serialize, Deserialize, Clone)]
struct ApiFunction {
    name: String,
    arguments: String,
}

#[derive(Serialize)]
struct ApiTool {
    #[serde(rename = "type")]
    type_: &'static str,
    function: ApiToolDef,
}

#[derive(Serialize)]
struct ApiToolDef {
    name: &'static str,
    description: &'static str,
    parameters: Value,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
    usage: Option<ApiUsage>,
}

#[derive(Deserialize)]
struct ApiUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ResponseMessage {
    content: Option<String>,
    tool_calls: Option<Vec<ApiToolCall>>,
}

// SSE chunk types
#[derive(Deserialize)]
struct ChunkResponse {
    choices: Vec<ChunkChoice>,
}

#[derive(Deserialize)]
struct ChunkChoice {
    delta: Delta,
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct Delta {
    content: Option<String>,
    tool_calls: Option<Vec<DeltaToolCall>>,
}

#[derive(Deserialize)]
struct DeltaToolCall {
    index: usize,
    id: Option<String>,
    function: Option<DeltaFunction>,
}

#[derive(Deserialize)]
struct DeltaFunction {
    name: Option<String>,
    arguments: Option<String>,
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

pub struct OpenRouterProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl OpenRouterProvider {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.into(),
            model: model.into(),
            base_url: "https://openrouter.ai/api/v1".into(),
        }
    }

    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    pub fn from_env_with_model(model: impl Into<String>) -> anyhow::Result<Self> {
        let _ = dotenvy::dotenv();
        let api_key =
            std::env::var("OPENROUTER_API_KEY").context("OPENROUTER_API_KEY not found")?;
        Ok(Self::new(api_key, model))
    }

    pub fn from_env() -> anyhow::Result<Self> {
        Self::from_env_with_model("openrouter/free")
    }

    fn convert_messages(messages: &[Message]) -> Vec<ApiMessage> {
        let mut out = Vec::new();
        for msg in messages {
            match msg {
                Message::System(s) => out.push(ApiMessage {
                    role: "system".into(),
                    content: Some(s.content.clone()),
                    tool_calls: None,
                    tool_call_id: None,
                }),
                Message::User(u) => out.push(ApiMessage {
                    role: "user".into(),
                    content: Some(u.content.clone()),
                    tool_calls: None,
                    tool_call_id: None,
                }),
                Message::Assistant(a) => out.push(ApiMessage {
                    role: "assistant".into(),
                    content: a.text.clone(),
                    tool_calls: if a.tool_calls.is_empty() {
                        None
                    } else {
                        Some(
                            a.tool_calls
                                .iter()
                                .map(|c| ApiToolCall {
                                    id: c.id.clone(),
                                    type_: "function".into(),
                                    function: ApiFunction {
                                        name: c.name.clone(),
                                        arguments: c.arguments.to_string(),
                                    },
                                })
                                .collect(),
                        )
                    },
                    tool_call_id: None,
                }),
                Message::ToolResult(r) => out.push(ApiMessage {
                    role: "tool".into(),
                    content: Some(r.content.clone()),
                    tool_calls: None,
                    tool_call_id: Some(r.tool_use_id.clone()),
                }),
                // Attachments and Progress are not sent to the API
                Message::Attachment(a) => out.push(ApiMessage {
                    role: "system".into(),
                    content: Some(format!("[Attachment: {}]\n{}", a.path, a.content)),
                    tool_calls: None,
                    tool_call_id: None,
                }),
                Message::Progress(_) => {}
            }
        }
        out
    }

    fn convert_tools(tools: &[&ToolDefinition]) -> Vec<ApiTool> {
        tools
            .iter()
            .map(|t| ApiTool {
                type_: "function",
                function: ApiToolDef {
                    name: t.name,
                    description: t.description,
                    parameters: t.parameters.clone(),
                },
            })
            .collect()
    }

    fn parse_assistant(choice: Choice, usage: Option<ApiUsage>) -> AssistantMessage {
        let tool_calls = choice
            .message
            .tool_calls
            .unwrap_or_default()
            .into_iter()
            .map(|tc| {
                let arguments = serde_json::from_str(&tc.function.arguments).unwrap_or(Value::Null);
                ToolCall {
                    id: tc.id,
                    name: tc.function.name,
                    arguments,
                }
            })
            .collect::<Vec<_>>();

        let stop_reason = match choice.finish_reason.as_deref() {
            Some("tool_calls") => StopReason::ToolUse,
            _ if !tool_calls.is_empty() => StopReason::ToolUse,
            _ => StopReason::Stop,
        };

        let token_usage = usage.map(|u| TokenUsage {
            input_tokens: u.prompt_tokens.unwrap_or(0),
            output_tokens: u.completion_tokens.unwrap_or(0),
            ..Default::default()
        });

        AssistantMessage {
            id: new_id(),
            text: choice.message.content,
            tool_calls,
            stop_reason,
            usage: token_usage,
        }
    }
}

/// Parse one SSE `data:` line into StreamEvents.
pub fn parse_sse_line(line: &str) -> Option<Vec<StreamEvent>> {
    let data = line.strip_prefix("data: ")?;
    if data == "[DONE]" {
        return Some(vec![StreamEvent::Done]);
    }

    let chunk: ChunkResponse = serde_json::from_str(data).ok()?;
    let choice = chunk.choices.into_iter().next()?;
    let mut events = Vec::new();

    if let Some(text) = choice.delta.content
        && !text.is_empty()
    {
        events.push(StreamEvent::TextDelta(text));
    }

    if let Some(tool_calls) = choice.delta.tool_calls {
        for tc in tool_calls {
            if let Some(id) = tc.id {
                let name = tc
                    .function
                    .as_ref()
                    .and_then(|f| f.name.clone())
                    .unwrap_or_default();
                events.push(StreamEvent::ToolCallStart {
                    index: tc.index,
                    id,
                    name,
                });
            }
            if let Some(ref func) = tc.function
                && let Some(ref args) = func.arguments
                && !args.is_empty()
            {
                events.push(StreamEvent::ToolCallDelta {
                    index: tc.index,
                    arguments: args.clone(),
                });
            }
        }
    }

    if events.is_empty() {
        None
    } else {
        Some(events)
    }
}

/// Collects StreamEvents into a complete AssistantMessage.
pub struct StreamAccumulator {
    text: String,
    tool_calls: Vec<PartialToolCall>,
}

struct PartialToolCall {
    id: String,
    name: String,
    arguments: String,
}

impl StreamAccumulator {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            tool_calls: Vec::new(),
        }
    }

    pub fn feed(&mut self, event: &StreamEvent) {
        match event {
            StreamEvent::TextDelta(s) => self.text.push_str(s),
            StreamEvent::ToolCallStart { index, id, name } => {
                while self.tool_calls.len() <= *index {
                    self.tool_calls.push(PartialToolCall {
                        id: String::new(),
                        name: String::new(),
                        arguments: String::new(),
                    });
                }
                self.tool_calls[*index].id = id.clone();
                self.tool_calls[*index].name = name.clone();
            }
            StreamEvent::ToolCallDelta { index, arguments } => {
                if let Some(tc) = self.tool_calls.get_mut(*index) {
                    tc.arguments.push_str(arguments);
                }
            }
            StreamEvent::Done => {}
        }
    }

    pub fn finish(self) -> AssistantMessage {
        let text = if self.text.is_empty() {
            None
        } else {
            Some(self.text)
        };
        let tool_calls: Vec<ToolCall> = self
            .tool_calls
            .into_iter()
            .filter(|tc| !tc.name.is_empty())
            .map(|tc| ToolCall {
                id: tc.id,
                name: tc.name,
                arguments: serde_json::from_str(&tc.arguments).unwrap_or(Value::Null),
            })
            .collect();
        let stop_reason = if tool_calls.is_empty() {
            StopReason::Stop
        } else {
            StopReason::ToolUse
        };
        AssistantMessage {
            id: new_id(),
            text,
            tool_calls,
            stop_reason,
            usage: None,
        }
    }
}

impl Default for StreamAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for OpenRouterProvider {
    async fn chat(
        &self,
        messages: &[Message],
        tools: &[&ToolDefinition],
    ) -> anyhow::Result<AssistantMessage> {
        let body = ChatRequest {
            model: &self.model,
            messages: Self::convert_messages(messages),
            tools: Self::convert_tools(tools),
            stream: false,
        };

        let resp: ChatResponse = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("request failed")?
            .error_for_status()
            .context("API error")?
            .json()
            .await
            .context("parse error")?;

        let choice = resp.choices.into_iter().next().context("no choices")?;
        Ok(Self::parse_assistant(choice, resp.usage))
    }
}

impl StreamProvider for OpenRouterProvider {
    async fn stream_chat(
        &self,
        messages: &[Message],
        tools: &[&ToolDefinition],
        tx: mpsc::UnboundedSender<StreamEvent>,
    ) -> anyhow::Result<AssistantMessage> {
        let body = ChatRequest {
            model: &self.model,
            messages: Self::convert_messages(messages),
            tools: Self::convert_tools(tools),
            stream: true,
        };

        let mut resp = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("request failed")?
            .error_for_status()
            .context("API error")?;

        let mut acc = StreamAccumulator::new();
        let mut buffer = String::new();

        while let Some(chunk) = resp.chunk().await.context("read chunk")? {
            let text = std::str::from_utf8(&chunk).context("invalid UTF-8 in SSE stream")?;
            buffer.push_str(text);
            while let Some(pos) = buffer.find('\n') {
                let line = buffer[..pos].trim_end_matches('\r').to_owned();
                buffer.drain(..=pos);
                if line.is_empty() {
                    continue;
                }
                if let Some(events) = parse_sse_line(&line) {
                    for event in events {
                        acc.feed(&event);
                        let _ = tx.send(event);
                    }
                }
            }
        }

        Ok(acc.finish())
    }
}

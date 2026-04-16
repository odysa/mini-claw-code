use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::types::*;

// ---------------------------------------------------------------------------
// OpenAI-compatible request/response types (provided — do not modify)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub(crate) struct ChatRequest<'a> {
    pub(crate) model: &'a str,
    pub(crate) messages: Vec<ApiMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) tools: Vec<ApiTool>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) stream: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct ApiMessage {
    pub(crate) role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool_calls: Option<Vec<ApiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool_call_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct ApiToolCall {
    pub(crate) id: String,
    #[serde(rename = "type")]
    pub(crate) type_: String,
    pub(crate) function: ApiFunction,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct ApiFunction {
    pub(crate) name: String,
    pub(crate) arguments: String,
}

#[derive(Serialize, Debug)]
pub(crate) struct ApiTool {
    #[serde(rename = "type")]
    pub(crate) type_: &'static str,
    pub(crate) function: ApiToolDef,
}

#[derive(Serialize, Debug)]
pub(crate) struct ApiToolDef {
    pub(crate) name: &'static str,
    pub(crate) description: &'static str,
    pub(crate) parameters: Value,
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

// ---------------------------------------------------------------------------
// Provider implementation
// ---------------------------------------------------------------------------

/// An HTTP provider that talks to OpenRouter (or any OpenAI-compatible API).
///
/// # Chapter 6: The HTTP Provider
///
/// Your task: Implement the constructor methods and the Provider trait.
/// The serde types above handle serialization — you write the logic.
pub struct OpenRouterProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl OpenRouterProvider {
    /// Create a new provider with the given API key and model name.
    ///
    /// Hint: Use `reqwest::Client::new()` and `.into()` for string conversion.
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.into(),
            model: model.into(),
            base_url: "https://openrouter.ai/api/v1".into(),
        }
    }

    /// Override the base URL (useful for testing with a local server).
    ///
    /// Default is "https://openrouter.ai/api/v1".
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Create a provider from the OPENROUTER_API_KEY environment variable.
    pub fn from_env_with_model(model: impl Into<String>) -> anyhow::Result<Self> {
        let _ = dotenvy::dotenv();
        let api_key = std::env::var("OPENROUTER_API_KEY").context("OPENROUTER_API_KEY not set")?;
        Ok(Self::new(api_key, model))
    }

    /// Create a provider from env with the default model.
    pub fn from_env() -> anyhow::Result<Self> {
        Self::from_env_with_model("openrouter/free")
    }

    /// Convert our Message types to the API's message format.
    ///
    /// Hint: Match on each Message variant and create the corresponding ApiMessage.
    /// - System -> role: "system", content: Some(text.clone())
    /// - User -> role: "user", content: Some(text.clone())
    /// - Assistant -> role: "assistant", content: turn.text.clone(),
    ///   tool_calls: convert each ToolCall to ApiToolCall (arguments.to_string() for Value→String)
    ///   Set tool_calls to None (not Some(vec![])) when empty.
    /// - ToolResult -> role: "tool", content: Some(content.clone()), tool_call_id: Some(id.clone())
    pub(crate) fn convert_messages(messages: &[Message]) -> Vec<ApiMessage> {
        messages
            .iter()
            .map(|msg| match msg {
                Message::System(text) => ApiMessage {
                    role: "system".into(),
                    content: Some(text.clone()),
                    tool_calls: None,
                    tool_call_id: None,
                },
                Message::User(text) => ApiMessage {
                    role: "user".into(),
                    content: Some(text.clone()),
                    tool_calls: None,
                    tool_call_id: None,
                },
                Message::Assistant(turn) => {
                    let tool_calls = if turn.tool_calls.is_empty() {
                        None
                    } else {
                        Some(
                            turn.tool_calls
                                .iter()
                                .map(|tc| ApiToolCall {
                                    id: tc.id.clone(),
                                    type_: "function".into(),
                                    function: ApiFunction {
                                        name: tc.name.clone(),
                                        arguments: tc.arguments.to_string(),
                                    },
                                })
                                .collect(),
                        )
                    };
                    ApiMessage {
                        role: "assistant".into(),
                        content: turn.text.clone(),
                        tool_calls,
                        tool_call_id: None,
                    }
                }
                Message::ToolResult { id, content } => ApiMessage {
                    role: "tool".into(),
                    content: Some(content.clone()),
                    tool_calls: None,
                    tool_call_id: Some(id.clone()),
                },
            })
            .collect()
    }

    /// Convert our ToolDefinition types to the API's tool format.
    ///
    /// Each tool becomes: { type: "function", function: { name, description, parameters } }
    pub(crate) fn convert_tools(tools: &[&ToolDefinition]) -> Vec<ApiTool> {
        tools
            .iter()
            .map(|td| ApiTool {
                type_: "function",
                function: ApiToolDef {
                    name: td.name,
                    description: td.description,
                    parameters: td.parameters.clone(),
                },
            })
            .collect()
    }
}

impl crate::streaming::StreamProvider for OpenRouterProvider {
    /// Stream a chat request using SSE.
    ///
    /// # Chapter 10: Streaming
    ///
    /// Hints:
    /// 1. Build ChatRequest with stream: true
    /// 2. Send POST request
    /// 3. Read response chunks with resp.chunk().await
    /// 4. Buffer bytes, split on newlines
    /// 5. Parse each line with parse_sse_line()
    /// 6. Feed events to StreamAccumulator and send via tx
    /// 7. Return acc.finish()
    async fn stream_chat(
        &self,
        messages: &[Message],
        tools: &[&ToolDefinition],
        tx: tokio::sync::mpsc::UnboundedSender<crate::streaming::StreamEvent>,
    ) -> anyhow::Result<AssistantTurn> {
        use crate::streaming::{StreamAccumulator, StreamEvent, parse_sse_line};

        let request = ChatRequest {
            model: &self.model,
            messages: Self::convert_messages(messages),
            tools: Self::convert_tools(tools),
            stream: true,
        };

        let mut resp = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&request)
            .send()
            .await
            .context("failed to send streaming request")?;

        let mut acc = StreamAccumulator::new();
        let mut buffer = String::new();

        while let Some(chunk) = resp.chunk().await? {
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(pos) = buffer.find('\n') {
                let line = buffer[..pos].trim_end_matches('\r').to_string();
                buffer = buffer[pos + 1..].to_string();
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

impl Provider for OpenRouterProvider {
    /// Send a chat request to the API and parse the response.
    ///
    /// Steps:
    /// 1. Build a ChatRequest with model, converted messages, converted tools, stream: false
    /// 2. POST to {base_url}/chat/completions with bearer auth
    /// 3. Parse the JSON response as ChatResponse
    /// 4. Extract the first choice's message
    /// 5. Convert any tool_calls back to our ToolCall type
    ///    (parse function.arguments from String to Value with serde_json::from_str)
    /// 6. Determine stop_reason from choice.finish_reason:
    ///    - "tool_calls" → StopReason::ToolUse
    ///    - anything else → StopReason::Stop
    /// 7. Extract usage from response if present
    async fn chat(
        &self,
        messages: &[Message],
        tools: &[&ToolDefinition],
    ) -> anyhow::Result<AssistantTurn> {
        let request = ChatRequest {
            model: &self.model,
            messages: Self::convert_messages(messages),
            tools: Self::convert_tools(tools),
            stream: false,
        };

        let resp = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&request)
            .send()
            .await
            .context("failed to send request")?;

        let body = resp.text().await.context("failed to read response body")?;
        let chat_resp: ChatResponse =
            serde_json::from_str(&body).context("failed to parse response")?;

        let choice = chat_resp
            .choices
            .into_iter()
            .next()
            .context("no choices in response")?;

        let tool_calls = choice
            .message
            .tool_calls
            .unwrap_or_default()
            .into_iter()
            .map(|tc| ToolCall {
                id: tc.id,
                name: tc.function.name,
                arguments: serde_json::from_str(&tc.function.arguments).unwrap_or(Value::Null),
            })
            .collect::<Vec<_>>();

        let stop_reason = if choice.finish_reason.as_deref() == Some("tool_calls") {
            StopReason::ToolUse
        } else {
            StopReason::Stop
        };

        let usage = chat_resp.usage.map(|u| TokenUsage {
            input_tokens: u.prompt_tokens.unwrap_or(0),
            output_tokens: u.completion_tokens.unwrap_or(0),
        });

        Ok(AssistantTurn {
            text: choice.message.content,
            tool_calls,
            stop_reason,
            usage,
        })
    }
}

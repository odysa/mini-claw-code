use tokio::sync::mpsc;

use crate::provider::Provider;
use crate::types::*;

/// Events emitted by the query engine during execution.
#[derive(Debug)]
pub enum QueryEvent {
    /// A chunk of text streamed from the LLM.
    TextDelta(String),
    /// A tool is about to be called.
    ToolStart { name: String, summary: String },
    /// A tool finished executing.
    ToolEnd { name: String, result: String },
    /// The engine finished with a final response.
    Done(String),
    /// The engine encountered an error.
    Error(String),
}

/// Configuration for the query engine.
pub struct QueryConfig {
    /// Maximum number of agent loop iterations before stopping.
    pub max_turns: usize,
    /// Maximum tool result size in characters before truncation.
    pub max_result_chars: usize,
}

impl Default for QueryConfig {
    fn default() -> Self {
        Self {
            max_turns: 50,
            max_result_chars: 100_000,
        }
    }
}

/// The core agent loop — mirrors Claude Code's QueryEngine.
///
/// Orchestrates: prompt → LLM call → tool dispatch → result collection → loop.
pub struct QueryEngine<P: Provider> {
    provider: P,
    tools: ToolSet,
    config: QueryConfig,
}

impl<P: Provider> QueryEngine<P> {
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            tools: ToolSet::new(),
            config: QueryConfig::default(),
        }
    }

    pub fn config(mut self, config: QueryConfig) -> Self {
        self.config = config;
        self
    }

    pub fn tool(mut self, t: impl Tool + 'static) -> Self {
        self.tools.push(t);
        self
    }

    pub fn tools(mut self, tools: ToolSet) -> Self {
        self.tools = tools;
        self
    }

    /// Execute all tool calls and return results.
    async fn execute_tools(&self, calls: &[ToolCall]) -> Vec<(String, ToolResult)> {
        self.tools
            .execute_calls(calls, self.config.max_result_chars)
            .await
    }

    /// Run the query engine with a single prompt.
    ///
    /// Returns the final text response.
    pub async fn run(&self, prompt: &str) -> anyhow::Result<String> {
        let mut messages = vec![Message::user(prompt)];
        self.chat(&mut messages).await
    }

    /// Run the query engine with existing message history.
    ///
    /// The caller pushes messages before calling. On return, the vec
    /// contains the full conversation including the assistant's final turn.
    pub async fn chat(&self, messages: &mut Vec<Message>) -> anyhow::Result<String> {
        let defs = self.tools.definitions();
        let mut turns = 0;

        loop {
            if turns >= self.config.max_turns {
                anyhow::bail!("exceeded max turns ({})", self.config.max_turns);
            }

            let turn = self.provider.chat(messages, &defs).await?;

            match turn.stop_reason {
                StopReason::Stop => {
                    let text = turn.text.clone().unwrap_or_default();
                    messages.push(Message::Assistant(turn));
                    return Ok(text);
                }
                StopReason::ToolUse => {
                    let results = self.execute_tools(&turn.tool_calls).await;
                    messages.push(Message::Assistant(turn));
                    for (id, result) in results {
                        messages.push(Message::tool_result(id, result.content));
                    }
                }
            }

            turns += 1;
        }
    }

    /// Run with event streaming via channel.
    pub async fn run_with_events(
        &self,
        prompt: &str,
        events: mpsc::UnboundedSender<QueryEvent>,
    ) -> Vec<Message> {
        let mut messages = vec![Message::user(prompt)];
        self.chat_with_events(&mut messages, events).await;
        messages
    }

    /// Chat with event streaming.
    pub async fn chat_with_events(
        &self,
        messages: &mut Vec<Message>,
        events: mpsc::UnboundedSender<QueryEvent>,
    ) {
        let defs = self.tools.definitions();
        let mut turns = 0;

        loop {
            if turns >= self.config.max_turns {
                let _ = events.send(QueryEvent::Error(format!(
                    "exceeded max turns ({})",
                    self.config.max_turns
                )));
                return;
            }

            let turn = match self.provider.chat(messages, &defs).await {
                Ok(t) => t,
                Err(e) => {
                    let _ = events.send(QueryEvent::Error(e.to_string()));
                    return;
                }
            };

            match turn.stop_reason {
                StopReason::Stop => {
                    let text = turn.text.clone().unwrap_or_default();
                    let _ = events.send(QueryEvent::Done(text));
                    messages.push(Message::Assistant(turn));
                    return;
                }
                StopReason::ToolUse => {
                    for call in &turn.tool_calls {
                        if let Some(t) = self.tools.get(&call.name) {
                            let _ = events.send(QueryEvent::ToolStart {
                                name: call.name.clone(),
                                summary: t.summary(&call.arguments),
                            });
                        }
                    }

                    let results = self.execute_tools(&turn.tool_calls).await;

                    for (id, result) in &results {
                        let call_name = turn
                            .tool_calls
                            .iter()
                            .find(|c| c.id == *id)
                            .map(|c| c.name.clone())
                            .unwrap_or_default();
                        let _ = events.send(QueryEvent::ToolEnd {
                            name: call_name,
                            result: if result.content.len() > 200 {
                                format!("{}...", &result.content[..200])
                            } else {
                                result.content.clone()
                            },
                        });
                    }

                    messages.push(Message::Assistant(turn));
                    for (id, result) in results {
                        messages.push(Message::tool_result(id, result.content));
                    }
                }
            }

            turns += 1;
        }
    }
}

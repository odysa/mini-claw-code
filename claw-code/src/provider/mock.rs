use std::collections::VecDeque;
use std::sync::Mutex;

use tokio::sync::mpsc;

use super::{Provider, StreamEvent, StreamProvider};
use crate::types::*;

/// A mock provider for testing. Returns pre-configured responses in sequence.
pub struct MockProvider {
    responses: Mutex<VecDeque<AssistantMessage>>,
}

impl MockProvider {
    pub fn new(responses: VecDeque<AssistantMessage>) -> Self {
        Self {
            responses: Mutex::new(responses),
        }
    }
}

impl Provider for MockProvider {
    async fn chat(
        &self,
        _messages: &[Message],
        _tools: &[&ToolDefinition],
    ) -> anyhow::Result<AssistantMessage> {
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| anyhow::anyhow!("MockProvider: no more responses"))
    }
}

/// Mock streaming provider that synthesizes StreamEvents from canned responses.
pub struct MockStreamProvider {
    inner: MockProvider,
}

impl MockStreamProvider {
    pub fn new(responses: VecDeque<AssistantMessage>) -> Self {
        Self {
            inner: MockProvider::new(responses),
        }
    }
}

impl StreamProvider for MockStreamProvider {
    async fn stream_chat(
        &self,
        messages: &[Message],
        tools: &[&ToolDefinition],
        tx: mpsc::UnboundedSender<StreamEvent>,
    ) -> anyhow::Result<AssistantMessage> {
        let turn = self.inner.chat(messages, tools).await?;

        // Synthesize stream events from the complete turn
        if let Some(ref text) = turn.text {
            for ch in text.chars() {
                let _ = tx.send(StreamEvent::TextDelta(ch.to_string()));
            }
        }
        for (i, call) in turn.tool_calls.iter().enumerate() {
            let _ = tx.send(StreamEvent::ToolCallStart {
                index: i,
                id: call.id.clone(),
                name: call.name.clone(),
            });
            let _ = tx.send(StreamEvent::ToolCallDelta {
                index: i,
                arguments: call.arguments.to_string(),
            });
        }
        let _ = tx.send(StreamEvent::Done);

        Ok(turn)
    }
}

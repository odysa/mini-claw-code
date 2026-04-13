mod mock;
pub mod openrouter;

pub use mock::{MockProvider, MockStreamProvider};
pub use openrouter::OpenRouterProvider;

use std::future::Future;
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::types::*;

/// Non-streaming LLM provider.
///
/// Uses RPITIT (return-position impl Trait in trait) because providers are
/// always used as generic parameters, never as trait objects.
pub trait Provider: Send + Sync {
    fn chat<'a>(
        &'a self,
        messages: &'a [Message],
        tools: &'a [&'a ToolDefinition],
    ) -> impl Future<Output = anyhow::Result<AssistantMessage>> + Send + 'a;
}

/// Arc<P> is a Provider whenever P is — enables sharing between agents.
impl<P: Provider> Provider for Arc<P> {
    fn chat<'a>(
        &'a self,
        messages: &'a [Message],
        tools: &'a [&'a ToolDefinition],
    ) -> impl Future<Output = anyhow::Result<AssistantMessage>> + Send + 'a {
        (**self).chat(messages, tools)
    }
}

/// Event emitted during streaming.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    TextDelta(String),
    ToolCallStart {
        index: usize,
        id: String,
        name: String,
    },
    ToolCallDelta {
        index: usize,
        arguments: String,
    },
    Done,
}

/// Streaming LLM provider.
pub trait StreamProvider: Send + Sync {
    fn stream_chat<'a>(
        &'a self,
        messages: &'a [Message],
        tools: &'a [&'a ToolDefinition],
        tx: mpsc::UnboundedSender<StreamEvent>,
    ) -> impl Future<Output = anyhow::Result<AssistantMessage>> + Send + 'a;
}

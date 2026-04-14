use std::collections::VecDeque;
use std::sync::Mutex;

use crate::types::*;

/// A mock provider for testing. Returns pre-configured responses in sequence.
///
/// # Chapter 1: MockProvider
///
/// Your task: Implement a provider that stores a list of `AssistantTurn` responses
/// and returns them one by one each time `chat()` is called.
///
/// Hints:
/// - Use `Mutex<VecDeque<AssistantTurn>>` to allow mutation through `&self`
/// - `pop_front()` removes from the front, giving FIFO order
/// - When constructing `AssistantTurn` for tests, set `usage: None`
pub struct MockProvider {
    pub(crate) responses: Mutex<VecDeque<AssistantTurn>>,
}

impl MockProvider {
    /// Create a new MockProvider that will return the given responses in order.
    ///
    /// Hint: Wrap the `VecDeque` in a `Mutex` and store it in `Self`.
    pub fn new(_responses: VecDeque<AssistantTurn>) -> Self {
        unimplemented!("Wrap responses in a Mutex and store in Self")
    }
}

impl Provider for MockProvider {
    /// Return the next canned response, or error if none remain.
    ///
    /// Hint: Lock the mutex, pop_front() the next response, convert None to an error.
    /// Use `some_option.ok_or_else(|| anyhow::anyhow!("..."))` to convert Option to Result.
    async fn chat(
        &self,
        _messages: &[Message],
        _tools: &[&ToolDefinition],
    ) -> anyhow::Result<AssistantTurn> {
        unimplemented!("Lock mutex, pop_front, return Ok(response) or Err if empty")
    }
}

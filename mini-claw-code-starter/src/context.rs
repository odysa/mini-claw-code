use crate::types::{Message, Provider, TokenUsage};

/// Manages conversation context by compacting old messages when the token
/// budget is exceeded.
///
/// # Chapter 18: Project Instructions & Context Management
pub struct ContextManager {
    max_tokens: u64,
    preserve_recent: usize,
    tokens_used: u64,
}

impl ContextManager {
    pub fn new(max_tokens: u64, preserve_recent: usize) -> Self {
        Self {
            max_tokens,
            preserve_recent,
            tokens_used: 0,
        }
    }

    /// Add the tokens from a single turn to the running total.
    pub fn record(&mut self, _usage: &TokenUsage) {
        unimplemented!("TODO ch18: increment tokens_used by input_tokens + output_tokens")
    }

    pub fn tokens_used(&self) -> u64 {
        self.tokens_used
    }

    pub fn should_compact(&self) -> bool {
        self.tokens_used >= self.max_tokens
    }

    /// Compact messages by summarizing old ones via the LLM.
    ///
    /// Hints:
    /// - Keep the leading system message (if any) plus the last `preserve_recent` messages.
    /// - Render the middle range as a short transcript ("User: ...", "Assistant: ...",
    ///   "  [tool: name]", "  Tool result: ...") and ask the provider to summarize.
    /// - Replace the middle with a synthetic `System("[Conversation summary]: ...")`.
    /// - Reset `tokens_used` to reflect the shrunken history (`/= 3` is a fine proxy).
    #[allow(clippy::ptr_arg)]
    pub async fn compact<P: Provider>(
        &mut self,
        _provider: &P,
        _messages: &mut Vec<Message>,
    ) -> anyhow::Result<()> {
        unimplemented!(
            "TODO ch18: summarize the middle of the history into a single System message"
        )
    }

    pub async fn maybe_compact<P: Provider>(
        &mut self,
        provider: &P,
        messages: &mut Vec<Message>,
    ) -> anyhow::Result<()> {
        if self.should_compact() {
            self.compact(provider, messages).await?;
        }
        Ok(())
    }
}

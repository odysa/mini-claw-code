use crate::types::{Message, Provider, TokenUsage};

/// Manages conversation context by compacting old messages when the token
/// budget is exceeded.
///
/// # Chapter 15: Context Management
pub struct ContextManager {
    max_tokens: u64,
    preserve_recent: usize,
    tokens_used: u64,
}

impl ContextManager {
    pub fn new(max_tokens: u64, preserve_recent: usize) -> Self {
        unimplemented!("Initialize fields")
    }

    pub fn record(&mut self, usage: &TokenUsage) {
        unimplemented!("Add input_tokens + output_tokens to tokens_used")
    }

    pub fn tokens_used(&self) -> u64 {
        self.tokens_used
    }

    pub fn should_compact(&self) -> bool {
        unimplemented!("Return true if tokens_used >= max_tokens")
    }

    /// Compact messages by summarizing old ones via the LLM.
    ///
    /// Hints:
    /// - Keep the first message (system prompt) and last preserve_recent messages
    /// - Summarize the middle messages by asking the provider
    /// - Replace middle with a single System message containing the summary
    /// - Reset tokens_used to tokens_used / 3
    pub async fn compact<P: Provider>(
        &mut self,
        provider: &P,
        messages: &mut Vec<Message>,
    ) -> anyhow::Result<()> {
        unimplemented!("Split messages, summarize middle via provider, rebuild")
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

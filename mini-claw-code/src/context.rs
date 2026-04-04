use crate::types::{Message, Provider, TokenUsage};

/// Manages conversation context by compacting old messages when the token
/// budget is exceeded.
pub struct ContextManager {
    /// Maximum total tokens before compaction triggers.
    max_tokens: u64,
    /// Number of recent messages to always preserve during compaction.
    preserve_recent: usize,
    /// Running total of tokens used in the current conversation.
    tokens_used: u64,
}

impl ContextManager {
    /// Create a context manager with a token budget and a count of recent
    /// messages to preserve during compaction.
    pub fn new(max_tokens: u64, preserve_recent: usize) -> Self {
        Self {
            max_tokens,
            preserve_recent,
            tokens_used: 0,
        }
    }

    /// Record token usage from a turn.
    pub fn record(&mut self, usage: &TokenUsage) {
        self.tokens_used += usage.input_tokens + usage.output_tokens;
    }

    /// Current estimated token usage.
    pub fn tokens_used(&self) -> u64 {
        self.tokens_used
    }

    /// Returns `true` if the conversation should be compacted.
    pub fn should_compact(&self) -> bool {
        self.tokens_used >= self.max_tokens
    }

    /// Compact the message history by summarizing old messages with the LLM.
    ///
    /// Keeps the first message (system prompt, if present) and the most recent
    /// `preserve_recent` messages. Everything in between is replaced by a
    /// single system message containing a summary.
    pub async fn compact<P: Provider>(
        &mut self,
        provider: &P,
        messages: &mut Vec<Message>,
    ) -> anyhow::Result<()> {
        if messages.len() <= self.preserve_recent + 1 {
            return Ok(());
        }

        // Split: keep first message + last N messages, summarize the middle
        let keep_start = if matches!(messages.first(), Some(Message::System(_))) {
            1
        } else {
            0
        };

        let total = messages.len();
        if total <= keep_start + self.preserve_recent {
            return Ok(());
        }

        let middle_end = total - self.preserve_recent;
        let middle = &messages[keep_start..middle_end];

        if middle.is_empty() {
            return Ok(());
        }

        // Build a summarization prompt
        let mut summary_parts = Vec::new();
        for msg in middle {
            match msg {
                Message::User(text) => summary_parts.push(format!("User: {text}")),
                Message::Assistant(turn) => {
                    if let Some(ref text) = turn.text {
                        summary_parts.push(format!("Assistant: {text}"));
                    }
                    for call in &turn.tool_calls {
                        summary_parts.push(format!("  [tool: {}]", call.name));
                    }
                }
                Message::ToolResult { content, .. } => {
                    let preview = if content.len() > 100 {
                        format!("{}...", &content[..100])
                    } else {
                        content.clone()
                    };
                    summary_parts.push(format!("  Tool result: {preview}"));
                }
                Message::System(text) => summary_parts.push(format!("System: {text}")),
            }
        }

        let prompt = format!(
            "Summarize this conversation history in 2-3 sentences, \
             preserving key facts and decisions:\n\n{}",
            summary_parts.join("\n")
        );

        let summary_messages = vec![Message::User(prompt)];
        let turn = provider.chat(&summary_messages, &[]).await?;
        let summary_text = turn.text.unwrap_or_else(|| "Previous conversation.".into());

        // Rebuild messages: [head] + [summary] + [recent]
        let mut new_messages = Vec::new();
        // Keep leading messages (system prompt)
        for msg in messages.iter().take(keep_start) {
            // We need to move, but we'll drain instead
            // For now, we'll reconstruct
            if let Message::System(text) = msg {
                new_messages.push(Message::System(text.clone()));
            }
        }

        new_messages.push(Message::System(format!(
            "[Conversation summary]: {summary_text}"
        )));

        // Keep recent messages — we need to drain from the original vec
        let recent_start = total - self.preserve_recent;
        let recent: Vec<Message> = messages.drain(recent_start..).collect();
        new_messages.extend(recent);

        *messages = new_messages;

        // Reset token counter (rough estimate: summary is much shorter)
        self.tokens_used /= 3;

        Ok(())
    }

    /// Check if compaction is needed and perform it if so.
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

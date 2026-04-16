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
        Self {
            max_tokens,
            preserve_recent,
            tokens_used: 0,
        }
    }

    pub fn record(&mut self, usage: &TokenUsage) {
        self.tokens_used += usage.input_tokens + usage.output_tokens;
    }

    pub fn tokens_used(&self) -> u64 {
        self.tokens_used
    }

    pub fn should_compact(&self) -> bool {
        self.tokens_used >= self.max_tokens
    }

    /// Compact messages by summarizing old ones via the LLM.
    #[allow(clippy::ptr_arg)]
    pub async fn compact<P: Provider>(
        &mut self,
        provider: &P,
        messages: &mut Vec<Message>,
    ) -> anyhow::Result<()> {
        if messages.len() <= self.preserve_recent + 1 {
            return Ok(());
        }

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

        let mut new_messages = Vec::new();
        for msg in messages.iter().take(keep_start) {
            if let Message::System(text) = msg {
                new_messages.push(Message::System(text.clone()));
            }
        }

        new_messages.push(Message::System(format!(
            "[Conversation summary]: {summary_text}"
        )));

        let recent_start = total - self.preserve_recent;
        let recent: Vec<Message> = messages.drain(recent_start..).collect();
        new_messages.extend(recent);

        *messages = new_messages;
        self.tokens_used /= 3;

        Ok(())
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

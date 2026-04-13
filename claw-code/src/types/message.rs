use serde::{Deserialize, Serialize};

use super::tool::ToolCall;
use super::usage::TokenUsage;

/// Unique identifier for messages in the conversation.
pub type MessageId = String;

/// A message in the conversation history.
///
/// Mirrors Claude Code's message system with rich variants for different
/// participants and meta-information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Message {
    /// System instructions or context.
    System(SystemMessage),
    /// User input.
    User(UserMessage),
    /// Assistant (LLM) response.
    Assistant(AssistantMessage),
    /// Result of executing a tool.
    ToolResult(ToolResultMessage),
    /// Attached file or context (CLAUDE.md, images, etc.).
    Attachment(AttachmentMessage),
    /// Progress update from a running tool (UI only, not sent to API).
    Progress(ProgressMessage),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMessage {
    pub id: MessageId,
    pub content: String,
    /// Tag for categorization (e.g., "instructions", "compact_boundary").
    #[serde(default)]
    pub tag: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessage {
    pub id: MessageId,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    pub id: MessageId,
    pub text: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub stop_reason: StopReason,
    pub usage: Option<TokenUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultMessage {
    pub id: MessageId,
    /// The tool_use ID this result corresponds to.
    pub tool_use_id: String,
    pub content: String,
    /// Whether the result was truncated due to size limits.
    #[serde(default)]
    pub is_truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentMessage {
    pub id: MessageId,
    pub path: String,
    pub content: String,
    /// Type of attachment: "file", "memory", "instructions".
    pub content_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressMessage {
    pub tool_use_id: String,
    pub data: serde_json::Value,
}

/// Why the model stopped generating.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StopReason {
    /// The model finished — check `text` for the response.
    Stop,
    /// The model wants to use tools — check `tool_calls`.
    ToolUse,
}

impl Message {
    /// Create a new system message.
    pub fn system(content: impl Into<String>) -> Self {
        Self::System(SystemMessage {
            id: new_id(),
            content: content.into(),
            tag: None,
        })
    }

    /// Create a new user message.
    pub fn user(content: impl Into<String>) -> Self {
        Self::User(UserMessage {
            id: new_id(),
            content: content.into(),
        })
    }

    /// Create a tool result message.
    pub fn tool_result(tool_use_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::ToolResult(ToolResultMessage {
            id: new_id(),
            tool_use_id: tool_use_id.into(),
            content: content.into(),
            is_truncated: false,
        })
    }

    /// Create an assistant message from a completed turn.
    pub fn assistant(
        text: Option<String>,
        tool_calls: Vec<ToolCall>,
        stop_reason: StopReason,
        usage: Option<TokenUsage>,
    ) -> Self {
        Self::Assistant(AssistantMessage {
            id: new_id(),
            text,
            tool_calls,
            stop_reason,
            usage,
        })
    }
}

/// Generate a simple unique ID.
pub fn new_id() -> MessageId {
    uuid::Uuid::new_v4().to_string()
}

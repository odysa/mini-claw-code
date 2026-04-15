use std::collections::VecDeque;
use std::sync::Arc;

use serde_json::{Value, json};
use tokio::sync::{Mutex, oneshot};

use crate::types::{Tool, ToolDefinition};

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Abstracts how user input is collected.
///
/// # Chapter 11: User Input
#[async_trait::async_trait]
pub trait InputHandler: Send + Sync {
    async fn ask(&self, question: &str, options: &[String]) -> anyhow::Result<String>;
}

// ---------------------------------------------------------------------------
// AskTool
// ---------------------------------------------------------------------------

/// Tool that lets the LLM ask the user a clarifying question.
///
/// # Chapter 11: User Input
pub struct AskTool {
    definition: ToolDefinition,
    handler: Arc<dyn InputHandler>,
}

impl AskTool {
    /// Create with a question (required) and options (optional array) parameter.
    ///
    /// Hint: Use param_raw for the "options" array parameter.
    pub fn new(handler: Arc<dyn InputHandler>) -> Self {
        unimplemented!("Create ToolDefinition with name 'ask_user', required 'question' param, optional 'options' array param via param_raw, store handler")
    }
}

#[async_trait::async_trait]
impl Tool for AskTool {
    fn definition(&self) -> &ToolDefinition {
        &self.definition
    }

    /// Extract question and options, call handler.ask().
    async fn call(&self, args: Value) -> anyhow::Result<String> {
        unimplemented!("Extract 'question' from args, parse options with parse_options(), call self.handler.ask()")
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Extract the optional `options` array from tool arguments.
fn parse_options(args: &Value) -> Vec<String> {
    args.get("options")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// Prints the question to stdout and reads from stdin.
pub struct CliInputHandler;

#[async_trait::async_trait]
impl InputHandler for CliInputHandler {
    async fn ask(&self, question: &str, options: &[String]) -> anyhow::Result<String> {
        unimplemented!("Print question and options, read stdin line via spawn_blocking, resolve numeric choice to option text")
    }
}

/// A request sent through a channel to collect user input asynchronously.
pub struct UserInputRequest {
    pub question: String,
    pub options: Vec<String>,
    pub response_tx: oneshot::Sender<String>,
}

/// Sends a UserInputRequest through a channel and awaits the response.
pub struct ChannelInputHandler {
    tx: tokio::sync::mpsc::UnboundedSender<UserInputRequest>,
}

impl ChannelInputHandler {
    pub fn new(tx: tokio::sync::mpsc::UnboundedSender<UserInputRequest>) -> Self {
        Self { tx }
    }
}

#[async_trait::async_trait]
impl InputHandler for ChannelInputHandler {
    /// Send a UserInputRequest and await the oneshot response.
    async fn ask(&self, question: &str, options: &[String]) -> anyhow::Result<String> {
        unimplemented!("Create oneshot channel, send UserInputRequest through self.tx, await and return the response")
    }
}

/// Returns pre-configured answers in sequence. Used in tests.
pub struct MockInputHandler {
    answers: Mutex<VecDeque<String>>,
}

impl MockInputHandler {
    pub fn new(answers: VecDeque<String>) -> Self {
        Self {
            answers: Mutex::new(answers),
        }
    }
}

#[async_trait::async_trait]
impl InputHandler for MockInputHandler {
    async fn ask(&self, _question: &str, _options: &[String]) -> anyhow::Result<String> {
        unimplemented!("Same pattern as MockProvider::chat() — lock, pop_front, ok_or_else error")
    }
}

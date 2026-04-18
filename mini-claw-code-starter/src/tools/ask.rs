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
    /// Hint: Use `ToolDefinition::new("ask_user", ...)`,
    /// `.param("question", "string", ..., true)`,
    /// then `.param_raw("options", json!({"type": "array", ...}), false)`.
    pub fn new(_handler: Arc<dyn InputHandler>) -> Self {
        unimplemented!("TODO bonus: build ask_user ToolDefinition + store the handler")
    }
}

#[async_trait::async_trait]
impl Tool for AskTool {
    fn definition(&self) -> &ToolDefinition {
        &self.definition
    }

    /// Extract question and options, call handler.ask().
    async fn call(&self, _args: Value) -> anyhow::Result<String> {
        unimplemented!(
            "TODO bonus: pull question+options from args and delegate to self.handler.ask"
        )
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
        use std::io::{self, BufRead, Write};

        let question = question.to_string();
        let options = options.to_vec();

        tokio::task::spawn_blocking(move || {
            println!("\n  {question}");
            for (i, opt) in options.iter().enumerate() {
                println!("    {}) {opt}", i + 1);
            }

            print!("  > ");
            io::stdout().flush()?;
            let mut line = String::new();
            io::stdin().lock().read_line(&mut line)?;
            let answer = line.trim().to_string();

            // If the user typed a valid option number, resolve it
            if let Ok(n) = answer.parse::<usize>()
                && n >= 1
                && n <= options.len()
            {
                return Ok(options[n - 1].clone());
            }
            Ok(answer)
        })
        .await?
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
        let (response_tx, response_rx) = oneshot::channel();
        self.tx.send(UserInputRequest {
            question: question.to_string(),
            options: options.to_vec(),
            response_tx,
        })?;
        Ok(response_rx.await?)
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
    /// Return the next canned answer, or error if the queue is empty.
    async fn ask(&self, _question: &str, _options: &[String]) -> anyhow::Result<String> {
        unimplemented!("TODO bonus: pop the next pre-configured answer from self.answers")
    }
}

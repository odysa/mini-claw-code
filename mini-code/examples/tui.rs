use std::io::{self, BufRead, Write};
use std::sync::Arc;
use std::time::Duration;

use mini_code::{
    AgentEvent, BashTool, EditTool, Message, OpenRouterProvider, ReadTool, StreamingAgent,
    WriteTool,
};
use tokio::sync::mpsc;

const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

// ANSI helpers
const BOLD_CYAN: &str = "\x1b[1;36m";
const BOLD_MAGENTA: &str = "\x1b[1;35m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";
const CLEAR_LINE: &str = "\x1b[2K\r";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let provider = OpenRouterProvider::from_env()?;
    let agent = Arc::new(
        StreamingAgent::new(provider)
            .tool(BashTool::new())
            .tool(ReadTool::new())
            .tool(WriteTool::new())
            .tool(EditTool::new()),
    );

    let stdin = io::stdin();
    let mut history: Vec<Message> = Vec::new();
    println!();

    loop {
        print!("{BOLD_CYAN}❯{RESET} ");
        io::stdout().flush()?;

        let mut line = String::new();
        if stdin.lock().read_line(&mut line)? == 0 {
            println!();
            break;
        }
        let prompt = line.trim().to_string();
        if prompt.is_empty() {
            continue;
        }
        println!();

        // Append user message and spawn streaming agent task
        history.push(Message::User(prompt));
        let (tx, mut rx) = mpsc::unbounded_channel();
        let agent = agent.clone();
        let mut msgs = std::mem::take(&mut history);
        let handle = tokio::spawn(async move {
            let _ = agent.chat(&mut msgs, tx).await;
            msgs
        });

        // UI event loop
        let mut tick = tokio::time::interval(Duration::from_millis(80));
        let mut frame = 0usize;
        let mut tool_count = 0usize;
        let mut streaming_text = false;
        const COLLAPSE_AFTER: usize = 3;

        // Initial spinner
        print!(
            "{BOLD_MAGENTA}⏺{RESET} {YELLOW}{} Thinking...{RESET}",
            SPINNER[0]
        );
        let _ = io::stdout().flush();

        loop {
            tokio::select! {
                event = rx.recv() => {
                    match event {
                        Some(AgentEvent::TextDelta(text)) => {
                            if !streaming_text {
                                print!("{CLEAR_LINE}");
                                streaming_text = true;
                            }
                            print!("{text}");
                            let _ = io::stdout().flush();
                        }
                        Some(AgentEvent::ToolCall { summary, .. }) => {
                            tool_count += 1;
                            streaming_text = false;

                            if tool_count <= COLLAPSE_AFTER {
                                print!("{CLEAR_LINE}  {DIM}⎿  {summary}{RESET}\n");
                            } else if tool_count == COLLAPSE_AFTER + 1 {
                                print!("{CLEAR_LINE}  {DIM}⎿  ... and 1 more{RESET}\n");
                            } else {
                                let extra = tool_count - COLLAPSE_AFTER;
                                print!("{CLEAR_LINE}\x1b[A{CLEAR_LINE}  {DIM}⎿  ... and {extra} more{RESET}\n");
                            }

                            let ch = SPINNER[frame % SPINNER.len()];
                            print!("{BOLD_MAGENTA}⏺{RESET} {YELLOW}{ch} Thinking...{RESET}");
                            let _ = io::stdout().flush();
                        }
                        Some(AgentEvent::Done(_)) => {
                            if streaming_text {
                                println!("\n");
                            } else {
                                print!("{CLEAR_LINE}");
                                let _ = io::stdout().flush();
                                println!();
                            }
                            break;
                        }
                        Some(AgentEvent::Error(e)) => {
                            print!("{CLEAR_LINE}");
                            let _ = io::stdout().flush();
                            if tool_count > 0 { println!(); }
                            println!("{BOLD_MAGENTA}⏺{RESET} {RED}error: {e}{RESET}\n");
                            break;
                        }
                        None => {
                            print!("{CLEAR_LINE}");
                            let _ = io::stdout().flush();
                            break;
                        }
                    }
                }
                _ = tick.tick() => {
                    if !streaming_text {
                        frame += 1;
                        let ch = SPINNER[frame % SPINNER.len()];
                        print!("\r{BOLD_MAGENTA}⏺{RESET} {YELLOW}{ch} Thinking...{RESET}");
                        let _ = io::stdout().flush();
                    }
                }
            }
        }

        // Recover conversation history from the agent task
        if let Ok(msgs) = handle.await {
            history = msgs;
        }
    }

    Ok(())
}

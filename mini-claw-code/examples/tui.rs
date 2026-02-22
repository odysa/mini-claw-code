use std::io::{self, BufRead, Write};
use std::sync::Arc;
use std::time::Duration;

use mini_claw_code::{
    AgentEvent, AskTool, BashTool, ChannelInputHandler, EditTool, Message, OpenRouterProvider,
    PlanAgent, ReadTool, UserInputRequest, WriteTool,
};
use tokio::sync::mpsc;

const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

// ANSI helpers
const BOLD_CYAN: &str = "\x1b[1;36m";
const BOLD_MAGENTA: &str = "\x1b[1;35m";
const BOLD_GREEN: &str = "\x1b[1;32m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";
const CLEAR_LINE: &str = "\x1b[2K\r";

/// Present options as an arrow-key-navigable list using crossterm raw mode.
///
/// Returns the selected option string, or switches to free-text if the user
/// types any letter.
fn select_option(question: &str, options: &[String]) -> io::Result<String> {
    use crossterm::{
        cursor,
        event::{self, Event, KeyCode, KeyEvent},
        terminal,
    };

    terminal::enable_raw_mode()?;

    let mut selected: usize = 0;
    let mut stdout = io::stdout();

    // Draw initial list
    write!(stdout, "\r\n  {BOLD_CYAN}{question}{RESET}\r\n")?;
    for (i, opt) in options.iter().enumerate() {
        if i == selected {
            write!(stdout, "  {BOLD_CYAN}> {opt}{RESET}\r\n")?;
        } else {
            write!(stdout, "    {opt}\r\n")?;
        }
    }
    stdout.flush()?;

    loop {
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(KeyEvent { code, .. }) = event::read()? {
                match code {
                    KeyCode::Up => {
                        if selected > 0 {
                            selected -= 1;
                        }
                    }
                    KeyCode::Down => {
                        if selected + 1 < options.len() {
                            selected += 1;
                        }
                    }
                    KeyCode::Enter => {
                        terminal::disable_raw_mode()?;
                        // Move past the list
                        write!(
                            stdout,
                            "\r{CLEAR_LINE}  {DIM}> {}{RESET}\r\n",
                            options[selected]
                        )?;
                        stdout.flush()?;
                        return Ok(options[selected].clone());
                    }
                    KeyCode::Char(_) => {
                        // Switch to free-text mode
                        terminal::disable_raw_mode()?;
                        write!(stdout, "\r{CLEAR_LINE}  > ")?;
                        stdout.flush()?;
                        let mut line = String::new();
                        io::stdin().lock().read_line(&mut line)?;
                        return Ok(line.trim().to_string());
                    }
                    KeyCode::Esc => {
                        terminal::disable_raw_mode()?;
                        return Ok(options[selected].clone());
                    }
                    _ => {}
                }

                // Redraw list — move cursor up to start of list
                write!(stdout, "{}", cursor::MoveUp(options.len() as u16))?;
                for (i, opt) in options.iter().enumerate() {
                    if i == selected {
                        write!(stdout, "\r{CLEAR_LINE}  {BOLD_CYAN}> {opt}{RESET}\r\n")?;
                    } else {
                        write!(stdout, "\r{CLEAR_LINE}    {opt}\r\n")?;
                    }
                }
                stdout.flush()?;
            }
        }
    }
}

/// Handle a UserInputRequest: either show arrow-key selection or a simple prompt.
fn handle_input_request(req: UserInputRequest) {
    let answer = if req.options.is_empty() {
        // Simple text prompt
        print!("\n  {BOLD_CYAN}{}{RESET}\n  > ", req.question);
        let _ = io::stdout().flush();
        let mut line = String::new();
        let _ = io::stdin().lock().read_line(&mut line);
        line.trim().to_string()
    } else {
        // Arrow-key selection
        match select_option(&req.question, &req.options) {
            Ok(s) => s,
            Err(_) => req.options.first().cloned().unwrap_or_default(),
        }
    };
    let _ = req.response_tx.send(answer);
}

/// Run the streaming UI event loop: spinner, tool calls, streamed text.
/// Returns when `Done` or `Error` is received.
async fn ui_event_loop(
    rx: &mut mpsc::UnboundedReceiver<AgentEvent>,
    input_rx: &mut mpsc::UnboundedReceiver<UserInputRequest>,
    spinner_label: &str,
) {
    let mut tick = tokio::time::interval(Duration::from_millis(80));
    let mut frame = 0usize;
    let mut tool_count = 0usize;
    let mut streaming_text = false;
    let mut text_buf = String::new();
    const COLLAPSE_AFTER: usize = 3;

    print!(
        "{BOLD_MAGENTA}⏺{RESET} {YELLOW}{} {spinner_label}{RESET}",
        SPINNER[0]
    );
    let _ = io::stdout().flush();

    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Some(AgentEvent::TextDelta(text)) => {
                        streaming_text = true;
                        text_buf.push_str(&text);
                    }
                    Some(AgentEvent::ToolCall { summary, .. }) => {
                        tool_count += 1;
                        streaming_text = false;
                        text_buf.clear();

                        if tool_count <= COLLAPSE_AFTER {
                            print!("{CLEAR_LINE}  {DIM}⎿  {summary}{RESET}\n");
                        } else if tool_count == COLLAPSE_AFTER + 1 {
                            print!("{CLEAR_LINE}  {DIM}⎿  ... and 1 more{RESET}\n");
                        } else {
                            let extra = tool_count - COLLAPSE_AFTER;
                            print!("{CLEAR_LINE}\x1b[A{CLEAR_LINE}  {DIM}⎿  ... and {extra} more{RESET}\n");
                        }

                        let ch = SPINNER[frame % SPINNER.len()];
                        print!("{BOLD_MAGENTA}⏺{RESET} {YELLOW}{ch} {spinner_label}{RESET}");
                        let _ = io::stdout().flush();
                    }
                    Some(AgentEvent::Done(_)) => {
                        print!("{CLEAR_LINE}");
                        let _ = io::stdout().flush();
                        if !text_buf.is_empty() {
                            let rendered = termimad::text(&text_buf);
                            print!("{rendered}\n");
                        } else {
                            println!();
                        }
                        return;
                    }
                    Some(AgentEvent::Error(e)) => {
                        print!("{CLEAR_LINE}");
                        let _ = io::stdout().flush();
                        if tool_count > 0 { println!(); }
                        println!("{BOLD_MAGENTA}⏺{RESET} {RED}error: {e}{RESET}\n");
                        return;
                    }
                    None => {
                        print!("{CLEAR_LINE}");
                        let _ = io::stdout().flush();
                        return;
                    }
                }
            }
            Some(req) = input_rx.recv() => {
                print!("{CLEAR_LINE}");
                let _ = io::stdout().flush();
                streaming_text = false;

                tokio::task::spawn_blocking(move || handle_input_request(req))
                    .await
                    .ok();

                let ch = SPINNER[frame % SPINNER.len()];
                print!("{BOLD_MAGENTA}⏺{RESET} {YELLOW}{ch} {spinner_label}{RESET}");
                let _ = io::stdout().flush();
            }
            _ = tick.tick() => {
                if !streaming_text {
                    frame += 1;
                    let ch = SPINNER[frame % SPINNER.len()];
                    print!("\r{BOLD_MAGENTA}⏺{RESET} {YELLOW}{ch} {spinner_label}{RESET}");
                    let _ = io::stdout().flush();
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let provider = OpenRouterProvider::from_env_with_model("anthropic/claude-opus-4-5")?;

    // Channel for AskTool → TUI communication
    let (input_tx, mut input_rx) = mpsc::unbounded_channel::<UserInputRequest>();
    let handler = Arc::new(ChannelInputHandler::new(input_tx));

    let agent = Arc::new(
        PlanAgent::new(provider)
            .tool(BashTool::new())
            .tool(ReadTool::new())
            .tool(WriteTool::new())
            .tool(EditTool::new())
            .tool(AskTool::new(handler)),
    );

    let stdin = io::stdin();
    let mut history: Vec<Message> = Vec::new();
    let mut plan_mode = false;
    println!();

    loop {
        if plan_mode {
            print!("{BOLD_GREEN}[plan]{RESET} {BOLD_CYAN}>{RESET} ");
        } else {
            print!("{BOLD_CYAN}>{RESET} ");
        }
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

        // Toggle plan mode
        if prompt == "/plan" {
            plan_mode = !plan_mode;
            if plan_mode {
                println!("  {BOLD_GREEN}Plan mode ON{RESET} — agent will plan before executing.\n");
            } else {
                println!("  {DIM}Plan mode OFF{RESET} — agent executes directly.\n");
            }
            continue;
        }

        println!();
        history.push(Message::User(prompt));

        if plan_mode {
            // ---- PLAN → APPROVE → EXECUTE ----
            loop {
                // Plan phase
                let (tx, mut rx) = mpsc::unbounded_channel();
                let agent_clone = agent.clone();
                let mut msgs = std::mem::take(&mut history);
                let handle = tokio::spawn(async move {
                    let result = agent_clone.plan(&mut msgs, tx).await;
                    (msgs, result)
                });

                ui_event_loop(&mut rx, &mut input_rx, "Planning...").await;

                let (msgs, plan_result) = handle.await?;
                history = msgs;

                if plan_result.is_err() {
                    break; // error already shown by ui_event_loop
                }

                // Approval prompt
                print!("  {BOLD_GREEN}Accept this plan?{RESET} {DIM}[y/n/feedback]{RESET} ");
                io::stdout().flush()?;

                let mut response = String::new();
                stdin.lock().read_line(&mut response)?;
                let response = response.trim().to_string();
                println!();

                if response.is_empty() || response.eq_ignore_ascii_case("y") {
                    // Execute phase
                    history.push(Message::User("Approved. Execute the plan.".into()));

                    let (tx2, mut rx2) = mpsc::unbounded_channel();
                    let agent_clone = agent.clone();
                    let mut msgs = std::mem::take(&mut history);
                    let handle2 = tokio::spawn(async move {
                        let _ = agent_clone.execute(&mut msgs, tx2).await;
                        msgs
                    });

                    ui_event_loop(&mut rx2, &mut input_rx, "Executing...").await;

                    if let Ok(msgs) = handle2.await {
                        history = msgs;
                    }
                    break;
                } else if response.eq_ignore_ascii_case("n") {
                    history.push(Message::User(
                        "Rejected. I'll give you new instructions.".into(),
                    ));
                    break;
                } else {
                    // Feedback → re-plan
                    history.push(Message::User(response));
                    // loop continues → calls plan() again
                }
            }
        } else {
            // ---- NORMAL MODE (execute directly) ----
            let (tx, mut rx) = mpsc::unbounded_channel();
            let agent_clone = agent.clone();
            let mut msgs = std::mem::take(&mut history);
            let handle = tokio::spawn(async move {
                let _ = agent_clone.execute(&mut msgs, tx).await;
                msgs
            });

            ui_event_loop(&mut rx, &mut input_rx, "Thinking...").await;

            if let Ok(msgs) = handle.await {
                history = msgs;
            }
        }
    }

    Ok(())
}

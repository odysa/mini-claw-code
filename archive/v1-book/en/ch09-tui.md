# Chapter 9: A Better TUI

The `chat.rs` CLI works, but it dumps plain text and shows every tool call. A
real coding agent deserves markdown rendering, a thinking spinner, and
collapsed tool calls when the agent gets busy.

See `mini-claw-code/examples/tui.rs` for a reference implementation. It uses:

- **`termimad`** for inline markdown rendering in the terminal.
- **`crossterm`** for raw terminal mode (used by the arrow-key selection UI in
  Chapter 11).
- An **animated spinner** (`⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`) that ticks while the agent thinks.
- **Collapsed tool calls**: after 3 tool calls, subsequent ones are collapsed
  into a `... and N more` counter to keep the output clean.

The TUI builds on the `AgentEvent` stream from `StreamingAgent` (Chapter 10).
The event loop uses `tokio::select!` to multiplex three sources:

1. **Agent events** (`AgentEvent::TextDelta`, `ToolCall`, `Done`, `Error`) --
   render streaming text, tool summaries, or final output.
2. **User input requests** from `AskTool` (Chapter 11) -- pause the spinner
   and show a text prompt or arrow-key selection list.
3. **Timer ticks** -- advance the spinner animation.

This chapter is exposition only -- no code to write. Read through
`examples/tui.rs` to see how the pieces fit together, or ask your mini-claw-code
agent to build a TUI for you.

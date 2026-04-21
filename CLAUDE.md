# CLAUDE.md

Guidance for Claude Code working in this repository.

## Overview

Tutorial project for building a mini coding agent in Rust. `mini-claw-code` is the reference implementation; `mini-claw-code-starter` is the fill-in-the-blanks template used by the 18-chapter mdBook in `mini-claw-code-book` (zh translation in `mini-claw-code-book-zh`). The 15-chapter v1 book is archived under `archive/v1-book/`.

## Workspace (4 crates)

- **mini-claw-code** — Reference lib (V2). Agent loop, streaming, planning, subagent, tools (bash/read/write/edit/ask), OpenRouter + mock providers, MCP client, hooks, permissions, safety, config, context manager, instructions loader.
- **mini-claw-code-starter** — Mirror of the above with stubbed bodies for learners.
- **mini-claw-code-xtask** — Build automation behind `cargo x <cmd>` (`solution-check`, `check`, `book`).
- **claw-code** — Earlier V2 scaffold, partially stale. Directory-per-module layout with unfinished stubs (`session/`, `tui/`, `mcp/`, `context/`). Has a few pieces not in `mini-claw-code` (glob/grep tools, `engine/query`, structured `prompt/`, richer `permission/`). Not used by the current book.

## Commands

```bash
cargo test -p mini-claw-code             # all reference tests
cargo test -p mini-claw-code test_read_  # filter by name
cargo x solution-check                   # fmt + clippy + tests on reference
cargo x check                            # same on starter
cargo x book                             # serve EN + 中文 at localhost:3000
```

## Architecture (`mini-claw-code/src/`)

- `types.rs` — `Provider`, `Tool`, `Message`, `StopReason` (core protocol).
- `agent.rs` — `single_turn()` and `SimpleAgent<P: Provider>` (loop until `StopReason::Stop`).
- `tools/` — `BashTool`, `ReadTool`, `WriteTool`, `EditTool`, `AskTool`.
- `providers/openrouter.rs` — OpenAI-compatible HTTP (reads `OPENROUTER_API_KEY` via `dotenvy`).
- `mock.rs` — `MockProvider` returning scripted responses for tests.
- Other modules: `streaming`, `planning`, `subagent`, `hooks`, `permissions`, `safety`, `mcp/`, `config`, `context`, `instructions`, `usage`.

## Testing

`mini-claw-code/src/tests/` is organized by feature (`read.rs`, `bash.rs`, `streaming.rs`, `plan_agent.rs`, …). The starter carries a reduced subset for the hands-on chapters. `claw-code/` has its own legacy `ch1.rs`–`ch15.rs` tests aligned with the archived v1 book.

## Environment

`.env` with `OPENROUTER_API_KEY` is required only for the live provider. Example CLI at `mini-claw-code/examples/chat.rs`.

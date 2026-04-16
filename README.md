<p align="center">
  <img src="docs/banner.png" alt="Mini Claw Code banner" width="500">
</p>

<h1 align="center">Mini Claw Code</h1>

<p align="center">
  <strong>Build your own coding agent from scratch in Rust.</strong>
</p>

<p align="center">
  <a href="https://odysa.github.io/mini-claw-code/">Read the Book</a> &middot;
  <a href="#quick-start">Quick Start</a> &middot;
  <a href="#chapter-roadmap">Chapters</a>
</p>

<p align="center">
  English | <a href="README.zh.md">中文</a>
</p>

---

> **V2 is in progress!** A new deep-dive series — [**Claude Code Architecture in Rust**](https://odysa.github.io/mini-claw-code/en/architecture/ch00-overview.html) — reverse-engineers how [Claude Code](https://claude.ai/code) actually works: streaming, permissions, hooks, MCP, subagents, and more. 26 chapters, built progressively from your first LLM response to a production-grade agent. V2 chapters are being actively developed — some chapters may be incomplete or subject to change. The original hands-on tutorial (V1) is still available below.

---

You use coding agents every day. Ever wonder how they actually work?

<p align="center">
  <img src="docs/demo.gif" alt="mini-claw-code running in a terminal" width="700">
</p>

It's simpler than you think. Strip away the UI, the streaming, the model routing — and every coding agent is just this loop:

```
loop:
    response = llm(messages, tools)
    if response.done:
        break
    for call in response.tool_calls:
        result = execute(call)
        messages.append(result)
```

The LLM never touches your filesystem. It *asks* your code to run tools — read a file, execute a command, edit code — and your code *does*. That loop is the entire idea.

This tutorial builds that loop from scratch. **15 chapters. Test-driven. No magic.**

```mermaid
flowchart LR
    U["You: 'Summarize doc.pdf'"] --> A["Your Agent"]
    A <-->|prompt + tool defs| LLM
    LLM -->|"tool_call: read('doc.pdf')"| A
    A --> T["Tools"]
    T --> A
    A -->|final answer| U

    subgraph T["Tools"]
        direction TB
        bash["bash"]
        read["read"]
        write["write"]
        edit["edit"]
    end
```

## What you'll build

A working coding agent that can:

- **Run shell commands** — `ls`, `grep`, `git`, anything
- **Read and write files** — full filesystem access
- **Edit code** — surgical find-and-replace
- **Talk to real LLMs** — via OpenRouter (free tier available, no credit card)
- **Stream responses** — SSE parsing, token-by-token output
- **Ask clarifying questions** — interactive user input mid-task
- **Plan before acting** — read-only planning with approval gating

All test-driven. No API key needed until Chapter 6 — and even then, the default model is free.

## The core loop

Every coding agent — yours included — runs on this:

```mermaid
flowchart TD
    A["User prompt"] --> B["LLM"]
    B -->|"StopReason::Stop"| C["Return text"]
    B -->|"StopReason::ToolUse"| D["Execute tool calls"]
    D -->|"feed results back"| B
```

Match on `StopReason`. Follow instructions. That's the architecture.

## Chapter roadmap

**Part I — Build it yourself** (hands-on, test-driven)

| Ch | You build | What clicks |
|----|-----------|-------------|
| 1 | `MockProvider` | The protocol: messages in, tool calls out |
| 2 | `ReadTool` | The `Tool` trait — every tool is this pattern |
| 3 | `single_turn()` | Match on `StopReason` — the LLM tells you what to do |
| 4 | Bash, Write, Edit | Repetition locks it in |
| 5 | `SimpleAgent` | The loop — single_turn generalized into a real agent |
| 6 | `OpenRouterProvider` | HTTP to a real LLM (OpenAI-compatible API) |
| 7 | CLI chat app | Wire it all together in ~15 lines |

**Part II — The Singularity** (your agent codes itself now)

| Ch | Topic | What it adds |
|----|-------|--------------|
| 8 | The Singularity | Your agent can edit its own source code |
| 9 | A Better TUI | Markdown rendering, spinners, collapsed tool calls |
| 10 | Streaming | `StreamingAgent` with SSE parsing and `AgentEvent`s |
| 11 | User Input | `AskTool` — let the LLM ask *you* questions |
| 12 | Plan Mode | Read-only planning with approval gating |
| 13 | Subagents | Spawn child agents for subtasks via `SubagentTool` |
| 14 | MCP | *coming soon* |
| 15 | Safety Rails | *coming soon* |

```mermaid
flowchart LR
    C1["1\nTypes"] --> C2["2\nTool"]
    C2 --> C3["3\nTurn"]
    C3 --> C4["4\nTools"]
    C4 --> C5["5\nAgent"]
    C5 --> C6["6\nHTTP"]
    C6 --> C7["7\nCLI"]
    C7 --> C8["8+\nExtensions"]

    style C1 fill:#2d333b,stroke:#539bf5,color:#adbac7
    style C2 fill:#2d333b,stroke:#539bf5,color:#adbac7
    style C3 fill:#2d333b,stroke:#539bf5,color:#adbac7
    style C4 fill:#2d333b,stroke:#539bf5,color:#adbac7
    style C5 fill:#2d333b,stroke:#539bf5,color:#adbac7
    style C6 fill:#2d333b,stroke:#539bf5,color:#adbac7
    style C7 fill:#2d333b,stroke:#539bf5,color:#adbac7
    style C8 fill:#2d333b,stroke:#e5534b,color:#adbac7
```

## Safety warning

This agent has **unrestricted shell access**. The `BashTool` passes LLM-generated commands directly to `bash -c` with no sandboxing, filtering, or timeout. The `ReadTool` and `WriteTool` can access any file your user account can. This is intentional for a learning project, but:

- **Do not run this agent on untrusted prompts or files** (prompt injection via file contents can execute arbitrary commands).
- **Do not run this on a machine with sensitive data** without understanding the risks.
- See Chapter 15 (coming soon) for how to add safety rails.

## Quick start

```bash
git clone https://github.com/odysa/mini-claw-code.git
cd mini-claw-code
cargo build
```

Start the tutorial book:

```bash
cargo install mdbook mdbook-mermaid   # one-time
cargo x book                          # opens at localhost:3000
```

Or read it online at **[odysa.github.io/mini-claw-code](https://odysa.github.io/mini-claw-code/)**.

## The workflow

Every chapter follows the same rhythm:

1. **Read** the chapter
2. **Open** the matching file in `mini-claw-code-starter/src/`
3. **Replace** `unimplemented!()` with your code
4. **Run** `cargo test -p mini-claw-code-starter chN`

Green tests = you got it.

## Project structure

```
mini-claw-code-starter/     <- YOUR code (fill in the stubs)
mini-claw-code/             <- Reference solution (no peeking!)
mini-claw-code-book/        <- The tutorial (15 chapters)
mini-claw-code-xtask/       <- Helper commands (cargo x ...)
```

## Prerequisites

- **Rust 1.85+** — [rustup.rs](https://rustup.rs)
- Basic Rust knowledge (ownership, enums, `Result`/`Option`)
- No API key until Chapter 6

## Commands

```bash
cargo test -p mini-claw-code-starter ch1    # test one chapter
cargo test -p mini-claw-code-starter        # test everything
cargo x check                               # fmt + clippy + tests
cargo x book                                # serve the tutorial
```

## License

MIT

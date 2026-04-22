# Overview

Welcome to **Building a Coding Agent in Rust** -- a hands-on tutorial where you build your own AI coding agent from scratch in the `mini-claw-code-starter` template, guided by the architecture of [Claude Code](https://claude.ai/code).

> Looking for the original V1 hands-on tutorial? It's archived at [archive/v1-book/en/](https://github.com/odysa/mini-claw-code/tree/main/archive/v1-book/en) (Chinese translation at [archive/v1-book/zh/](https://github.com/odysa/mini-claw-code/tree/main/archive/v1-book/zh)).

## What you'll build

By the end of this book, you'll have built a complete coding agent that:

- **Connects to an LLM** via an OpenAI-compatible HTTP provider
- **Uses tools** -- bash, file read/write/edit -- with a simple `Tool` trait
- **Loops autonomously** -- the `SimpleAgent` drives the provider-tool cycle until done
- **Streams events** through channels so a UI can show progress in real-time
- **Tests deterministically** with a `MockProvider` that returns canned responses
- **Enforces safety** with a permission engine, safety checks, and hooks
- **Loads project instructions** from CLAUDE.md files and layered config

## Architecture

The starter codebase uses a flat module layout:

```
mini-claw-code-starter/src/
  types.rs          -- Messages, tools, ToolSet, Provider trait, TokenUsage
  agent.rs          -- SimpleAgent (the core agent loop) and AgentEvent
  mock.rs           -- MockProvider for deterministic testing
  streaming.rs      -- SSE parsing, StreamAccumulator
  instructions.rs   -- InstructionLoader (CLAUDE.md discovery)
  permissions.rs    -- PermissionEngine
  safety.rs         -- SafetyChecker, SafeToolWrapper
  hooks.rs          -- Hook trait, HookRegistry
  planning.rs       -- PlanAgent (two-phase plan/execute)
  config.rs         -- Config, ConfigLoader, CostTracker
  context.rs        -- SystemPromptBuilder
  providers/
    openrouter.rs   -- OpenRouterProvider (real HTTP backend)
  tools/            -- Tool implementations (bash, file read/write/edit)
```

## How to use this book

**Start with Chapters 1-3.** Three short, hands-on chapters get you from zero to a working agent in under an hour:

1. [**Your First LLM Call**](./ch01-first-llm-call.md) — implement `MockProvider` (`test_mock_`)
2. [**Your First Tool Call**](./ch02-first-tool.md) — implement `ReadTool` (`test_read_`)
3. [**The Agentic Loop**](./ch03-agentic-loop.md) — implement `single_turn` and `SimpleAgent` (`test_single_turn_`, `test_simple_agent_`)

Then continue with Chapters 4-18 for the full architecture: streaming, permissions, hooks, plan mode, configuration, and more.

The `mini-claw-code-starter` crate contains stub implementations with `unimplemented!()` markers and doc comments describing what to do. Read the chapter, fill in the stubs, then verify your work by running the tests.

**Run tests to check your progress:**

```bash
# Run tests for a specific chapter (use the correct test name from the table below)
cargo test -p mini-claw-code-starter test_mock_

# Run all tests
cargo test -p mini-claw-code-starter
```

## Prerequisites

- Rust (edition 2024, 1.85+)
- Basic familiarity with async Rust (`async`/`await`, `tokio`)
- An OpenRouter API key (for the live provider chapters)

## Chapter roadmap

### Getting Started

| Chapter | Topic | File(s) to edit | Test command |
|---------|-------|-----------------|--------------|
| 1 | Your First LLM Call | `src/mock.rs` | `test_mock_` |
| 2 | Your First Tool Call | `src/tools/read.rs` | `test_read_` |
| 3 | The Agentic Loop | `src/agent.rs` | `test_single_turn_`, `test_simple_agent_` |

### Part I: Core Agent

| Chapter | Topic | File(s) to edit | Test command |
|---------|-------|-----------------|--------------|
| 4 | Messages & Types | `src/types.rs` (pre-filled) | `test_mock_` |
| 5a | Provider & Streaming Foundations | `src/mock.rs`, `src/streaming.rs` | `test_mock_`, `test_streaming_parse_`, `test_streaming_accumulator_` |
| 5b | OpenRouter & StreamingAgent | `src/providers/openrouter.rs`, `src/streaming.rs` | `test_openrouter_`, `test_streaming_stream_chat_`, `test_streaming_streaming_agent_` |
| 6 | Tool Interface | `src/tools/read.rs` (already done in Ch2 — re-read) | `test_read_` |
| 7 | The Agentic Loop (Deep Dive) | `src/agent.rs` (already done in Ch3 — re-read) | `test_single_turn_`, `test_simple_agent_` |

### Part II: Prompt & Tools

| Chapter | Topic | File(s) to edit | Test command |
|---------|-------|-----------------|--------------|
| 8 | System Prompt | `src/instructions.rs` | `instructions` |
| 9 | File Tools | `src/tools/write.rs`, `src/tools/edit.rs` (read.rs already done in Ch2) | `test_read_`, `test_write_`, `test_edit_` |
| 10 | Bash Tool | `src/tools/bash.rs` | `test_bash_` |
| 11 | Search Tools | (extension -- no stubs) | (no tests) |
| 12 | Tool Registry | `src/types.rs` (ToolSet — pre-filled, re-read) | `test_multi_tool_` |

### Part III: Safety & Control

| Chapter | Topic | File(s) to edit | Test command |
|---------|-------|-----------------|--------------|
| 13 | Permission Engine | `src/permissions.rs` | `permissions` |
| 14 | Safety Checks | `src/safety.rs` | `safety` |
| 15 | Hooks | `src/hooks.rs` | `hooks` |
| 16 | Plan Mode | `src/planning.rs` | `plan` |

### Part IV: Configuration

| Chapter | Topic | File(s) to edit | Test command |
|---------|-------|-----------------|--------------|
| 17 | Settings Hierarchy | `src/config.rs`, `src/usage.rs` | `config`, `cost_tracker` |
| 18 | Project Instructions | `src/instructions.rs`, `src/context.rs` | `instructions`, `context_manager` |

### Part V: Extensions

These chapters build opt-in capabilities on top of the core agent. Their tests
are marked `#[ignore]` in the starter because they depend on the rest of the
book being implemented first; run them with `--ignored`.

| Chapter | Topic | File(s) to edit | Test command |
|---------|-------|-----------------|--------------|
| 19 | AskTool (user input) | `src/tools/ask.rs` | `test_ask_` (run with `--ignored`) |
| 20 | Subagents | `src/subagent.rs` | `test_subagent_` (run with `--ignored`) |

### Bonus (no chapter yet -- stubs + tests available)

| Topic | File to edit | Test command |
|-------|-------------|--------------|
| Interactive CLI | `examples/chat.rs` | `cargo run --example chat` (after stub is filled in) |

Let's start building.

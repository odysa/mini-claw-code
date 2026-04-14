# Overview

Welcome to **Building a Coding Agent in Rust** -- a hands-on tutorial where you build your own AI coding agent from scratch in the `mini-claw-code-starter` template, guided by the architecture of [Claude Code](https://claude.ai/code).

## What you'll build

By the end of this book, you'll have built a complete coding agent that:

- **Connects to an LLM** via an OpenRouter-compatible HTTP provider
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

**Start with the Getting Started section.** Three short, hands-on chapters get you from zero to a working agent in under an hour:

1. [**Your First LLM Call**](./intro01-first-llm-call.md) — implement `MockProvider` (`test_ch1`)
2. [**Your First Tool Call**](./intro02-first-tool.md) — implement `ReadTool` (`test_ch2`)
3. [**The Agentic Loop**](./intro03-agentic-loop.md) — implement `single_turn` and `SimpleAgent` (`test_ch3`, `test_ch5`)

Then dive into the **Deep Dive** chapters (1-15) for the full architecture: streaming, permissions, hooks, plan mode, configuration, and more.

The `mini-claw-code-starter` crate contains stub implementations with `unimplemented!()` markers and doc comments describing what to do. Read the chapter, fill in the stubs, then verify your work by running the tests.

**Important: Deep dive chapter numbers do NOT match test file numbers.** The chapters were reorganized by topic, but test files kept their original numbering. Use the mapping table below to find the correct test command for each chapter.

**Run tests to check your progress:**

```bash
# Run tests for a specific chapter (use the correct test name from the table below)
cargo test -p mini-claw-code-starter test_ch1

# Run all tests
cargo test -p mini-claw-code-starter
```

## Prerequisites

- Rust (edition 2024, 1.85+)
- Basic familiarity with async Rust (`async`/`await`, `tokio`)
- An OpenRouter API key (for the live provider chapters)

## Chapter roadmap

### Part I: Core Agent

| Chapter | Topic | File(s) to edit | Test command |
|---------|-------|-----------------|--------------|
| 1 | Messages & Types | `src/types.rs` (pre-filled) | `test_ch1` |
| 2 | Provider & Streaming | `src/mock.rs`, `src/streaming.rs`, `src/providers/openrouter.rs` | `test_ch1`, `test_ch6`, `test_ch10` |
| 3 | Tool Interface | `src/tools/read.rs` | `test_ch2` |
| 4 | The Agentic Loop | `src/agent.rs` | `test_ch3`, `test_ch5` |

### Part II: Prompt & Tools

| Chapter | Topic | File(s) to edit | Test command |
|---------|-------|-----------------|--------------|
| 5 | System Prompt | `src/instructions.rs` | `test_ch17` |
| 6 | File Tools | `src/tools/read.rs`, `src/tools/write.rs`, `src/tools/edit.rs` | `test_ch2`, `test_ch4` |
| 7 | Bash Tool | `src/tools/bash.rs` | `test_ch4` |
| 8 | Search Tools | (extension -- no stubs) | (no tests) |
| 9 | Tool Registry | `src/types.rs` (ToolSet) | `test_ch7` |

### Part III: Safety & Control

| Chapter | Topic | File(s) to edit | Test command |
|---------|-------|-----------------|--------------|
| 10 | Permission Engine | `src/permissions.rs` | `test_ch19` |
| 11 | Safety Checks | `src/safety.rs` | `test_ch18` |
| 12 | Hooks | `src/hooks.rs` | `test_ch20` |
| 13 | Plan Mode | `src/planning.rs` | `test_ch12` |

### Part IV: Configuration

| Chapter | Topic | File(s) to edit | Test command |
|---------|-------|-----------------|--------------|
| 14 | Settings Hierarchy | `src/config.rs`, `src/usage.rs` | `test_ch16`, `test_ch14` |
| 15 | Project Instructions | `src/instructions.rs`, `src/context.rs` | `test_ch17`, `test_ch15` |

Let's start building.

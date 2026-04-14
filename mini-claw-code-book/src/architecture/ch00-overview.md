# Overview

Welcome to **Building a Coding Agent in Rust** — a hands-on tutorial where you build your own AI coding agent from scratch in the `mini-claw-code-starter` template, guided by the architecture of [Claude Code](https://claude.ai/code).

## What you'll build

By the end of this book, you'll have built a complete coding agent that:

- **Connects to an LLM** via an OpenRouter-compatible HTTP provider
- **Uses tools** — bash, file read/write/edit — with a simple `Tool` trait
- **Loops autonomously** — the `SimpleAgent` drives the provider-tool cycle until done
- **Streams events** through channels so a UI can show progress in real-time
- **Tests deterministically** with a `MockProvider` that returns canned responses

## Architecture

The starter codebase uses a flat module layout:

```
mini-claw-code-starter/src/
  types.rs          — Messages, tools, ToolSet, Provider trait, TokenUsage
  agent.rs          — SimpleAgent (the core agent loop) and AgentEvent
  mock.rs           — MockProvider for deterministic testing
  streaming.rs      — SSE parsing, StreamAccumulator
  providers/
    openrouter.rs   — OpenRouterProvider (real HTTP backend)
  tools/            — Tool implementations (bash, file read/write/edit)
```

## How to use this book

Each chapter explains the concepts, walks through the design, and tells you what to fill in. The `mini-claw-code-starter` crate contains stub implementations with `unimplemented!()` markers and doc comments describing what to do. Read the chapter, fill in the stubs, then verify your work by running the tests.

**Run tests to check your progress:**

```bash
# Run tests for a specific chapter
cargo test -p mini-claw-code-starter test_ch1

# Run all tests
cargo test -p mini-claw-code-starter
```

## Prerequisites

- Rust (edition 2024, 1.85+)
- Basic familiarity with async Rust (`async`/`await`, `tokio`)
- An OpenRouter API key (for the live provider chapters)

## Chapter roadmap

| Chapter | What you build |
|---------|---------------|
| **1 — Messages & Types** | The `Message` enum, `ToolDefinition`, `ToolSet`, `TokenUsage` |
| **2 — Provider & Streaming** | `Provider` trait, `MockProvider`, SSE parsing, `OpenRouterProvider` |
| **3 — Tool Interface** | The `Tool` trait, your first concrete tool |
| **4 — The Agent Loop** | `SimpleAgent` with `chat()`, `run()`, `AgentEvent` |

Let's start building.

# Overview

Welcome to **Building a Coding Agent in Rust** — a hands-on tutorial where you build a production-grade AI coding agent from scratch, mirroring the real architecture of [Claude Code](https://claude.ai/code).

## What you'll build

By the end of this book, you'll have built a complete coding agent that:

- **Streams responses** from an LLM in real-time via Server-Sent Events
- **Uses tools** — file read/write/edit, bash, glob, grep — with a rich Tool trait matching Claude Code's interface
- **Validates and gates tool execution** through a multi-stage permission pipeline (allow/deny/ask)
- **Protects the user** with path validation, command filtering, and protected file checks
- **Extends via hooks** — pre/post tool events with shell commands, blocking, and argument modification
- **Loads project context** by discovering and injecting CLAUDE.md files
- **Manages configuration** through a 4-level settings hierarchy (user → project → local → env)
- **Tracks tokens and cost** per model with cumulative session tracking
- **Auto-compacts context** when approaching the token limit, using the LLM to summarize
- **Saves and resumes sessions** via JSONL transcripts
- **Connects to MCP servers** for third-party tool integration over JSON-RPC/stdio
- **Spawns subagents** with isolated message histories and shared providers
- **Coordinates multiple agents** with teams, workers, and shared scratchpads
- **Renders a rich TUI** with streaming text, tool progress, spinners, and permission dialogs

## Architecture

The codebase mirrors Claude Code's module structure:

```
claw-code/src/
  types/          — Messages, tools, permissions, usage
  engine/         — QueryEngine (the core agent loop)
  provider/       — LLM backends (OpenRouter, mock)
  tools/          — Tool implementations (bash, file ops, search)
  permission/     — Permission engine and safety checks
  hooks/          — Event-driven extensibility
  config/         — Layered settings hierarchy
  context/        — Token tracking and compaction
  session/        — Transcript persistence
  mcp/            — Model Context Protocol client
  prompt/         — System prompt builder with modular sections
  agents/         — Subagents and multi-agent coordination
  tui/            — Terminal UI with streaming
```

## How to use this book

Each chapter explains the concepts, walks through the design, and provides complete code listings. The `claw-code/` crate contains the reference implementation. Read the chapter, study the code, then verify your understanding by running the tests.

**Run tests to check your progress:**

```bash
# Run tests for a specific chapter
cargo test -p claw-code test_ch1

# Run all tests
cargo test -p claw-code
```

## Prerequisites

- Rust (edition 2024, 1.85+)
- Basic familiarity with async Rust (`async`/`await`, `tokio`)
- An OpenRouter API key (for the live provider chapters)

## Chapter roadmap

| Section | Chapters | What you build |
|---------|----------|---------------|
| **Your First Agent SDK** | 1-3 | First LLM response, first tool call, the agentic loop |
| **Under the Hood** | 4-5 | Messages & types deep dive, system prompt |
| **Building Your Toolbox** | 6-9 | File tools, bash, glob/grep, tool registry |
| **Safety & Control** | 10-13 | Permissions, safety checks, hooks, plan mode |
| **Configuration** | 14-17 | Settings, CLAUDE.md, memory, token tracking |
| **Context Management** | 18-20 | Compaction, session save/resume |
| **MCP Integration** | 21-22 | MCP protocol and client |
| **Multi-Agent** | 23-26 | Subagents, coordination, user input, TUI |

Let's start building.

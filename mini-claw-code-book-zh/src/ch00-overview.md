# 概述

**用 Rust 构建 Coding Agent** — 动手实践教程，在 `mini-claw-code-starter` 模板里从零搭建自己的 AI coding agent，架构参考 [Claude Code](https://claude.ai/code)。

> 想找最初的 V1 教程？已归档至 [archive/v1-book/en/](https://github.com/odysa/mini-claw-code/tree/main/archive/v1-book/en)（中文版见 [archive/v1-book/zh/](https://github.com/odysa/mini-claw-code/tree/main/archive/v1-book/zh)）。

## 你将构建什么

读完本书，你就有了一个完整的 coding agent，能做到：

- **连接 LLM**，通过兼容 OpenAI 的 HTTP provider
- **调用工具**：bash、文件读/写/编辑，统一用 `Tool` trait 封装
- **自主循环**：`SimpleAgent` 驱动 provider-工具循环，直到任务完成
- **流式推送事件**，通过 channel 让 UI 实时展示进度
- **确定性测试**，用 `MockProvider` 返回预设响应，无需真实 API
- **安全策略**：权限引擎、安全检查、hook 三重保障
- **加载项目说明**，从 CLAUDE.md 和分层配置中读取

## 架构

starter 代码库采用扁平模块结构：

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

## 怎么用这本书

**先看第 1–3 章。** 三章短小精悍，不到一小时就能从零跑起一个 agent：

1. [**第一次 LLM 调用**](./ch01-first-llm-call.md) — 实现 `MockProvider`（`test_mock_`）
2. [**第一次工具调用**](./ch02-first-tool.md) — 实现 `ReadTool`（`test_read_`）
3. [**Agentic 循环**](./ch03-agentic-loop.md) — 实现 `single_turn` 和 `SimpleAgent`（`test_single_turn_`、`test_simple_agent_`）

之后继续第 4–18 章，深入完整架构：流式、权限、hook、计划模式、配置等。

`mini-claw-code-starter` crate 里的 stub 实现带有 `unimplemented!()` 占位和说明注释，告诉你该做什么。读完章节，填好 stub，跑测试验证。

**跑测试检查进度：**

```bash
# 跑某章的测试（用下表对应的测试名称）
cargo test -p mini-claw-code-starter test_mock_

# 跑所有测试
cargo test -p mini-claw-code-starter
```

## 前置条件

- Rust（edition 2024，1.85+）
- 了解 async Rust 基础（`async`/`await`、`tokio`）
- OpenRouter API key（实时 provider 章节需要）

## 章节路线图

### 入门

| 第 N 章 | 主题 | 需编辑的文件 | 测试命令 |
|---------|------|------------|---------|
| 1 | 第一次 LLM 调用 | `src/mock.rs` | `test_mock_` |
| 2 | 第一次工具调用 | `src/tools/read.rs` | `test_read_` |
| 3 | Agentic 循环 | `src/agent.rs` | `test_single_turn_`、`test_simple_agent_` |

### 第一部分：核心 Agent

| 第 N 章 | 主题 | 需编辑的文件 | 测试命令 |
|---------|------|------------|---------|
| 4 | 消息与类型 | `src/types.rs`（已预填）| `test_mock_` |
| 5 | Provider 与流式 | `src/mock.rs`、`src/streaming.rs`、`src/providers/openrouter.rs` | `test_mock_`、`test_openrouter_`、`streaming` |
| 6 | 工具接口 | `src/tools/read.rs`（第 2 章已完成，重新阅读）| `test_read_` |
| 7 | Agentic 循环（深度解析）| `src/agent.rs`（第 3 章已完成，重新阅读）| `test_single_turn_`、`test_simple_agent_` |

### 第二部分：Prompt 与工具

| 第 N 章 | 主题 | 需编辑的文件 | 测试命令 |
|---------|------|------------|---------|
| 8 | 系统 Prompt | `src/instructions.rs` | `instructions` |
| 9 | 文件工具 | `src/tools/write.rs`、`src/tools/edit.rs`（read.rs 第 2 章已完成）| `test_read_`、`test_write_`、`test_edit_` |
| 10 | Bash 工具 | `src/tools/bash.rs` | `test_bash_` |
| 11 | 搜索工具 | （扩展章节，无 stub）| （无测试）|
| 12 | 工具注册表 | `src/types.rs`（ToolSet，已预填，重新阅读）| `test_multi_tool_` |

### 第三部分：安全与控制

| 第 N 章 | 主题 | 需编辑的文件 | 测试命令 |
|---------|------|------------|---------|
| 13 | 权限引擎 | `src/permissions.rs` | `permissions` |
| 14 | 安全检查 | `src/safety.rs` | `safety` |
| 15 | Hook | `src/hooks.rs` | `hooks` |
| 16 | 计划模式 | `src/planning.rs` | `plan` |

### 第四部分：配置

| 第 N 章 | 主题 | 需编辑的文件 | 测试命令 |
|---------|------|------------|---------|
| 17 | 配置层级 | `src/config.rs`、`src/usage.rs` | `config`、`cost_tracker` |
| 18 | 项目说明 | `src/instructions.rs`、`src/context.rs` | `instructions`、`context_manager` |

### 附加内容（暂无章节，stub 与测试已就绪）

| 主题 | 需编辑的文件 | 测试命令 |
|------|------------|---------|
| AskTool（用户输入）| `src/tools/ask.rs` | `ask`（加 `--ignored` 运行）|
| SubagentTool（子 agent）| `src/subagent.rs` | `subagent`（加 `--ignored` 运行）|
| 交互式 CLI | `examples/chat.rs` | `cargo run --example chat`（填好 stub 后运行）|

开始构建。

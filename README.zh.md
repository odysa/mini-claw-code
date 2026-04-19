<p align="center">
  <img src="docs/banner.png" alt="Mini Claw Code banner" width="500">
</p>

<h1 align="center">Mini Claw Code</h1>

<p align="center">
  <strong>用 Rust 从零构建一个编程 agent —— 以 Claude Code 的架构为指引。</strong>
</p>

<p align="center">
  <a href="https://odysa.github.io/mini-claw-code/zh/">阅读中文版</a> &middot;
  <a href="https://odysa.github.io/mini-claw-code/">English</a> &middot;
  <a href="#快速开始">快速开始</a> &middot;
  <a href="#章节路线图">章节</a>
</p>

<p align="center">
  <a href="./README.md">English README</a>
</p>

---

你每天都在使用编程 agent。有没有想过它们究竟是怎么工作的？

<p align="center">
  <img src="docs/demo.gif" alt="mini-claw-code 在终端中运行" width="700">
</p>

其实没你想的那么复杂。剥掉 UI、流式传输、模型路由 —— 每个编程 agent 本质上都是这个循环：

```
loop:
    response = llm(messages, tools)
    if response.done:
        break
    for call in response.tool_calls:
        result = execute(call)
        messages.append(result)
```

LLM 永远不会直接碰你的文件系统。它只是*请求*你的代码去运行工具 —— 读文件、执行命令、编辑代码 —— 然后你的代码*去做*。这个循环就是全部。

本书从零构建这个循环，再把它扩展成一个真实编程 agent 的完整架构：流式传输、权限、hooks、plan 模式、配置等等。**18 章，测试驱动，没有魔法。**

<p align="center">
  <img src="docs/architecture.svg" alt="Mini Claw Code 架构：User ↔ Agent ↔ LLM + Tools" width="900">
</p>

## 你会构建什么

一个能工作的编程 agent，它可以：

- **执行 shell 命令** —— `ls`、`grep`、`git`，任何命令都行
- **读、写、编辑文件** —— 完整的文件系统访问，带外科手术级别的查找替换
- **与真实的 LLM 对话** —— 通过 OpenRouter（有免费套餐，无需信用卡）
- **流式返回响应** —— SSE 解析，token 级输出
- **搜索代码库** —— 用 glob 找文件、用 grep 找内容
- **执行安全约束** —— 权限规则、命令过滤、受保护路径
- **运行用户 hook** —— 工具前后的 shell 命令
- **先规划再行动** —— 两阶段 plan/execute，带审批门控
- **加载项目指令** —— CLAUDE.md 发现、分层配置

全程测试驱动。直到第 5 章才需要 API key —— 而即便到那时，默认模型也是免费的。

## 核心循环

每个编程 agent —— 包括你的 —— 都跑在这个循环上：

<p align="center">
  <img src="docs/core-loop.svg" alt="核心循环：用户 prompt → LLM → StopReason::Stop 或 ToolUse → 回到循环" width="900">
</p>

对 `StopReason` 做模式匹配，按指示行事。这就是整个架构。

## 章节路线图

**入门** —— 一小时内从零跑通一个可工作的 agent

| 章 | 你构建 | 你领悟 |
|----|--------|--------|
| 1 | `MockProvider` | 协议：消息进，工具调用出 |
| 2 | `ReadTool` | `Tool` trait —— 每个工具都是这个模式 |
| 3 | `single_turn()` + `SimpleAgent` | 对 `StopReason` 做模式匹配，再包一层循环 |

**第 I 部分 —— 核心 agent**

| 章 | 主题 | 新增内容 |
|----|------|----------|
| 4 | 消息与类型 | 每个 provider 与工具共用的协议 |
| 5 | Provider 与流式 | `OpenRouterProvider`、SSE 解析、`StreamingAgent` |
| 6 | 工具接口 | 为什么 `Tool` 与 `Provider` 选择了不同的 async 风格 |
| 7 | Agent 循环（深入篇） | `execute_tools`、事件走线、所有权 |

**第 II 部分 —— 提示词与工具**

| 章 | 主题 | 新增内容 |
|----|------|----------|
| 8 | 系统提示词 | 静态身份 + 动态项目上下文 |
| 9 | 文件工具 | `WriteTool`、`EditTool` 及其精确匹配不变式 |
| 10 | Bash 工具 | 用异步 `tokio::process` 捕获 stdout+stderr |
| 11 | 搜索工具 | `GlobTool`、`GrepTool` —— agent 的眼睛 |
| 12 | 工具注册表 | `ToolSet` 查找、为 UI 准备的工具摘要 |

**第 III 部分 —— 安全与控制**

| 章 | 主题 | 新增内容 |
|----|------|----------|
| 13 | 权限引擎 | glob 规则、会话级允许、默认策略 |
| 14 | 安全检查 | 路径校验、命令过滤、受保护文件 |
| 15 | Hooks | 工具前后的 shell 命令，支持 block/modify/continue |
| 16 | Plan 模式 | 两阶段 只读计划 → 审批 → 执行 |

**第 IV 部分 —— 配置**

| 章 | 主题 | 新增内容 |
|----|------|----------|
| 17 | 设置层级 | TOML 分层、环境变量覆盖、`CostTracker` |
| 18 | 项目指令与上下文管理 | CLAUDE.md 发现、`ContextManager` |

<p align="center">
  <img src="docs/roadmap.svg" alt="章节路线图：入门 (1-3) → 核心 (4-7) → 提示词与工具 (8-12) → 安全 (13-16) → 配置 (17-18)" width="900">
</p>

## 安全警告

这个核心 agent 拥有**不受限制的 shell 访问权限**。`BashTool` 会把 LLM 生成的命令直接传给 `bash -c`；`ReadTool`/`WriteTool` 能接触到你账号能接触的任何文件。第 13–16 章才会加上真正的安全护栏。在那之前：

- **不要让 agent 处理不可信的 prompt 或文件内容**（通过文件内容的 prompt 注入可以执行任意命令）。
- **不要在含敏感数据的机器上运行**，除非你完全理解其中的风险。

## 快速开始

```bash
git clone https://github.com/odysa/mini-claw-code.git
cd mini-claw-code
cargo build
```

本地阅读本书：

```bash
cargo install mdbook mdbook-mermaid   # 一次性安装
cargo x book                          # 同时提供中英双语，localhost:3000（右上角切换）
```

或在线阅读：**[odysa.github.io/mini-claw-code/zh/](https://odysa.github.io/mini-claw-code/zh/)**（中文） / **[English](https://odysa.github.io/mini-claw-code/)**。页面右上角的 **EN / 中文** 按钮可随时切换语言。

## 工作流

每个实战章节都遵循相同的节奏：

<p align="center">
  <img src="docs/workflow.svg" alt="工作流：阅读 → 打开 → 替换 → 运行，测试失败就回到循环" width="900">
</p>

1. **阅读**章节 —— 它会告诉你要改哪些文件、运行哪些测试。
2. **打开** `mini-claw-code-starter/src/` 下对应的文件。
3. **替换** `unimplemented!()` 为你的代码。
4. **运行**章节给你的测试命令（例如 `cargo test -p mini-claw-code-starter test_read_`）。

测试变绿 = 你做对了。

> **提醒：** 章节编号和 starter 里的测试文件编号并不对应（章节按主题重新组织过）。每章都会明确告诉你该运行哪个 `test_chN_` 前缀。完整映射见[概览章节](https://odysa.github.io/mini-claw-code/zh/ch00-overview.html)。

## 项目结构

<p align="center">
  <img src="docs/workspace.svg" alt="工作空间：Book、Starter（你写的代码）、Reference、xtask —— 一个 Cargo workspace 下的四个 crate" width="900">
</p>

```
mini-claw-code-starter/     <- 你的代码（在此填空）
mini-claw-code/             <- 参考实现（别偷看！）
mini-claw-code-book/        <- 教程（英文，18 章）
mini-claw-code-book-zh/     <- 教程（中文）
mini-claw-code-xtask/       <- 辅助命令（cargo x ...）
```

## 前置要求

- **Rust 1.85+** —— [rustup.rs](https://rustup.rs)
- 基础的 Rust 知识（所有权、enum、`Result`/`Option`）
- 基础的 async 熟悉度（`async`/`await`、`tokio`）
- 第 5 章之前不需要 API key

## 常用命令

```bash
cargo test -p mini-claw-code-starter test_read_   # 跑单个章节的测试（映射表见书中）
cargo test -p mini-claw-code-starter             # 跑全部测试
cargo x check                                    # fmt + clippy + starter 构建
cargo x book                                     # 在 localhost:3000 同时提供中英双语（中文在 /zh/）
```

## V1 版本？

原版实战教程（15 章，第 I 部分实战 + 第 II 部分扩展）及其中文翻译已归档在 [archive/v1-book/](https://github.com/odysa/mini-claw-code/tree/main/archive/v1-book)。GitHub 原生渲染 markdown —— 从 [archive/v1-book/zh/ch00-overview.md](https://github.com/odysa/mini-claw-code/blob/main/archive/v1-book/zh/ch00-overview.md) 开始。

## 许可证

MIT

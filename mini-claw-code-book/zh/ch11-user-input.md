# 第十一章：用户输入

你的 agent 可以读取文件、运行命令、编写代码——但它无法向*你*提问。如果它不确定该采用哪种方案、操作哪个文件，或者是否要执行一个破坏性操作，它只能靠猜测。

真正的编程 agent 通过 **ask tool（询问工具）** 来解决这个问题。Claude Code 有 `AskUserQuestion`，Kimi CLI 有审批提示。LLM 调用一个特殊工具，agent 暂停执行，用户输入答案。答案作为工具结果返回，执行继续。

在本章中，你将构建：

1. 一个 **`InputHandler` trait**，抽象用户输入的收集方式。
2. 一个 **`AskTool`**，供 LLM 调用来向用户提问。
3. 三种 handler 实现：CLI、基于 channel 的（用于 TUI）以及 mock（用于测试）。

## 为什么需要 trait？

不同的 UI 以不同方式收集输入：

- **CLI** 应用打印到 stdout 并从 stdin 读取。
- **TUI** 应用通过 channel 发送请求，等待事件循环收集答案（可能通过方向键选择）。
- **测试**需要提供预设答案，无需任何 I/O。

`InputHandler` trait 让 `AskTool` 能与这三者配合使用，而不需要知道具体使用的是哪一个：

```rust
#[async_trait::async_trait]
pub trait InputHandler: Send + Sync {
    async fn ask(&self, question: &str, options: &[String]) -> anyhow::Result<String>;
}
```

`question` 是 LLM 想要询问的内容。`options` 切片是一个可选的选项列表——如果为空，用户输入自由文本。如果非空，UI 可以呈现一个选择列表。

## AskTool

`AskTool` 实现了 `Tool` trait。它接收一个 `Arc<dyn InputHandler>`，以便 handler 可以跨线程共享：

```rust
pub struct AskTool {
    definition: ToolDefinition,
    handler: Arc<dyn InputHandler>,
}
```

### 工具定义

LLM 需要知道工具接受哪些参数。`question` 是必需的（字符串类型）。`options` 是可选的（字符串数组）。

对于 `options`，我们需要一个数组类型的 JSON schema——`param()` 无法表达这一点，因为它只处理标量类型（scalar type）。所以首先，给 `ToolDefinition` 添加 `param_raw()`：

```rust
/// 使用原始 JSON schema 值添加一个参数。
///
/// 用于 `param()` 无法表达的复杂类型（数组、嵌套对象）。
pub fn param_raw(mut self, name: &str, schema: Value, required: bool) -> Self {
    self.parameters["properties"][name] = schema;
    if required {
        self.parameters["required"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::Value::String(name.to_string()));
    }
    self
}
```

现在工具定义同时使用 `param()` 和 `param_raw()`：

```rust
impl AskTool {
    pub fn new(handler: Arc<dyn InputHandler>) -> Self {
        Self {
            definition: ToolDefinition::new(
                "ask_user",
                "Ask the user a clarifying question...",
            )
            .param("question", "string", "The question to ask the user", true)
            .param_raw(
                "options",
                json!({
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional list of choices to present to the user"
                }),
                false,
            ),
            handler,
        }
    }
}
```

### Tool::call

`call` 的实现提取 `question`，通过辅助函数解析 options，然后委托给 handler：

```rust
#[async_trait::async_trait]
impl Tool for AskTool {
    fn definition(&self) -> &ToolDefinition {
        &self.definition
    }

    async fn call(&self, args: Value) -> anyhow::Result<String> {
        let question = args
            .get("question")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing required parameter: question"))?;

        let options = parse_options(&args);

        self.handler.ask(question, &options).await
    }
}

/// 从工具参数中提取可选的 `options` 数组。
fn parse_options(args: &Value) -> Vec<String> {
    args.get("options")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}
```

`parse_options` 辅助函数让 `call()` 专注于正常路径（happy path）。如果 `options` 缺失或不是数组，则默认为空 vec——handler 将此视为自由文本输入。

## 三种 handler

### CliInputHandler

最简单的 handler。打印问题，列出编号选项（如果有），从 stdin 读取一行，并解析编号答案：

```rust
pub struct CliInputHandler;

#[async_trait::async_trait]
impl InputHandler for CliInputHandler {
    async fn ask(&self, question: &str, options: &[String]) -> anyhow::Result<String> {
        let question = question.to_string();
        let options = options.to_vec();

        // 使用 spawn_blocking，因为 stdin 是同步的
        tokio::task::spawn_blocking(move || {
            // 显示问题和编号选项（如果有）
            println!("\n  {question}");
            for (i, opt) in options.iter().enumerate() {
                println!("    {}) {opt}", i + 1);
            }

            // 读取答案
            print!("  > ");
            io::stdout().flush()?;
            let mut line = String::new();
            io::stdin().lock().read_line(&mut line)?;
            let answer = line.trim().to_string();

            // 如果用户输入了有效的选项编号，则解析它
            Ok(resolve_option(&answer, &options))
        }).await?
    }
}

/// 如果 `answer` 是一个匹配某个选项的数字，返回该选项。
/// 否则返回原始答案。
fn resolve_option(answer: &str, options: &[String]) -> String {
    if let Ok(n) = answer.parse::<usize>()
        && n >= 1
        && n <= options.len()
    {
        return options[n - 1].clone();
    }
    answer.to_string()
}
```

`resolve_option` 辅助函数让闭包体保持简洁。它使用了 **let-chain 语法**（在 Rust 1.87 / edition 2024 中稳定）：多个条件用 `&&` 连接，包括 `let Ok(n) = ...` 模式绑定。如果用户输入 `"2"` 且有三个选项，则解析为 `options[1]`。否则返回原始文本。

注意 `for` 循环在切片为空时什么都不做——不需要特殊的 `if` 分支。

在简单的 CLI 应用中使用它，例如 `examples/chat.rs`：

```rust
let agent = SimpleAgent::new(provider)
    .tool(BashTool::new())
    .tool(ReadTool::new())
    .tool(WriteTool::new())
    .tool(EditTool::new())
    .tool(AskTool::new(Arc::new(CliInputHandler)));
```

### ChannelInputHandler

对于 TUI 应用，输入收集发生在事件循环中，而非工具内部。`ChannelInputHandler` 通过 channel 桥接这一差距：

```rust
pub struct UserInputRequest {
    pub question: String,
    pub options: Vec<String>,
    pub response_tx: oneshot::Sender<String>,
}

pub struct ChannelInputHandler {
    tx: mpsc::UnboundedSender<UserInputRequest>,
}
```

当 `ask()` 被调用时，它通过 channel 发送一个 `UserInputRequest` 并等待 oneshot 响应：

```rust
#[async_trait::async_trait]
impl InputHandler for ChannelInputHandler {
    async fn ask(&self, question: &str, options: &[String]) -> anyhow::Result<String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.tx.send(UserInputRequest {
            question: question.to_string(),
            options: options.to_vec(),
            response_tx,
        })?;
        Ok(response_rx.await?)
    }
}
```

TUI 事件循环接收请求并按自己的方式渲染——可以是简单的文本提示，也可以是使用 `crossterm` 在 raw 终端模式下实现的方向键导航选择列表。

### MockInputHandler

用于测试，在队列中预先配置答案：

```rust
pub struct MockInputHandler {
    answers: Mutex<VecDeque<String>>,
}

#[async_trait::async_trait]
impl InputHandler for MockInputHandler {
    async fn ask(&self, _question: &str, _options: &[String]) -> anyhow::Result<String> {
        self.answers.lock().await.pop_front()
            .ok_or_else(|| anyhow::anyhow!("MockInputHandler: no more answers"))
    }
}
```

这遵循与 `MockProvider` 相同的模式——从前端弹出，空时报错。注意这里使用的是 `tokio::sync::Mutex`（配合 `.lock().await`），而非 `std::sync::Mutex`。原因是：`ask()` 是一个 `async fn`，锁守卫（lock guard）必须跨越 `.await` 边界持有。`std::sync::Mutex` 的守卫是 `!Send` 的，因此跨 `.await` 持有它无法编译。`tokio::sync::Mutex` 产生一个 `Send` 安全的守卫，可以在异步上下文中使用。与第一章中的 `MockProvider` 对比，后者使用 `std::sync::Mutex`，因为其 `chat()` 方法不会跨 `.await` 持有守卫。

## 工具摘要

更新 `agent.rs` 中的 `tool_summary()`，以便在终端输出中为 `ask_user` 调用显示 `"question"`：

```rust
let detail = call.arguments
    .get("command")
    .or_else(|| call.arguments.get("path"))
    .or_else(|| call.arguments.get("question"))  // <-- 新增
    .and_then(|v| v.as_str());
```

## Plan mode 集成

`ask_user` 是只读的——它收集信息而不修改任何内容。将其添加到 `PlanAgent` 的默认 `read_only` 集合中（参见[第十二章](./ch12-plan-mode.md)），这样 LLM 在规划阶段也能提问：

```rust
read_only: HashSet::from(["bash", "read", "ask_user"]),
```

## 接入整合

将模块添加到 `mini-claw-code/src/tools/mod.rs`：

```rust
mod ask;
pub use ask::*;
```

并从 `lib.rs` 重新导出：

```rust
pub use tools::{
    AskTool, BashTool, ChannelInputHandler, CliInputHandler,
    EditTool, InputHandler, MockInputHandler, ReadTool,
    UserInputRequest, WriteTool,
};
```

## 运行测试

```bash
cargo test -p mini-claw-code ch11
```

测试验证了：

- **工具定义**：schema 包含 `question`（必需）和 `options`（可选数组）。
- **仅问题**：`MockInputHandler` 为仅包含问题的调用返回答案。
- **带选项**：工具正确地将 options 传递给 handler。
- **缺少问题**：缺少 `question` 参数返回错误。
- **handler 耗尽**：空的 `MockInputHandler` 返回错误。
- **Agent 循环**：LLM 调用 `ask_user`，获取答案，然后返回最终文本。
- **先询问再调用工具**：`ask_user` 之后跟着另一个工具调用（例如 `read`）。
- **多次询问**：两次连续的 `ask_user` 调用，使用不同的答案。
- **Channel 往返**：`ChannelInputHandler` 通过 oneshot channel 发送请求并接收响应。
- **param_raw**：`param_raw()` 正确地将数组参数添加到 `ToolDefinition`。

## 回顾

- **`InputHandler` trait** 抽象了 CLI、TUI 和测试中的输入收集方式。
- **`AskTool`** 让 LLM 暂停执行并向用户提问。
- **`param_raw()`** 扩展了 `ToolDefinition`，支持数组等复杂 JSON schema 类型。
- **三种 handler**：`CliInputHandler` 用于简单应用，`ChannelInputHandler` 用于 TUI 应用，`MockInputHandler` 用于测试。
- **Plan mode**：`ask_user` 默认是只读的，因此在规划阶段也能使用。
- **纯增量变更**：无需修改 `SimpleAgent`、`StreamingAgent` 或任何现有工具。

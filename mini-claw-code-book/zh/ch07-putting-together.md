# 第七章：一个简单的 CLI

你已经构建了所有组件：用于测试的模拟提供者（mock provider）、四个工具、
智能体循环（agent loop）以及 HTTP 提供者。现在是时候把它们全部组装成一个
可以工作的 CLI 了。

## 目标

为 `SimpleAgent` 添加一个 `chat()` 方法，并编写 `examples/chat.rs`，使得：

1. 智能体能够记住对话内容——每个提示都建立在之前的对话基础上。
2. 它打印 `> `，读取一行输入，运行智能体，然后打印结果。
3. 在智能体工作时显示 `thinking...` 指示器。
4. 持续运行，直到用户按下 Ctrl+D（EOF）。

## `chat()` 方法

打开 `mini-claw-code-starter/src/agent.rs`。在 `run()` 下方你会看到 `chat()`
方法的签名。

### 为什么需要一个新方法？

`run()` 每次调用时都会创建一个新的 `Vec<Message>`。这意味着 LLM 没有之前
对话的记忆。一个真正的 CLI 应该向前传递上下文，这样 LLM 才能说"我已经读过
那个文件了"或"正如我之前提到的"。

`chat()` 通过接受调用者传入的消息历史来解决这个问题：

```rust
pub async fn chat(&self, messages: &mut Vec<Message>) -> anyhow::Result<String>
```

调用者在调用前推入 `Message::User(...)`，而 `chat()` 负责追加助手的回合。
当它返回时，`messages` 包含了完整的对话历史，可以直接用于下一轮。

### 实现

循环体与 `run()` 完全相同。唯一的区别是：

1. 使用传入的 `messages` 而不是创建新的 vec。
2. 在 `StopReason::Stop` 时，在推入 `Message::Assistant(turn)` *之前*克隆文本
   ——因为推入操作会移动 `turn`，所以你需要先提取文本。
3. 推入 `Message::Assistant(turn)`，使历史记录包含最终响应。
4. 返回克隆的文本。

```rust
pub async fn chat(&self, messages: &mut Vec<Message>) -> anyhow::Result<String> {
    let defs = self.tools.definitions();

    loop {
        let turn = self.provider.chat(messages, &defs).await?;

        match turn.stop_reason {
            StopReason::Stop => {
                let text = turn.text.clone().unwrap_or_default();
                messages.push(Message::Assistant(turn));
                return Ok(text);
            }
            StopReason::ToolUse => {
                // 与 run() 相同的工具执行逻辑 ...
            }
        }
    }
}
```

`ToolUse` 分支与 `run()` 中完全一样：执行每个工具，收集结果，推入助手回合，
推入工具结果。

### 所有权细节

在 `run()` 中你可以直接 `return Ok(turn.text.unwrap_or_default())`，
因为函数不再需要 `turn` 了。在 `chat()` 中你还需要将
`Message::Assistant(turn)` 推入历史记录。由于推入操作会移动 `turn`，
你必须先提取文本：

```rust
let text = turn.text.clone().unwrap_or_default();
messages.push(Message::Assistant(turn));  // 移动 turn
return Ok(text);                          // 返回克隆的副本
```

相比 `run()` 这只是一行的改动，但它很重要。

## CLI

打开 `mini-claw-code-starter/examples/chat.rs`。你会看到一个包含
`unimplemented!()` 的骨架。把它替换成完整的程序。

### 第 1 步：导入

```rust
use mini_claw_code_starter::{
    BashTool, EditTool, Message, OpenRouterProvider, ReadTool, SimpleAgent, WriteTool,
};
use std::io::{self, BufRead, Write};
```

注意 `Message` 的导入——你需要它来构建历史向量。

### 第 2 步：创建提供者和智能体

```rust
let provider = OpenRouterProvider::from_env()?;
let agent = SimpleAgent::new(provider)
    .tool(BashTool::new())
    .tool(ReadTool::new())
    .tool(WriteTool::new())
    .tool(EditTool::new());
```

和之前一样——这里没有新内容。（在[第十一章](./ch11-user-input.md)中你会在这里
添加 `AskTool`，这样智能体就可以向你提出澄清性问题。）

### 第 3 步：系统提示词和历史向量

```rust
let cwd = std::env::current_dir()?.display().to_string();
let mut history: Vec<Message> = vec![Message::System(format!(
    "You are a coding agent. Help the user with software engineering tasks \
     using all available tools. Be concise and precise.\n\n\
     Working directory: {cwd}"
))];
```

系统提示词（system prompt）是历史记录中的第一条消息。它告诉 LLM 应该扮演
什么角色。有两点需要注意：

1. **提示词中不包含工具名称。** 工具定义是通过 API 单独发送的。系统提示词
   专注于*行为*——做一个编码智能体，使用任何可用的工具，简洁精确。

2. **包含了工作目录。** LLM 需要知道自己在哪里，这样 `read` 和 `bash` 等
   工具调用才能使用正确的路径。这正是真正的编码智能体所做的——Claude Code、
   OpenCode 和 Kimi CLI 都会在系统提示词中注入当前目录（有时还包括平台、
   日期等信息）。

历史向量存在于循环之外，在整个会话过程中积累每一个用户提示、助手响应和工具
结果。系统提示词保持在最前面，在每一轮中为 LLM 提供一致的指令。

### 第 4 步：REPL 循环

```rust
let stdin = io::stdin();

loop {
    print!("> ");
    io::stdout().flush()?;

    let mut line = String::new();
    if stdin.lock().read_line(&mut line)? == 0 {
        println!();
        break;
    }

    let prompt = line.trim();
    if prompt.is_empty() {
        continue;
    }

    history.push(Message::User(prompt.to_string()));
    print!("    thinking...");
    io::stdout().flush()?;
    match agent.chat(&mut history).await {
        Ok(text) => {
            print!("\x1b[2K\r");
            println!("{}\n", text.trim());
        }
        Err(e) => {
            print!("\x1b[2K\r");
            println!("error: {e}\n");
        }
    }
}
```

几点需要注意：

- **`history.push(Message::User(...))`** 在调用智能体之前添加用户提示。
  `chat()` 会追加剩余的内容。
- **`print!("    thinking...")`** 在智能体工作时显示状态。需要 `flush()` 是
  因为 `print!`（没有换行符）不会自动刷新缓冲区。
- **`\x1b[2K\r`** 是一个 ANSI 转义序列："清除整行，将光标移到第 1 列。"
  这会在打印响应之前清除 `thinking...` 文本。当智能体打印工具摘要时也会
  自动清除（因为 `tool_summary()` 使用了相同的转义序列）。
- **`stdout.flush()?`** 在 `print!` 之后确保提示符和思考指示器立即显示。
- `read_line` 在 EOF（Ctrl+D）时返回 `0`，从而跳出循环。
- 智能体的错误会被打印出来而不是导致崩溃——这使得即使某个请求失败，
  循环也能继续运行。

### main 函数

用异步 main 包裹所有内容：

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 第 1-4 步放在这里
    Ok(())
}
```

### 完整程序

把所有内容放在一起，整个程序大约 45 行。这就是你构建的框架的优美之处——
最终的组装非常简单直接，因为每个组件都有清晰的接口。

## 运行完整测试套件

运行完整的测试套件：

```bash
cargo test -p mini-claw-code-starter
```

这会运行第 1 章到第 7 章的所有测试。如果全部通过，恭喜——你的智能体框架
已经完成且经过了全面测试。

### 测试验证了什么

第 7 章的测试是集成测试，它们组合了所有组件：

- **写入后读取流程**：写入文件，读回内容，验证内容正确。
- **编辑流程**：写入文件，编辑文件，读回结果。
- **多工具流水线**：在多个回合中使用 bash、write、edit 和 read。
- **长对话**：五步工具调用序列。

大约有 10 个集成测试，覆盖了完整的智能体流水线。

## 运行聊天示例

要使用真实的 LLM 进行尝试，你需要一个 API 密钥。在工作区根目录创建一个
`.env` 文件：

```
OPENROUTER_API_KEY=sk-or-v1-your-key-here
```

然后运行：

```bash
cargo run -p mini-claw-code-starter --example chat
```

你会看到一个交互式提示符。尝试一个多轮对话：

```text
> List the files in the current directory
    thinking...
    [bash: ls]
Cargo.toml  src/  examples/  ...

> What is in Cargo.toml?
    thinking...
    [read: Cargo.toml]
The Cargo.toml contains the package definition for mini-claw-code-starter...

> Add a new dependency for serde
    thinking...
    [read: Cargo.toml]
    [edit: Cargo.toml]
Done! I added serde to the dependencies.

>
```

注意第二个提示（"What is in Cargo.toml?"）无需重复上下文就能正常工作——
LLM 已经从第一次交互中知道了目录列表。这就是对话历史的作用。

按 Ctrl+D（或 Ctrl+C）退出。

## 你已经构建了什么

让我们退后一步，看看完整的全貌：

```text
examples/chat.rs
    |
    | creates
    v
SimpleAgent<OpenRouterProvider>
    |
    | holds
    +---> OpenRouterProvider (HTTP to LLM API)
    +---> ToolSet (HashMap<String, Box<dyn Tool>>)
              |
              +---> BashTool
              +---> ReadTool
              +---> WriteTool
              +---> EditTool
```

`chat()` 方法驱动整个交互过程：

```text
User prompt
    |
    v
history: [User, Assistant, ToolResult, ..., User]
    |
    v
Provider.chat() ---HTTP---> LLM API
    |
    | AssistantTurn
    v
Tool calls? ----yes---> Execute tools ---> append to history ---> loop
    |
    no
    |
    v
Append final Assistant to history, return text
```

在所有文件中大约 300 行 Rust 代码，你已经拥有了：

- 一个基于 trait 的工具系统，带有 JSON schema 定义。
- 一个通用的智能体循环，可以与任何提供者配合使用。
- 一个用于确定性测试的模拟提供者。
- 一个用于真实 LLM API 的 HTTP 提供者。
- 一个带有对话记忆的 CLI，将所有这些串联在一起。

## 接下来的方向

这个框架是有意做得精简的。以下是一些扩展思路：

**流式响应（Streaming responses）** ——不再等待完整响应，而是在 token 到达时
逐步输出。这意味着需要将 `chat()` 改为返回 `Stream` 而不是单个
`AssistantTurn`。

**Token 限制** ——跟踪 token 使用量，当上下文窗口满时截断旧消息。

**更多工具** ——添加网络搜索工具、数据库查询工具，或者任何你能想到的工具。
`Tool` trait 使得添加新功能变得很容易。

**更丰富的 UI** ——添加旋转动画、Markdown 渲染或折叠式工具调用显示。
参见 `mini-claw-code/examples/tui.rs`，其中使用 `termimad` 实现了这三个功能。

你构建的基础是扎实的。每一个扩展都只是在现有模式上添加内容，而不是重写。
`Provider` trait、`Tool` trait 和智能体循环是你接下来想要构建的一切的基石。

## 下一步

前往[第八章：奇点](./ch08-singularity.md)——你的智能体现在可以修改它自己的
源代码了，我们将讨论这意味着什么，以及接下来该何去何从。

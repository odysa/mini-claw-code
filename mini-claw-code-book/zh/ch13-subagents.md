# 第十三章：subagent

复杂任务很难处理。即使是最优秀的大语言模型（LLM），当一个提示（prompt）要求它
研究代码库、设计方案、编写代码并验证结果——同时还要保持连贯的对话时，也会力不从心。
上下文窗口（context window）被填满，模型失去焦点，质量开始下降。

**subagent** 通过分解来解决这个问题：父 agent 为每个子任务生成一个
subagent。subagent 拥有自己的消息历史和工具集，运行至完成后返回一个摘要。父 agent
只看到最终答案——一个干净、聚焦的结果，不包含 subagent 内部推理的噪声。

这正是 Claude Code 的 **Task 工具** 的工作方式。当 Claude Code 需要探索大型
代码库或处理独立的子任务时，它会生成一个 subagent 来完成工作并汇报结果。OpenCode
和 Anthropic Agent SDK 也使用了相同的模式。

在本章中，你将构建 `SubagentTool`——一个能够生成临时 subagent 的 `Tool` 实现。

你将完成以下内容：

1. 为 `Arc<P>` 添加一个 blanket `impl Provider`，使父子 agent 可以共享同一个
   Provider。
2. 构建 `SubagentTool<P: Provider>`，使用基于闭包的工具工厂（tool factory）和
   构建器方法（builder methods）。
3. 实现 `Tool` trait，包含内联的 agent 循环和轮次限制。
4. 将其作为模块接入并重新导出。

## 为什么需要 subagent？

考虑以下场景：

```text
User: "Add error handling to all API endpoints"

Agent (no subagents):
  → reads 15 files, context window fills up
  → forgets what it learned from file 3
  → produces inconsistent changes

Agent (with subagents):
  → spawns child: "Add error handling to /api/users.rs"
  → child reads 1 file, writes changes, returns "Done: added Result types"
  → spawns child: "Add error handling to /api/posts.rs"
  → child does the same
  → parent sees clean summaries, coordinates the overall task
```

关键洞察：**subagent 就是一个 Tool**。它接收任务描述作为输入，在内部完成工作，
然后返回一个字符串结果。父 agent 的循环不需要任何特殊处理——它调用 subagent 工具
的方式与调用 `read` 或 `bash` 完全相同。

## 通过 `Arc<P>` 共享 Provider

父 agent 和 subagent 需要使用同一个 LLM Provider。在生产环境中，这意味着共享
HTTP 客户端、API 密钥和配置。克隆 Provider 会导致连接重复。我们希望以低成本
的方式共享它。

答案是 `Arc<P>`。但有一个问题：我们的 `Provider` trait 使用了 RPITIT
（return-position `impl Trait` in trait），这意味着它不是对象安全的
（object-safe）。我们不能使用 `dyn Provider`。我们*可以*使用 `Arc<P>`（其中
`P: Provider`）——但前提是 `Arc<P>` 本身也实现了 `Provider`。

一个 blanket impl 可以解决这个问题。在 `types.rs` 中：

```rust
impl<P: Provider> Provider for Arc<P> {
    fn chat<'a>(
        &'a self,
        messages: &'a [Message],
        tools: &'a [&'a ToolDefinition],
    ) -> impl Future<Output = anyhow::Result<AssistantTurn>> + Send + 'a {
        (**self).chat(messages, tools)
    }
}
```

这通过解引用（deref）委托给内部的 `P`。现在 `Arc<MockProvider>` 和
`Arc<OpenRouterProvider>` 都是合法的 Provider。现有代码完全不受影响——如果
你之前传递的是 `MockProvider`，它仍然可以正常工作。`Arc` 包装是可选的。

## `SubagentTool` 结构体

```rust
pub struct SubagentTool<P: Provider> {
    provider: Arc<P>,
    tools_factory: Box<dyn Fn() -> ToolSet + Send + Sync>,
    system_prompt: Option<String>,
    max_turns: usize,
    definition: ToolDefinition,
}
```

这里有三个设计决策：

**使用 `Arc<P>` 作为 Provider。** 父 agent 创建 `Arc::new(provider)`，保留一个
克隆给自己，并传递一个克隆给 `SubagentTool`。两者共享同一个底层 Provider。
成本低、安全，无需克隆 HTTP 客户端。

**使用闭包工厂生产工具。** 工具是 `Box<dyn Tool>`——它们不可克隆（Clone）。
每次 subagent 生成都需要一个全新的 `ToolSet`。`Fn() -> ToolSet` 闭包可以按需
生产。这天然可以捕获 `Arc` 来共享状态：

```rust
let provider = Arc::new(OpenRouterProvider::from_env()?);

SubagentTool::new(provider, || {
    ToolSet::new()
        .with(ReadTool::new())
        .with(WriteTool::new())
        .with(BashTool::new())
})
```

**`max_turns` 安全限制。** 没有这个限制，一个困惑的 subagent 可能会无限循环。
默认值为 10——对实际任务来说足够宽裕，对防止失控循环来说足够严格。

## 构建器（Builder）

构造过程使用与代码库其他部分相同的流式构建器风格（fluent builder pattern）：

```rust
impl<P: Provider> SubagentTool<P> {
    pub fn new(
        provider: Arc<P>,
        tools_factory: impl Fn() -> ToolSet + Send + Sync + 'static,
    ) -> Self {
        Self {
            provider,
            tools_factory: Box::new(tools_factory),
            system_prompt: None,
            max_turns: 10,
            definition: ToolDefinition::new(
                "subagent",
                "Spawn a child agent to handle a subtask independently. \
                 The child has its own message history and tools.",
            )
            .param(
                "task",
                "string",
                "A clear description of the subtask for the child agent to complete.",
                true,
            ),
        }
    }

    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    pub fn max_turns(mut self, max: usize) -> Self {
        self.max_turns = max;
        self
    }
}
```

工具定义暴露了一个 `task` 参数——LLM 写一个清晰的描述来说明 subagent 应该做什么。
简洁而有效。

## `Tool` trait 实现

`SubagentTool` 的核心是它的 `Tool::call()` 方法。它内联了一个最小化的 agent
循环——与 `SimpleAgent::chat()` 相同的协议（调用 Provider、执行工具、循环），
但增加了轮次限制、不输出到终端，并使用局部拥有的消息向量（message vec）：

```rust
#[async_trait::async_trait]
impl<P: Provider + 'static> Tool for SubagentTool<P> {
    fn definition(&self) -> &ToolDefinition {
        &self.definition
    }

    async fn call(&self, args: Value) -> anyhow::Result<String> {
        let task = args
            .get("task")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing required parameter: task"))?;

        let tools = (self.tools_factory)();
        let defs = tools.definitions();

        let mut messages = Vec::new();
        if let Some(ref prompt) = self.system_prompt {
            messages.push(Message::System(prompt.clone()));
        }
        messages.push(Message::User(task.to_string()));

        for _ in 0..self.max_turns {
            let turn = self.provider.chat(&messages, &defs).await?;

            match turn.stop_reason {
                StopReason::Stop => {
                    return Ok(turn.text.unwrap_or_default());
                }
                StopReason::ToolUse => {
                    let mut results = Vec::with_capacity(turn.tool_calls.len());
                    for call in &turn.tool_calls {
                        let content = match tools.get(&call.name) {
                            Some(t) => t
                                .call(call.arguments.clone())
                                .await
                                .unwrap_or_else(|e| format!("error: {e}")),
                            None => format!("error: unknown tool `{}`", call.name),
                        };
                        results.push((call.id.clone(), content));
                    }
                    messages.push(Message::Assistant(turn));
                    for (id, content) in results {
                        messages.push(Message::ToolResult { id, content });
                    }
                }
            }
        }

        Ok("error: max turns exceeded".to_string())
    }
}
```

有几点值得注意：

**没有使用 `tokio::spawn`。** subagent 在父 agent 的 `Tool::call()` future 内
运行。这是有意为之的——生成一个后台任务会增加协调复杂性（通道、join handle、
取消机制）。内联运行保持了简单性和确定性。

**全新的消息历史。** subagent 仅以系统提示（可选）和作为 `User` 消息的任务描述
开始。它永远看不到父 agent 的对话。当 subagent 完成时，只有其最终文本作为工具结果
返回给父 agent。subagent 的内部消息会被丢弃。

**轮次限制是软错误。** 当超过 `max_turns` 时，工具返回一个错误字符串而不是
`Err(...)`。这让父 LLM 看到失败并决定如何处理（用更简单的任务重试、尝试不同
的方法等），而不是让整个 agent 循环崩溃。

**Provider 错误会向上传播。** 如果 LLM API 在 subagent 运行期间失败，错误通过
`?` 冒泡到父 agent。这是有意的——API 错误是基础设施故障，而非任务失败。

## 接入模块

在 `mini-claw-code/src/lib.rs` 中添加模块并重新导出：

```rust
pub mod subagent;
// ...
pub use subagent::SubagentTool;
```

## 使用示例

以下是如何为父 agent 接入 subagent 工具：

```rust
use std::sync::Arc;
use mini_claw_code::*;

let provider = Arc::new(OpenRouterProvider::from_env()?);
let p = provider.clone();

let agent = SimpleAgent::new(provider)
    .tool(ReadTool::new())
    .tool(WriteTool::new())
    .tool(BashTool::new())
    .tool(SubagentTool::new(p, || {
        ToolSet::new()
            .with(ReadTool::new())
            .with(WriteTool::new())
            .with(BashTool::new())
    }));

let result = agent.run("Refactor the auth module").await?;
```

父 LLM 在其工具列表中看到 `subagent`，与 `read`、`write` 和 `bash` 并列。
当任务足够复杂时，LLM 可以选择通过 `subagent` 委派——或者直接使用其他工具处理。
由 LLM 自行决定。

你也可以给 subagent 设置专门的系统提示：

```rust
SubagentTool::new(provider, || {
    ToolSet::new()
        .with(ReadTool::new())
        .with(BashTool::new())
})
.system_prompt("You are a security auditor. Review code for vulnerabilities.")
.max_turns(15)
```

## 运行测试

```bash
cargo test -p mini-claw-code ch13
```

测试验证了以下场景：

- **文本响应**：subagent 立即返回文本（没有工具调用）。
- **使用工具**：subagent 在回答前使用 `ReadTool`。
- **多步骤**：subagent 跨多个轮次进行多次工具调用。
- **超过最大轮次**：轮次限制被强制执行，返回错误字符串。
- **缺少任务参数**：缺少 `task` 参数时报错。
- **Provider 错误**：subagent 的 Provider 错误传播到父 agent。
- **未知工具**：subagent 优雅地处理未知工具。
- **构建器模式**：链式调用 `.system_prompt().max_turns()` 能够编译通过。
- **系统提示**：配置系统提示后 subagent 正确运行。
- **写入工具**：subagent 写入文件，父 agent 之后继续工作。
- **父 agent 继续**：subagent 完成后父 agent 恢复自己的工作。
- **历史隔离**：subagent 的消息不会泄露到父 agent 的消息向量中。

## 总结

- **`SubagentTool`** 是一个生成临时 subagent 的 `Tool`。父 agent 只看到最终答案。
- **`Arc<P>`** blanket impl 让父子 agent 共享 Provider 而无需克隆。完全向后兼容。
- **闭包工厂** 为每次 subagent 生成产生一个全新的 `ToolSet`，因为 `Box<dyn Tool>`
  不可克隆。
- **内联 agent 循环** 配合 `max_turns` 守卫，使 `SimpleAgent` 保持不变。不需要
  `tokio::spawn`——subagent 在 `Tool::call()` 内运行。
- **消息隔离**：subagent 的内部消息局限于 `call()` future 中。只有最终文本传回
  父 agent。
- **单一 `task` 参数**：LLM 写一个清晰的任务描述；subagent 处理其余部分。
- **纯增量修改**：唯一对现有代码的改动是 `types.rs` 中的 blanket impl。其他
  都是新代码。

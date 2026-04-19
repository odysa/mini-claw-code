# 第 5b 章：OpenRouter 与 StreamingAgent

> **需要编辑的文件：** `src/providers/openrouter.rs`、`src/streaming.rs`（底部的 `StreamingAgent` 块）
> **需要运行的测试：** `cargo test -p mini-claw-code-starter test_openrouter_`、`cargo test -p mini-claw-code-starter test_streaming_streaming_agent_`、`cargo test -p mini-claw-code-starter test_streaming_stream_chat_`
> **预计用时：** 35 分钟

## 目标

- 实现 `OpenRouterProvider`，让 agent 能与真实的 OpenAI 兼容 API 通信——非流式和流式两种方式都要。
- 实现 `StreamingAgent::chat`——在 LLM 仍在生成内容时把流式文本增量转发到 UI channel 的 agent 循环。

[第 5a 章](./ch05a-provider-foundations.md)构建了抽象（`Provider`、`StreamProvider`、`StreamEvent`）、mock（`MockProvider`、`MockStreamProvider`）以及解析/积累机制（`parse_sse_line`、`StreamAccumulator`）。这一章把这些部件接入真实的 HTTP provider，并将流式 channel 接通 agent 循环。

下面的内容若假设 `parse_sse_line` 或 `StreamAccumulator` 已经存在——它们确实存在，因为你在第 5a 章已经实现了。

### 侧边栏：面向 Go 开发者的 tokio 并发

如果 Go 是你的原生异步语言，下面这张翻译对照表是阅读流式代码前的必备知识。本章的一切都建立在这五个原语之上；已经习惯用 `tokio` 思考的可以跳过。

| Go                                      | Tokio                                        | 说明                                                                                                                     |
|-----------------------------------------|-----------------------------------------------|---------------------------------------------------------------------------------------------------------------------------|
| `go func() { ... }()`                   | `tokio::spawn(async { ... })`                 | 两者都是"触发后不管"。`tokio::spawn` 返回 `JoinHandle`，如果你关心结果，可以稍后 `await` 它。           |
| `ch := make(chan T, n)`                 | `let (tx, rx) = tokio::sync::mpsc::channel::<T>(n)` | 有界 channel。无界版本用 `mpsc::unbounded_channel()`——类似于缓冲区无限大的 channel。 |
| `ch <- v`                               | `tx.send(v).await`                            | Tokio 中的异步发送（缓冲区满时等待）。无界版本用 `tx.send(v)`，无需 `.await`。                  |
| `v, ok := <-ch`                         | `let Some(v) = rx.recv().await { ... }`       | 当*所有*发送方都被 drop 时，`recv` 返回 `None`（等价于 `close(ch)` 加排空）。                                 |
| `close(ch)`                             | drop 掉每一个 `tx` 克隆                         | Tokio 没有显式的 close。最后一个发送方被 drop 时，接收方收到 `None`，循环退出。                      |
| `wg.Add(1); wg.Wait()`                  | `handle.await`（或 `tokio::join!`、`try_join!`） | `JoinHandle` 相当于单个 goroutine 的 WaitGroup。多个 handle：`tokio::join!(h1, h2)` 并发运行它们。    |
| `select { case <-a: case <-b: }`        | `tokio::select! { _ = a => ..., _ = b => ... }` | 直接对应。若分支不互斥，需使用 `biased;`。                                                                 |

本章有一个值得单独说明的细节：我们通过*丢弃发送方*来表示"流结束"，没有显式的 close 调用。接收方任务观察到 `rx.recv().await == None` 后退出循环。如果你忘记 drop 发送方（比如把它放在一个比生产者活得更长的 `Arc` 里），接收方会永远挂起——这正是 [§"为什么不在主循环中直接 `rx.recv()`？"](#为什么不在主循环中直接-rxrecv) 中分析的死锁模式之一。

---

## OpenRouterProvider

有了解析基础设施，可以构建真实的 provider 了。目标是 [OpenRouter](https://openrouter.ai/) API，与 OpenAI 兼容——同样的请求/响应格式适用于 OpenAI、Together、Groq 等。

### API 类型

provider 需要 serde 类型处理请求和响应载荷。请求侧：

```rust
#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ApiMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ApiTool>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
}
```

`skip_serializing_if` 注解保持 JSON 整洁——`tools` 为空时省略（某些模型对空数组会报错），`stream` 为 `false` 时省略（这是 API 的默认值）。

`ApiMessage`、`ApiToolCall`、`ApiFunction`、`ApiTool` 和 `ApiToolDef` 镜像 OpenAI 消息格式。响应类型（`ChatResponse`、`Choice`、`ResponseMessage`）反序列化非流式响应。块类型（`ChunkResponse`、`ChunkChoice`、`Delta`、`DeltaToolCall`、`DeltaFunction`）反序列化流式响应——你在第 5a 章已为 `parse_sse_line` 实现了它们。

### 转换辅助函数

`OpenRouterProvider` 上两个 `impl` 方法负责内部类型和 API 格式之间的转换。`convert_messages` 处理四个 `Message` 变体：

```rust
pub(crate) fn convert_messages(messages: &[Message]) -> Vec<ApiMessage> {
    let mut out = Vec::new();
    for msg in messages {
        match msg {
            Message::System(text) => out.push(ApiMessage {
                role: "system".into(),
                content: Some(text.clone()),
                tool_calls: None,
                tool_call_id: None,
            }),
            Message::User(text) => out.push(ApiMessage {
                role: "user".into(),
                content: Some(text.clone()),
                tool_calls: None,
                tool_call_id: None,
            }),
            Message::Assistant(turn) => out.push(ApiMessage {
                role: "assistant".into(),
                content: turn.text.clone(),
                tool_calls: if turn.tool_calls.is_empty() {
                    None
                } else {
                    Some(
                        turn.tool_calls
                            .iter()
                            .map(|c| ApiToolCall {
                                id: c.id.clone(),
                                type_: "function".into(),
                                function: ApiFunction {
                                    name: c.name.clone(),
                                    arguments: c.arguments.to_string(),
                                },
                            })
                            .collect(),
                    )
                },
                tool_call_id: None,
            }),
            Message::ToolResult { id, content } => out.push(ApiMessage {
                role: "tool".into(),
                content: Some(content.clone()),
                tool_calls: None,
                tool_call_id: Some(id.clone()),
            }),
        }
    }
    out
}
```

四个细节值得停下来看：

- **`System` 和 `User` 是对称的。** 形状相同，只是 role 字符串不同，其他字段（`tool_calls`、`tool_call_id`）均为 `None`。
- **`Assistant` 有细微差别。** `text` 直接映射到 `content`，但工具调用需要重新序列化。`c.arguments` 是 `serde_json::Value`；OpenAI API 期望它是 JSON *字符串*，所以调用 `.to_string()` 把 `Value` 转回文本。发送空的 `tool_calls: []` 数组会让一些 provider 以请求格式错误为由拒绝，因此改用 `None`。
- **`ToolResult` 变为 `role: "tool"`。** 通过 `tool_call_id` 将结果与原始调用关联。没有这个 id，provider 无法对上结果和调用，下一个响应通常是报错。
- **没有 default 分支。** 每个 `Message` 变体都被显式处理。如果第 4 章新增了变体，这里的 match 会拒绝编译，直到你决定如何序列化它——这正是我们想要的行为。

`convert_tools` 更简单：把每个 `ToolDefinition` 包裹进 OpenAI 函数调用信封。

```rust
pub(crate) fn convert_tools(tools: &[&ToolDefinition]) -> Vec<ApiTool> {
    tools
        .iter()
        .map(|t| ApiTool {
            type_: "function",
            function: ApiToolDef {
                name: t.name,
                description: t.description,
                parameters: t.parameters.clone(),
            },
        })
        .collect()
}
```

信封形状固定：`{ "type": "function", "function": { name, description, parameters } }`。每个 OpenAI 兼容 provider 都期望这个格式，`ToolDefinition` 在第 4 章设计时就是为了让这个映射只需一行。

### provider struct

```rust
pub struct OpenRouterProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
    base_url: String,
}
```

持有可复用的 `reqwest::Client`、API 密钥、模型名称和基础 URL。构造函数：`new(api_key, model)` 显式创建，`from_env()` 通过 `dotenvy` 加载 `OPENROUTER_API_KEY`，`base_url(self, url)` 构建器方法覆盖端点（适用于本地测试或替代 provider）。

### 非流式 `Provider` impl

非流式路径更简单：一次 POST，一个 JSON 响应，返回 `AssistantTurn`。完整实现：

```rust
impl Provider for OpenRouterProvider {
    async fn chat(
        &self,
        messages: &[Message],
        tools: &[&ToolDefinition],
    ) -> anyhow::Result<AssistantTurn> {
        let body = ChatRequest {
            model: &self.model,
            messages: Self::convert_messages(messages),
            tools: Self::convert_tools(tools),
            stream: false,
        };

        let resp: ChatResponse = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("request failed")?
            .error_for_status()
            .context("API returned error status")?
            .json()
            .await
            .context("failed to parse response")?;

        let choice = resp.choices.into_iter().next().context("no choices")?;

        let tool_calls = choice
            .message
            .tool_calls
            .unwrap_or_default()
            .into_iter()
            .map(|tc| {
                let arguments =
                    serde_json::from_str(&tc.function.arguments).unwrap_or(Value::Null);
                ToolCall {
                    id: tc.id,
                    name: tc.function.name,
                    arguments,
                }
            })
            .collect();

        let stop_reason = match choice.finish_reason.as_deref() {
            Some("tool_calls") => StopReason::ToolUse,
            _ => StopReason::Stop,
        };

        let usage = resp.usage.map(|u| TokenUsage {
            input_tokens: u.prompt_tokens.unwrap_or(0),
            output_tokens: u.completion_tokens.unwrap_or(0),
        });

        Ok(AssistantTurn {
            text: choice.message.content,
            tool_calls,
            stop_reason,
            usage,
        })
    }
}
```

三个决策值得注意：

- **`error_for_status()` 将 HTTP 4xx/5xx 转为 `Err`。** 否则来自 OpenRouter 的 403 会把响应体当作 `ChatResponse` 反序列化，在更后面以莫名其妙的方式失败。
- **工具调用参数以 JSON *字符串*形式到达，不是 `Value`。** OpenAI 规范在传输格式中用 `"arguments": "{\"path\":\"foo.rs\"}"`。我们自己把它解析回 `Value`；解析失败时回退到 `Value::Null`，格式错误的 `arguments` 字段不会中止整个轮次。
- **`stop_reason` 是对 `finish_reason` 的直接映射。** 只有 `"tool_calls"` 变为 `ToolUse`，其他一切（`"stop"`、`"length"`、null、缺失）变为 `Stop`。与[第 3 章的旁注](./ch03-agentic-loop.md#aside-who-decides-stop-vs-tooluse)中"由模型决定"的说法一致——我们只是在翻译模型自己的停止信号。

### 流式 `StreamProvider` impl

流式路径与非流式请求形状相同，只是 `stream: true`，但读取的是分块 HTTP 响应，解析为 Server-Sent Events。完整实现：

```rust
impl crate::streaming::StreamProvider for OpenRouterProvider {
    async fn stream_chat(
        &self,
        messages: &[Message],
        tools: &[&ToolDefinition],
        tx: tokio::sync::mpsc::UnboundedSender<crate::streaming::StreamEvent>,
    ) -> anyhow::Result<AssistantTurn> {
        use crate::streaming::{StreamAccumulator, parse_sse_line};

        let body = ChatRequest {
            model: &self.model,
            messages: Self::convert_messages(messages),
            tools: Self::convert_tools(tools),
            stream: true,
        };

        let mut resp = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("request failed")?
            .error_for_status()
            .context("API returned error status")?;

        let mut acc = StreamAccumulator::new();
        let mut buffer = String::new();

        while let Some(chunk) = resp.chunk().await.context("failed to read chunk")? {
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].trim_end_matches('\r').to_string();
                buffer = buffer[newline_pos + 1..].to_string();

                if line.is_empty() {
                    continue;
                }

                if let Some(events) = parse_sse_line(&line) {
                    for event in events {
                        acc.feed(&event);
                        let _ = tx.send(event);
                    }
                }
            }
        }

        Ok(acc.finish())
    }
}
```

逐步解析：

1. **同样的请求，`stream: true`。** API 返回分块 HTTP 响应，不是单个 JSON 体。请求构建和鉴权与非流式路径完全相同——这正是抽象的价值所在。
2. **读取原始字节块。** `resp.chunk()` 返回 `Option<Bytes>`——HTTP 体以任意大小的片段到达，与 SSE 事件边界不对齐。一个 chunk 可能是半行、几行，或多个事件挤在一起。
3. **缓冲并按换行符分割。** TCP 块可能在 SSE 行中间截断。`buffer` 积累原始文本，内层 `while` 循环提取完整行。经典的面向行协议解析——积累字节，行可用时消费。内层循环持续到缓冲区没有更多完整行，然后等待下一个块。
4. **解析每行。** `parse_sse_line`（来自第 5a 章）把 `data:` 行转换为 `StreamEvent`。空行（SSE 事件分隔符）和非数据行（注释、keep-alive）返回 `None` 被跳过。
5. **同时喂给 accumulator 和 channel。** 对每个事件，accumulator 更新内部状态（构建最终的 `AssistantTurn`），channel 实时把同一个事件传给 UI。`let _ = tx.send(event)` 有意忽略发送错误：接收方已被 drop 时（如转发任务因主循环取消而退出），仍然要把流消费完，底层 HTTP 连接才能干净释放。
6. **返回组装好的消息。** 流结束（`resp.chunk()` 返回 `None`）后，accumulator 已收集所有内容，`finish()` 产生最终的 `AssistantTurn`。此时 `tx` 被 drop（函数返回），channel 关闭，向转发任务发出退出信号——这正是下面 `StreamingAgent` 所依赖的终止流程。

这种双路设计（accumulator + channel）正是 Claude Code 处理流式传输的方式。UI 在 token 到达时渲染，agent 循环看到的是干净、完整的响应——无需对部分状态做任何特殊处理。

### 你的任务

`OpenRouterProvider` 位于 `src/providers/openrouter.rs`。填写构造函数、转换辅助函数、`Provider` impl 和 `StreamProvider` impl。所需依赖（`reqwest`、`dotenvy`）已在 `Cargo.toml` 中。

---

## StreamingAgent

provider 层有了流式传输之后，需要一个能从中受益的 agent 循环。把 LLM 回复*流入 provider* 只有在文本*到达用户终端*时才有意义。这个接线工作就是 `StreamingAgent`。

`StreamingAgent` 是第 3 章 `SimpleAgent` 的流式版本：

- `SimpleAgent::chat` 调用 `provider.chat()`，返回完整的 `AssistantTurn`。
- `StreamingAgent::chat` 调用 `provider.stream_chat()`，**在 LLM 仍在生成时把文本增量转发到 UI channel**，流结束后返回组装好的响应。

struct 和构建器与 `SimpleAgent` 完全相同：

```rust
pub struct StreamingAgent<P: StreamProvider> {
    provider: P,
    tools: ToolSet,
}

impl<P: StreamProvider> StreamingAgent<P> {
    pub fn new(provider: P) -> Self {
        Self { provider, tools: ToolSet::new() }
    }

    pub fn tool(mut self, t: impl Tool + 'static) -> Self {
        self.tools.push(t);
        self
    }

    pub async fn run(
        &self,
        prompt: &str,
        events: mpsc::UnboundedSender<AgentEvent>,
    ) -> anyhow::Result<String> {
        let mut messages = vec![Message::User(prompt.to_string())];
        self.chat(&mut messages, events).await
    }

    pub async fn chat(
        &self,
        messages: &mut Vec<Message>,
        events: mpsc::UnboundedSender<AgentEvent>,
    ) -> anyhow::Result<String> { /* ... */ }
}
```

`run()` 是 `chat()` 的薄包装。真正的工作在 `chat()` 里，也是本章最微妙的一段代码。

### 两个 channel 及它们解决的问题

`StreamingAgent::chat` 坐落在两个*词汇不同*的 channel 之间：

- **下游（provider → agent）：** provider 用 `StreamEvent`——原始流片段，包括 `TextDelta`、`ToolCallStart`、`ToolCallDelta` 和 `Done`。这是流式 LLM 响应的全部底层语法。
- **上游（agent → UI）：** UI 要的是 `AgentEvent`——agent 级别的通知：`TextDelta` 用于可显示的文本，`ToolCall` 表示工具开始运行，`Done` 表示整个对话结束，`Error` 表示出了问题。

`StreamingAgent::chat` 是翻译器。它需要：

1. 给 provider 一个 `StreamEvent` channel，让 provider 向其发送增量。
2. **并发地**从该 channel 拉取，过滤 `TextDelta`，重新发送为 `AgentEvent::TextDelta` 到 UI channel——这一切都在 provider 仍在生成时进行。
3. 等待 provider 返回组装好的 `AssistantTurn`。
4. 决策：轮次以 `Stop` 结束就发送 `AgentEvent::Done` 并返回；以 `ToolUse` 结束就每次调用发送 `ToolCall` 事件，运行工具，追加结果，循环。

关键词是第 2 步的**并发**。不能在 `stream_chat` 返回后再 `recv()` 事件——那时生成已经结束，UI 一直在等一个冻结的屏幕。需要独立任务在 provider 仍在写入时从流 channel 拉取。

### 转发任务模式

完整的 `chat()` 实现：

```rust
pub async fn chat(
    &self,
    messages: &mut Vec<Message>,
    events: mpsc::UnboundedSender<AgentEvent>,
) -> anyhow::Result<String> {
    let defs = self.tools.definitions();

    loop {
        // 1. Fresh stream channel for this turn.
        let (stream_tx, mut stream_rx) = mpsc::unbounded_channel();

        // 2. Spawn a forwarder task: drain stream_rx, relay TextDeltas to `events`.
        let events_clone = events.clone();
        let forwarder = tokio::spawn(async move {
            while let Some(event) = stream_rx.recv().await {
                if let StreamEvent::TextDelta(text) = event {
                    let _ = events_clone.send(AgentEvent::TextDelta(text));
                }
            }
        });

        // 3. Kick off generation. The provider writes StreamEvents into stream_tx.
        //    Dropping stream_tx here would close the channel early — so we pass it by value.
        let turn = match self.provider.stream_chat(messages, &defs, stream_tx).await {
            Ok(t) => t,
            Err(e) => {
                let _ = events.send(AgentEvent::Error(e.to_string()));
                return Err(e);
            }
        };

        // 4. stream_chat has returned → stream_tx was dropped → forwarder sees
        //    stream_rx closed → forwarder exits. Await it to propagate any panic
        //    and ensure all deltas are flushed before we emit downstream events.
        let _ = forwarder.await;

        // 5. Now handle the assembled turn: stop or another tool round.
        match turn.stop_reason {
            StopReason::Stop => {
                let text = turn.text.clone().unwrap_or_default();
                let _ = events.send(AgentEvent::Done(text.clone()));
                messages.push(Message::Assistant(turn));
                return Ok(text);
            }
            StopReason::ToolUse => {
                let mut results = Vec::with_capacity(turn.tool_calls.len());
                for call in &turn.tool_calls {
                    let _ = events.send(AgentEvent::ToolCall {
                        name: call.name.clone(),
                        summary: tool_summary(call),
                    });
                    let content = match self.tools.get(&call.name) {
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
                // Loop: feed results back to the LLM.
            }
        }
    }
}
```

逐步解析：

1. **每次循环迭代创建新的 channel。** 每个轮次都创建新的 `mpsc::unbounded_channel()`，不能跨工具轮次复用——丢弃 `stream_tx` 是告知转发任务轮次结束的方式（见第 4 步）。保留同一个 channel，转发任务就永远不会退出。

2. **spawn 转发任务。** `tokio::spawn` 并发运行一个任务，在 `stream_rx.recv().await` 上循环，把 `StreamEvent::TextDelta` 过滤为 `AgentEvent::TextDelta`。其他内容被丢弃——`ToolCallStart`/`ToolCallDelta`/`Done` 不会以文本形式出现在 UI 中。把 `events` 发送方移入任务之前先克隆，因为转发任务退出后还需要原始的来发送 `ToolCall`/`Done`/`Error`。

3. **调用 `stream_chat` 并等待。** provider 现在向 `stream_tx` 写入 `StreamEvent`，转发任务在事件到达时拉取并把文本中继到 UI，当前任务阻塞在 `stream_chat` future 上。三个任务同时推进：HTTP 响应读取器、转发任务，以及（通过 channel）UI 渲染器。

4. **等待转发任务。** `stream_chat` 返回时，其持有的 `stream_tx` 被 drop，channel 关闭，`stream_rx.recv()` 返回 `None`，结束转发任务的 `while let` 循环。等待 `JoinHandle` 做了两件事：确保转发任务在我们继续之前把每一个最后的增量刷新到 UI，并暴露转发任务可能遇到的 panic。忘记这个 `await` 是经典的"最后几个 token 丢失"bug。

5. **根据 `stop_reason` 分发。** 此时有了完整的 `AssistantTurn`，UI 也看到了每一个 `TextDelta`。模型完成了（`Stop`）就发送 `AgentEvent::Done` 并返回；需要工具（`ToolUse`）就每次调用发送 `ToolCall` 事件（UI 用这些显示"[bash: ls]"旋转图标），用与 `SimpleAgent` 相同的优雅错误处理运行每个工具，把结果追加到 `messages`，让 `loop` 继续——下一轮会 spawn 新的转发任务并调用 `stream_chat`。

### 为什么不在主循环中直接 `rx.recv()`？

单任务方式——"调用 `stream_chat`，然后排空 `rx`"——会死锁。`stream_chat` 在流被完全消费之前不返回；无界 channel 里充满了事件但没人读，provider 会一直写入（技术上可行，但轮次结束前什么都不渲染）。用*有界* channel 会在 `tx.send().await` 处阻塞 provider，进而阻塞 `stream_chat`，永不返回。无论哪种方式，UI 都要等到轮次结束才能看到 token——流式传输就失去了意义。

转发任务模式把两端解耦：provider 的写入侧和 UI 的读取侧都能独立推进。

### 完整工作模式的端到端视图

下面把死锁修复后的流程完整画出来。四个 Rust 任务，三条关键边：provider 写入 `tx`，转发任务拉取 `rx` 并重新发送到 `events`，主循环等待 `stream_chat` 的返回值做控制流决策。终止完全依赖 drop：`stream_chat` 返回时 drop 掉 `tx`，`rx.recv()` 随后返回 `None`，转发任务循环退出，`handle.await` 解除阻塞。

```mermaid
sequenceDiagram
    participant M as Main loop
    participant F as Forwarder task
    participant P as stream_chat
    participant U as UI (events rx)

    M->>M: let (tx, rx) = mpsc::unbounded_channel::<StreamEvent>()
    M->>F: tokio::spawn(forwarder(rx, events))
    M->>P: provider.stream_chat(messages, tools, tx).await
    Note over P: holds the tx sender;<br/>writes events as they arrive
    P-->>F: tx.send(TextDelta) (many)
    F-->>U: events.send(AgentEvent::TextDelta)
    P-->>F: tx.send(ToolCallStart / Delta / Done)
    F-->>U: events.send(...)
    P-->>M: returns AssistantTurn (drops tx here)
    Note over F: rx.recv() now returns None,<br/>forwarder loop exits naturally
    F-->>M: JoinHandle resolves
    M->>M: match turn.stop_reason { Stop => ..., ToolUse => ... }
```

三个不变式保证这个模式正常运转：

1. **provider 拥有发送方。** 只有 `stream_chat` 持有 `tx`——主循环将其交出后不保留克隆。`stream_chat` 返回时，最后一个 `tx` 被 drop，channel 关闭。
2. **转发任务拥有接收方。** 在独立 spawn 的任务中运行，接收方能在 `stream_chat` 仍在写入时推进，没有其他人调用 `rx.recv()`。
3. **主循环等待两者。** 先等 `stream_chat`，再等转发任务的 `JoinHandle`。等待 handle 是防止主循环把未完成的转发任务泄漏到下一次 agent 循环迭代的关键。

三个不变式中任何一个被打破——主循环持有多余的 `tx` 克隆、转发任务在主任务上内联运行、或主循环跳过 handle 的 await——就会出现上述死锁的某个变体。所以这个模式值得认真学一次，以后每当需要把流式 I/O 桥接到逐步决策循环时，直接拿来用。

### 你的任务

在 `src/streaming.rs` 中填写 `StreamingAgent::chat()` 存根。四步配方：channel、转发任务、等待 `stream_chat`、等待转发任务。然后对 `stop_reason` 的 `match` 与 `SimpleAgent::chat` 的形状相同。

---

## 运行测试

```bash
cargo test -p mini-claw-code-starter test_openrouter_
cargo test -p mini-claw-code-starter test_streaming_streaming_agent_
cargo test -p mini-claw-code-starter test_streaming_stream_chat_
```

### 这些测试验证的内容

**`test_openrouter_`**（OpenRouterProvider）：

- **`test_openrouter_convert_messages`** — 内部 `Message` 变体被转换为正确的 OpenAI API 格式
- **`test_openrouter_convert_tools`** — `ToolDefinition` 值被包裹在 OpenAI 函数调用信封中

**`test_streaming_streaming_agent_`**（StreamingAgent 对 `MockStreamProvider` 的端到端测试）：

- **`test_streaming_streaming_agent_text_response`** — 单轮文本响应；UI channel 至少看到一个 `TextDelta` 和一个 `Done`
- **`test_streaming_streaming_agent_tool_loop`** — agent 运行一轮工具调用并产生最终答案；UI channel 看到 `ToolCall` 事件和 `Done`
- **`test_streaming_streaming_agent_chat_history`** — `chat()` 将最终的 assistant 轮次追加到调用方提供的 `messages` vec 中

**`test_streaming_stream_chat_`**（OpenRouter 流式传输对本地 TCP mock）：

- **`test_streaming_stream_chat_events_order`** — 脚本化的 SSE 体被解析为正确顺序的事件，组装好的 `AssistantTurn` 与预期匹配

---

## 关键要点

`StreamingAgent` 是第 5a 章一切投入的回报。provider 产生 `StreamEvent`，转发任务把它们在到达时翻译为 UI 级别的 `AgentEvent`，主循环等待组装好的 `AssistantTurn` 来决定下一步。token 实时到达终端；agent 循环仍然看到干净、完整的消息——无需对流式和非流式做特殊处理。

"把复杂流分成两个并发侧，用任务桥接"——这个模式正是 Claude Code 在渲染器中使用的。写过一次之后，每当需要把流式 I/O 与逐步决策混合，它就会随处出现。

[第 6 章](./ch06-tool-interface.md)转向工具——agent 与外部世界接口的另一半。

## 自我检测

{{#quiz ../quizzes/ch05b.toml}}

---

[← 第 5a 章：Provider 与流式基础](./ch05a-provider-foundations.md) · [目录](./ch00-overview.md) · [第 6 章：工具接口 →](./ch06-tool-interface.md)

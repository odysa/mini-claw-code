# 第六章：OpenRouter Provider

到目前为止，所有功能都通过 `MockProvider` 在本地运行。在本章中，你将实现 `OpenRouterProvider` —— 一个通过 HTTP 使用 OpenAI 兼容的 chat completions API 与真实 LLM 通信的 provider。

这是让你的 agent 真正运转起来的一章。

## 目标

实现 `OpenRouterProvider`，使其能够：

1. 通过 API 密钥和模型名称创建实例。
2. 将我们内部的 `Message` 和 `ToolDefinition` 类型转换为 API 格式。
3. 向 chat completions 端点发送 HTTP POST 请求。
4. 将响应解析回 `AssistantTurn`。

## 关键 Rust 概念

### Serde 派生宏与属性

`openrouter.rs` 中的 API 类型已经提供好了 —— 你不需要修改它们。但理解它们会有所帮助：

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct ApiToolCall {
    pub(crate) id: String,
    #[serde(rename = "type")]
    pub(crate) type_: String,
    pub(crate) function: ApiFunction,
}
```

使用到的关键 serde 属性：

- **`#[serde(rename = "type")]`** —— JSON 字段名为 `"type"`，但 `type` 是 Rust 的保留关键字。因此结构体字段命名为 `type_`，serde 在序列化/反序列化时自动重命名。

- **`#[serde(skip_serializing_if = "Option::is_none")]`** —— 当值为 `None` 时，在 JSON 中省略该字段。这很重要，因为 API 期望某些未使用的字段不存在（而非为 `null`）。

- **`#[serde(skip_serializing_if = "Vec::is_empty")]`** —— 对空向量同理。如果没有工具，我们完全省略 `tools` 字段。

### `reqwest` HTTP 客户端

`reqwest` 是 Rust 中标准的 HTTP 客户端 crate。使用模式如下：

```rust
let response: MyType = client
    .post(url)
    .bearer_auth(&api_key)
    .json(&body)        // 将 body 序列化为 JSON
    .send()
    .await
    .context("request failed")?
    .error_for_status() // 将 4xx/5xx 转换为错误
    .context("API returned error status")?
    .json()             // 将响应反序列化为 JSON
    .await
    .context("failed to parse response")?;
```

每个方法返回一个 builder 或 future，你可以链式调用。`?` 运算符在每一步传播错误。

### `impl Into<String>`

多个方法使用 `impl Into<String>` 作为参数类型：

```rust
pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self
```

这接受任何可以转换为 `String` 的类型：`String`、`&str`、`Cow<str>` 等。在方法内部，调用 `.into()` 获取 `String`：

```rust
api_key: api_key.into(),
model: model.into(),
```

### `dotenvy`

`dotenvy` crate 从 `.env` 文件加载环境变量：

```rust
let _ = dotenvy::dotenv(); // 如果 .env 存在则加载，忽略错误
let key = std::env::var("OPENROUTER_API_KEY")?;
```

`let _ =` 丢弃返回值，因为 `.env` 文件不存在也没关系（变量可能已经在环境中了）。

## API 类型

文件 `mini-claw-code-starter/src/providers/openrouter.rs` 开头有一组 serde 结构体。它们表示 OpenAI 兼容的 chat completions API 格式。以下是简要说明：

**请求类型：**
- `ChatRequest` —— POST 请求体：模型名称、消息、工具
- `ApiMessage` —— 单条消息，包含 role、content 和可选的 tool calls
- `ApiTool` / `ApiToolDef` —— API 格式的工具定义

**响应类型：**
- `ChatResponse` —— API 响应：一个 choices 列表
- `Choice` —— 单个选项，包含一条消息和 `finish_reason`
- `ResponseMessage` —— 助手的响应：可选的 content 和可选的 tool calls

`Choice` 上的 `finish_reason` 字段告诉你模型为什么停止生成。在你的 `chat()` 实现中将其映射到 `StopReason`：`"tool_calls"` 对应 `StopReason::ToolUse`，其他值对应 `StopReason::Stop`。

这些类型已经完整实现了。你的任务是实现 *使用* 它们的方法。

## 具体实现

### 第一步：实现 `new()`

初始化全部四个字段：

```rust
pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
    Self {
        client: reqwest::Client::new(),
        api_key: api_key.into(),
        model: model.into(),
        base_url: "https://openrouter.ai/api/v1".into(),
    }
}
```

### 第二步：实现 `base_url()`

一个简单的 builder 方法，用于覆盖 base URL：

```rust
pub fn base_url(mut self, url: impl Into<String>) -> Self {
    self.base_url = url.into();
    self
}
```

### 第三步：实现 `from_env_with_model()`

1. 使用 `dotenvy::dotenv()` 加载 `.env`（忽略返回值）。
2. 从环境变量中读取 `OPENROUTER_API_KEY`。
3. 用密钥和模型调用 `Self::new()`。

使用 `std::env::var("OPENROUTER_API_KEY")` 并链式调用 `.context(...)` 以便在密钥缺失时提供清晰的错误信息。

### 第四步：实现 `from_env()`

这是一行代码，使用默认模型 `"openrouter/free"` 调用 `from_env_with_model`。这是 OpenRouter 上的免费模型 —— 无需充值即可开始使用。

### 第五步：实现 `convert_messages()`

此方法将我们的 `Message` 枚举转换为 API 的 `ApiMessage` 格式。遍历消息并对每个变体进行匹配：

- **`Message::System(text)`** 转换为 role 为 `"system"`、`content: Some(text.clone())` 的 `ApiMessage`。其他字段为 `None`。

- **`Message::User(text)`** 转换为 role 为 `"user"`、`content: Some(text.clone())` 的 `ApiMessage`。其他字段为 `None`。

- **`Message::Assistant(turn)`** 转换为 role 为 `"assistant"` 的 `ApiMessage`。将 `content` 设为 `turn.text.clone()`。如果 `turn.tool_calls` 非空，将每个 `ToolCall` 转换为 `ApiToolCall`：

  ```rust
  ApiToolCall {
      id: c.id.clone(),
      type_: "function".into(),
      function: ApiFunction {
          name: c.name.clone(),
          arguments: c.arguments.to_string(), // Value -> String
      },
  }
  ```

  如果 `tool_calls` 为空，设置 `tool_calls: None`（而非 `Some(vec![])`）。

- **`Message::ToolResult { id, content }`** 转换为 role 为 `"tool"`、`content: Some(content.clone())` 且 `tool_call_id: Some(id.clone())` 的 `ApiMessage`。

### 第六步：实现 `convert_tools()`

将每个 `&ToolDefinition` 映射为 `ApiTool`：

```rust
ApiTool {
    type_: "function",
    function: ApiToolDef {
        name: t.name,
        description: t.description,
        parameters: t.parameters.clone(),
    },
}
```

### 第七步：实现 `chat()`

这是核心方法，它将所有部分整合在一起：

1. 用模型、转换后的消息和转换后的工具构建 `ChatRequest`。
2. 使用 bearer auth 将其 POST 到 `{base_url}/chat/completions`。
3. 将响应解析为 `ChatResponse`。
4. 提取第一个 choice。
5. 将 `tool_calls` 转换回我们的 `ToolCall` 类型。

工具调用的转换是最棘手的部分。API 返回的 `function.arguments` 是一个 *字符串*（JSON 编码），但我们的 `ToolCall` 将其存储为 `serde_json::Value`。因此你需要解析它：

```rust
let arguments = serde_json::from_str(&tc.function.arguments)
    .unwrap_or(Value::Null);
```

`unwrap_or(Value::Null)` 处理参数字符串不是有效 JSON 的情况（对于行为正常的 API 来说不太可能发生，但做好防御总是好的）。

以下是 `chat()` 方法的骨架代码：

```rust
async fn chat(
    &self,
    messages: &[Message],
    tools: &[&ToolDefinition],
) -> anyhow::Result<AssistantTurn> {
    let body = ChatRequest {
        model: &self.model,
        messages: Self::convert_messages(messages),
        tools: Self::convert_tools(tools),
    };

    let response: ChatResponse = self.client
        .post(format!("{}/chat/completions", self.base_url))
        // ... bearer_auth, json, send, error_for_status, json ...
        ;

    let choice = response.choices.into_iter().next()
        .context("no choices in response")?;

    // 将 choice.message.tool_calls 转换为 Vec<ToolCall>
    // 将 finish_reason 映射为 StopReason
    // 返回 AssistantTurn { text, tool_calls, stop_reason }
    todo!()
}
```

补全 HTTP 调用链和响应转换逻辑。

## 运行测试

运行第六章的测试：

```bash
cargo test -p mini-claw-code-starter ch6
```

第六章的测试验证了转换方法（`convert_messages` 和 `convert_tools`）、构造函数逻辑，以及使用本地 mock HTTP 服务器的完整 `chat()` 方法。测试 *不会* 调用真实的 LLM API，因此不需要 API 密钥。还有一些额外的边界情况测试，一旦你的核心实现正确就会通过。

### 可选：实时测试

如果你想使用真实 API 进行测试，请设置 OpenRouter API 密钥：

1. 在 [openrouter.ai](https://openrouter.ai) 注册。
2. 创建 API 密钥。
3. 在工作区根目录创建 `.env` 文件：

```
OPENROUTER_API_KEY=sk-or-v1-your-key-here
```

然后尝试构建并运行第七章的聊天示例。但首先，请读完本章，然后继续第七章，在那里你将把所有东西连接起来。

## 总结

你已经实现了一个真正的 HTTP provider，它能够：

- 通过 API 密钥和模型名称（或从环境变量）构建实例。
- 在内部类型和 OpenAI 兼容的 API 格式之间进行转换。
- 发送 HTTP 请求并解析响应。

关键模式：
- **Serde 属性** 用于 JSON 字段映射（`rename`、`skip_serializing_if`）。
- **`reqwest`** 提供流式 builder API 的 HTTP 客户端。
- **`impl Into<String>`** 实现灵活的字符串参数。
- **`dotenvy`** 用于加载 `.env` 文件。

你的 agent 框架现在已经完整了。每一个部分 —— 工具、agent 循环和 HTTP provider —— 都已实现并通过测试。

## 下一步

在[第七章：简单的 CLI](./ch07-putting-together.md) 中，你将把所有内容连接成一个带有对话记忆的交互式 CLI。

# Chapter 6: The OpenRouter Provider

Up to now, everything has run locally with the `MockProvider`. In this chapter
you will implement `OpenRouterProvider` -- a provider that talks to a real LLM
over HTTP using the OpenAI-compatible chat completions API.

This is the chapter that makes your agent real.

## Goal

Implement `OpenRouterProvider` so that:

1. It can be created with an API key and model name.
2. It converts our internal `Message` and `ToolDefinition` types to the API
   format.
3. It sends HTTP POST requests to the chat completions endpoint.
4. It parses responses back into `AssistantTurn`.

## Key Rust concepts

### Serde derives and attributes

The API types in `openrouter.rs` are already provided -- you do not need to
modify them. But understanding them helps:

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct ApiToolCall {
    pub(crate) id: String,
    #[serde(rename = "type")]
    pub(crate) type_: String,
    pub(crate) function: ApiFunction,
}
```

Key serde attributes used:

- **`#[serde(rename = "type")]`** -- The JSON field is called `"type"`, but
  `type` is a reserved keyword in Rust. So the struct field is `type_` and
  serde renames it during serialization/deserialization.

- **`#[serde(skip_serializing_if = "Option::is_none")]`** -- Omits the field
  from JSON if the value is `None`. This is important because the API expects
  certain fields to be absent (not `null`) when unused.

- **`#[serde(skip_serializing_if = "Vec::is_empty")]`** -- Same idea for
  empty vectors. If there are no tools, we omit the `tools` field entirely.

### The `reqwest` HTTP client

`reqwest` is the standard HTTP client crate in Rust. The pattern:

```rust
let response: MyType = client
    .post(url)
    .bearer_auth(&api_key)
    .json(&body)        // serialize body as JSON
    .send()
    .await
    .context("request failed")?
    .error_for_status() // turn 4xx/5xx into errors
    .context("API returned error status")?
    .json()             // deserialize response as JSON
    .await
    .context("failed to parse response")?;
```

Each method returns a builder or future that you chain together. The `?`
operator propagates errors at each step.

### `impl Into<String>`

Several methods use `impl Into<String>` as a parameter type:

```rust
pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self
```

This accepts anything that can be converted into a `String`: `String`, `&str`,
`Cow<str>`, etc. Inside the method, call `.into()` to get the `String`:

```rust
api_key: api_key.into(),
model: model.into(),
```

### `dotenvy`

The `dotenvy` crate loads environment variables from a `.env` file:

```rust
let _ = dotenvy::dotenv(); // loads .env if present, ignores errors
let key = std::env::var("OPENROUTER_API_KEY")?;
```

The `let _ =` discards the result because it is fine if `.env` does not exist
(the variable might already be in the environment).

## The API types

The file `mini-claw-code-starter/src/providers/openrouter.rs` starts with a block
of serde structs. These represent the OpenAI-compatible chat completions API
format. Here is a quick summary:

**Request types:**
- `ChatRequest` -- the POST body: model name, messages, tools
- `ApiMessage` -- a single message with role, content, optional tool calls
- `ApiTool` / `ApiToolDef` -- tool definition in API format

**Response types:**
- `ChatResponse` -- the API response: a list of choices
- `Choice` -- a single choice containing a message and a `finish_reason`
- `ResponseMessage` -- the assistant's response: optional content, optional
  tool calls

The `finish_reason` field on `Choice` tells you why the model stopped
generating. Map it to `StopReason` in your `chat()` implementation:
`"tool_calls"` becomes `StopReason::ToolUse`, anything else becomes
`StopReason::Stop`.

These are already complete. Your job is to implement the methods that *use*
them.

## The implementation

### Step 1: Implement `new()`

Initialize all four fields:

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

### Step 2: Implement `base_url()`

A simple builder method that overrides the base URL:

```rust
pub fn base_url(mut self, url: impl Into<String>) -> Self {
    self.base_url = url.into();
    self
}
```

### Step 3: Implement `from_env_with_model()`

1. Load `.env` with `dotenvy::dotenv()` (ignore the result).
2. Read `OPENROUTER_API_KEY` from the environment.
3. Call `Self::new()` with the key and model.

Use `std::env::var("OPENROUTER_API_KEY")` and chain `.context(...)` for a
clear error message if the key is missing.

### Step 4: Implement `from_env()`

This is a one-liner that calls `from_env_with_model` with the default model
`"openrouter/free"`. This is a free model on OpenRouter -- no credits needed
to get started.

### Step 5: Implement `convert_messages()`

This method translates our `Message` enum into the API's `ApiMessage` format.
Iterate over the messages and match on each variant:

- **`Message::System(text)`** becomes an `ApiMessage` with role `"system"` and
  `content: Some(text.clone())`. The other fields are `None`.

- **`Message::User(text)`** becomes an `ApiMessage` with role `"user"` and
  `content: Some(text.clone())`. The other fields are `None`.

- **`Message::Assistant(turn)`** becomes an `ApiMessage` with role `"assistant"`.
  Set `content` to `turn.text.clone()`. If `turn.tool_calls` is non-empty,
  convert each `ToolCall` to an `ApiToolCall`:

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

  If `tool_calls` is empty, set `tool_calls: None` (not `Some(vec![])`).

- **`Message::ToolResult { id, content }`** becomes an `ApiMessage` with role
  `"tool"`, `content: Some(content.clone())`, and `tool_call_id: Some(id.clone())`.

### Step 6: Implement `convert_tools()`

Map each `&ToolDefinition` to an `ApiTool`:

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

### Step 7: Implement `chat()`

This is the main method. It brings everything together:

1. Build a `ChatRequest` with the model, converted messages, and converted tools.
2. POST it to `{base_url}/chat/completions` with bearer auth.
3. Parse the response as `ChatResponse`.
4. Extract the first choice.
5. Convert `tool_calls` back to our `ToolCall` type.

The tool call conversion is the trickiest part. The API returns
`function.arguments` as a *string* (JSON-encoded), but our `ToolCall` stores
it as a `serde_json::Value`. So you need to parse it:

```rust
let arguments = serde_json::from_str(&tc.function.arguments)
    .unwrap_or(Value::Null);
```

The `unwrap_or(Value::Null)` handles the case where the arguments string is
not valid JSON (unlikely with a well-behaved API, but good to be safe).

Here is the skeleton for the `chat()` method:

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

    // Convert choice.message.tool_calls to Vec<ToolCall>
    // Map finish_reason to StopReason
    // Return AssistantTurn { text, tool_calls, stop_reason }
    todo!()
}
```

Fill in the HTTP call chain and the response conversion logic.

## Running the tests

Run the Chapter 6 tests:

```bash
cargo test -p mini-claw-code-starter ch6
```

The Chapter 6 tests verify the conversion methods (`convert_messages` and
`convert_tools`), the constructor logic, and the full `chat()` method using a
local mock HTTP server. They do *not* call a real LLM API, so no API key is
needed. There are also additional edge-case tests that will pass once your core
implementation is correct.

### Optional: Live test

If you want to test with a real API, set up an OpenRouter API key:

1. Sign up at [openrouter.ai](https://openrouter.ai).
2. Create an API key.
3. Create a `.env` file in the workspace root:

```
OPENROUTER_API_KEY=sk-or-v1-your-key-here
```

Then try building and running the chat example from Chapter 7. But first,
finish reading this chapter and move on to Chapter 7 where you wire everything
up.

## Recap

You have implemented a real HTTP provider that:

- Constructs from an API key and model name (or from environment variables).
- Converts between your internal types and the OpenAI-compatible API format.
- Sends HTTP requests and parses responses.

The key patterns:
- **Serde attributes** for JSON field mapping (`rename`, `skip_serializing_if`).
- **`reqwest`** for HTTP with a fluent builder API.
- **`impl Into<String>`** for flexible string parameters.
- **`dotenvy`** for loading `.env` files.

Your agent framework is now complete. Every piece -- tools, the agent loop,
and the HTTP provider -- is implemented and tested.

## What's next

In [Chapter 7: A Simple CLI](./ch07-putting-together.md) you will wire
everything into an interactive CLI with conversation memory.

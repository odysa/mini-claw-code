# Chapter 2: Provider & Streaming

> **File(s) to edit:** `src/mock.rs`, `src/streaming.rs`, `src/providers/openrouter.rs`
> **Tests to run:** `cargo test -p mini-claw-code-starter test_ch1` (MockProvider), `cargo test -p mini-claw-code-starter test_ch6` (OpenRouterProvider), `cargo test -p mini-claw-code-starter test_ch10` (streaming)

In Chapter 1 we defined the data that flows through our agent. Now we need something to *drive* that data -- an LLM backend that takes a conversation and returns an assistant response. In this chapter you will build:

1. A `Provider` trait that abstracts over any LLM backend
2. A `StreamProvider` trait that adds real-time token streaming via channels
3. A `StreamEvent` enum describing the incremental pieces of a streamed response
4. A `MockProvider` for deterministic testing without network calls
5. A `MockStreamProvider` that synthesizes stream events from canned responses
6. An SSE line parser and a `StreamAccumulator` that reassembles events into a complete message
7. The `OpenRouterProvider` that talks to a real API

By the end, the mock provider tests in `test_ch1`, the OpenRouter tests in `test_ch6`, and the streaming tests in `test_ch10` should pass.

```bash
cargo test -p mini-claw-code-starter test_ch1
cargo test -p mini-claw-code-starter test_ch6
cargo test -p mini-claw-code-starter test_ch10
```

---

## Why a trait?

A coding agent needs to call an LLM, but *which* LLM should not be hard-coded. During tests we want instant, deterministic responses. In production we want streaming over HTTP. The `Provider` trait gives us that seam.

Claude Code uses a similar abstraction internally -- every LLM call goes through a provider interface, and the choice of backend (Anthropic API, Bedrock, Vertex) is resolved at startup.

## The Provider trait (RPITIT)

Here is the full trait:

```rust
pub trait Provider: Send + Sync {
    fn chat<'a>(
        &'a self,
        messages: &'a [Message],
        tools: &'a [&'a ToolDefinition],
    ) -> impl Future<Output = anyhow::Result<AssistantTurn>> + Send + 'a;
}
```

A few things to notice:

**No `#[async_trait]`.** Older Rust code uses the `async_trait` crate to work around the fact that traits could not have `async fn` methods. Since Rust 1.75, the language supports *return-position `impl Trait` in traits* (RPITIT). Instead of writing `async fn chat(...)`, we write `fn chat(...) -> impl Future<...>` and the compiler handles the rest. The effect is the same -- callers can `.await` the return value -- but we avoid a heap allocation that `async_trait` required (it boxed every future).

We use RPITIT rather than `async fn` in the trait signature because it gives us explicit control over the lifetime and `Send` bound. Writing `async fn` in a trait works, but today it does not automatically infer `Send` for the returned future, which means you cannot spawn the future onto a multi-threaded runtime. The explicit `impl Future<...> + Send + 'a` signature solves that.

**Why `Send + Sync` on the trait itself?** Our agent loop will hold a `P: Provider` behind a shared reference (and later behind `Arc`). The `Sync` bound lets multiple tasks share the provider, and `Send` lets it cross thread boundaries.

**Lifetime `'a` everywhere.** The returned future borrows both `&self` and the input slices. Tying them to a single lifetime `'a` tells the compiler the future lives no longer than those borrows, avoiding `'static` requirements.

The `Provider` trait is already defined in `src/types.rs` (Chapter 1). The starter puts it alongside the message types because everything lives in a flat layout.

## The Arc\<P\> blanket impl

Directly below the `Provider` trait, add this:

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

This says: "if `P` is a `Provider`, then `Arc<P>` is also a `Provider`." It just dereferences through the `Arc` and delegates to the inner value.

Why does this matter? Later, when we build subagents, the main agent and its subagents will share the same provider. Cloning an `Arc` is cheap, and the blanket impl means subagent code that is generic over `P: Provider` works identically whether it receives a bare provider or a shared one. Without this impl, you would need separate type plumbing to pass shared providers around.

Both the `Provider` trait and the `Arc<P>` blanket impl are already in `src/types.rs`.

## StreamEvent

Before defining the streaming trait, we need a vocabulary for the incremental chunks an LLM sends back:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    /// A fragment of the model's text response.
    TextDelta(String),
    /// The beginning of a tool call (carries the call ID and tool name).
    ToolCallStart {
        index: usize,
        id: String,
        name: String,
    },
    /// A fragment of a tool call's JSON arguments.
    ToolCallDelta {
        index: usize,
        arguments: String,
    },
    /// The stream is complete.
    Done,
}
```

These four variants map directly to the OpenAI streaming API:

- **TextDelta** -- a fragment of the model's natural-language output (e.g. `"Hello"`, then `" world"`).
- **ToolCallStart** -- the model has begun a tool call. `index` identifies which call (a single turn can request multiple tools), `id` is a server-assigned correlation ID, and `name` is the tool.
- **ToolCallDelta** -- a fragment of the JSON arguments for the call at `index`. Arguments arrive incrementally because the model generates JSON token-by-token.
- **Done** -- end-of-stream signal.

The `index` field matters because streaming interleaves fragments from multiple tool calls, and consumers need to know which call each fragment belongs to.

## The StreamProvider trait

```rust
pub trait StreamProvider: Send + Sync {
    fn stream_chat<'a>(
        &'a self,
        messages: &'a [Message],
        tools: &'a [&'a ToolDefinition],
        tx: mpsc::UnboundedSender<StreamEvent>,
    ) -> impl Future<Output = anyhow::Result<AssistantTurn>> + Send + 'a;
}
```

The design uses a **channel-based** streaming model rather than returning an `AsyncIterator` or `Stream`. The caller creates a `tokio::sync::mpsc::unbounded_channel()`, passes the sender half to `stream_chat`, and reads events from the receiver half -- typically in a separate task that renders them to the terminal.

The method itself still returns the fully assembled `AssistantTurn` when the stream is complete. This means the agent loop always gets a clean `AssistantTurn` to work with, regardless of whether streaming is enabled. The channel is a side-channel for the UI.

Why `UnboundedSender` instead of a bounded channel? Streaming events are tiny and arrive at network speed, not faster. Backpressure is unnecessary because the bottleneck is the API, not the consumer. An unbounded channel keeps the API simpler.

### Your task

The `StreamEvent` enum, `StreamProvider` trait, `StreamAccumulator`, and `parse_sse_line` all live in `src/streaming.rs` in the starter. Open that file -- you will see `unimplemented!()` stubs with doc comments. Fill them in as described below.

---

## MockProvider

Testing an agent against a live API is slow, expensive, and nondeterministic. The `MockProvider` lets you script exact responses and verify that your agent handles them correctly.

```rust
use std::collections::VecDeque;
use std::sync::Mutex;

pub struct MockProvider {
    responses: Mutex<VecDeque<AssistantTurn>>,
}

impl MockProvider {
    pub fn new(responses: VecDeque<AssistantTurn>) -> Self {
        Self {
            responses: Mutex::new(responses),
        }
    }
}

impl Provider for MockProvider {
    async fn chat(
        &self,
        _messages: &[Message],
        _tools: &[&ToolDefinition],
    ) -> anyhow::Result<AssistantTurn> {
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| anyhow::anyhow!("MockProvider: no more responses"))
    }
}
```

The design:

- **`VecDeque`** -- responses are consumed in FIFO order. The first call to `chat` returns the first response, the second call returns the second, and so on.
- **`Mutex`** -- the `Provider` trait takes `&self` (not `&mut self`), because providers are shared. But we need to mutate the queue. A `std::sync::Mutex` (not `tokio::sync::Mutex`) is fine here because the critical section is trivial -- just a `pop_front`. Holding a `std::sync::Mutex` briefly across an `.await` point is acceptable when the lock is never contended.
- **Error on exhaustion** -- if the test scripts three responses but the agent calls `chat` a fourth time, it gets an error instead of a silent panic. This catches agent loops that spin more times than expected.

### Testing strategy

The `MockProvider` is the foundation of all our tests. By scripting the exact sequence of responses, you can test:

- **Single-turn**: one response with `StopReason::Stop`
- **Tool use loops**: first response has `StopReason::ToolUse` with tool calls, the agent executes them and sends results back, second response has `StopReason::Stop`
- **Multi-turn sequences**: any number of scripted turns
- **Error handling**: an empty queue returns an error

Look at the chapter 2 tests to see this in action:

```rust
#[tokio::test]
async fn test_ch2_mock_returns_text() {
    let provider = MockProvider::new(VecDeque::from([AssistantTurn {
        text: Some("Hello!".into()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    }]));
    let turn = provider.chat(&[Message::User("Hi".into())], &[]).await.unwrap();
    assert_eq!(turn.text.as_deref(), Some("Hello!"));
}
```

Notice that the test ignores the `messages` input -- the mock does not look at what the agent sends. This is intentional. You are testing the *agent's behavior* given a known provider response, not the provider's ability to understand prompts.

### Your task

Open `src/mock.rs` in the starter. You will see the `MockProvider` struct with `unimplemented!()` stubs. Fill in `new()` and the `Provider` impl.

---

## MockStreamProvider

The `MockStreamProvider` wraps a `MockProvider` and synthesizes `StreamEvent`s from each canned response. This lets you test UI code that consumes stream events without needing a real HTTP connection.

The struct wraps a `MockProvider` and its `stream_chat` impl works in three steps:

1. Delegate to `self.inner.chat()` to get the canned `AssistantTurn`
2. Decompose it into events: text is sent **character-by-character** as `TextDelta` events, each tool call emits a `ToolCallStart` + single `ToolCallDelta`, and a final `Done` is sent
3. Return the original `AssistantTurn` unchanged

Here is the full implementation:

```rust
pub struct MockStreamProvider {
    inner: MockProvider,
}

impl MockStreamProvider {
    pub fn new(responses: VecDeque<AssistantTurn>) -> Self {
        Self {
            inner: MockProvider::new(responses),
        }
    }
}

impl StreamProvider for MockStreamProvider {
    async fn stream_chat(
        &self,
        messages: &[Message],
        tools: &[&ToolDefinition],
        tx: mpsc::UnboundedSender<StreamEvent>,
    ) -> anyhow::Result<AssistantTurn> {
        let turn = self.inner.chat(messages, tools).await?;

        // Synthesize stream events from the complete turn
        if let Some(ref text) = turn.text {
            for ch in text.chars() {
                let _ = tx.send(StreamEvent::TextDelta(ch.to_string()));
            }
        }
        for (i, call) in turn.tool_calls.iter().enumerate() {
            let _ = tx.send(StreamEvent::ToolCallStart {
                index: i,
                id: call.id.clone(),
                name: call.name.clone(),
            });
            let _ = tx.send(StreamEvent::ToolCallDelta {
                index: i,
                arguments: call.arguments.to_string(),
            });
        }
        let _ = tx.send(StreamEvent::Done);

        Ok(turn)
    }
}
```

This avoids duplicating the response queue logic -- the `inner.chat()` call handles the `VecDeque` pop. The `let _ = tx.send(...)` pattern intentionally ignores send errors -- if the receiver is dropped, nobody is listening, and that is fine.

### Your task

The `MockStreamProvider` is in `src/streaming.rs` in the starter. Fill in its `new()` and `stream_chat()` stubs.

---

## Server-Sent Events and parse\_sse\_line

When the real provider requests `stream: true`, the API returns a stream of [Server-Sent Events](https://developer.mozilla.org/en-US/docs/Web/API/Server-sent_events/Using_server-sent_events) (SSE). SSE is a simple text protocol over HTTP:

```
data: {"choices":[{"delta":{"content":"Hello"},"finish_reason":null}]}

data: {"choices":[{"delta":{"content":" world"},"finish_reason":null}]}

data: [DONE]
```

Each event is a line starting with `data: ` followed by a JSON payload (or the special string `[DONE]`). Events are separated by blank lines. That is the entire protocol -- no framing, no length prefixes, just newline-delimited text. This simplicity is why SSE is the standard for LLM streaming.

Our `parse_sse_line` function handles a single line:

```rust
pub fn parse_sse_line(line: &str) -> Option<Vec<StreamEvent>> {
    let data = line.strip_prefix("data: ")?;
    if data == "[DONE]" {
        return Some(vec![StreamEvent::Done]);
    }

    let chunk: ChunkResponse = serde_json::from_str(data).ok()?;
    let choice = chunk.choices.into_iter().next()?;
    let mut events = Vec::new();

    if let Some(text) = choice.delta.content
        && !text.is_empty()
    {
        events.push(StreamEvent::TextDelta(text));
    }

    if let Some(tool_calls) = choice.delta.tool_calls {
        for tc in tool_calls {
            if let Some(id) = tc.id {
                let name = tc.function
                    .as_ref()
                    .and_then(|f| f.name.clone())
                    .unwrap_or_default();
                events.push(StreamEvent::ToolCallStart {
                    index: tc.index,
                    id,
                    name,
                });
            }
            if let Some(ref func) = tc.function
                && let Some(ref args) = func.arguments
                && !args.is_empty()
            {
                events.push(StreamEvent::ToolCallDelta {
                    index: tc.index,
                    arguments: args.clone(),
                });
            }
        }
    }

    if events.is_empty() { None } else { Some(events) }
}
```

Walk through the logic:

1. **Strip the `data: ` prefix.** Lines that do not start with `data: ` (like `event: ping` or blank lines) return `None` -- they are not data events.
2. **Check for `[DONE]`.** This is the OpenAI-standard end-of-stream sentinel. Return a `Done` event.
3. **Parse JSON into `ChunkResponse`.** If the JSON is malformed, `.ok()?` silently skips it. This is intentional -- SSE streams occasionally include keep-alive pings or malformed chunks, and crashing would be worse than dropping a token.
4. **Extract text deltas.** The `delta.content` field contains the text fragment. Empty strings are skipped.
5. **Extract tool call events.** A single chunk can contain both a `ToolCallStart` (when the `id` field is present, signaling a new call) and a `ToolCallDelta` (when `arguments` is present). The `if let ... && let ...` syntax is Rust's let-chains feature, stabilized in edition 2024.

The tests verify the parser against three cases: a text delta line produces `StreamEvent::TextDelta("Hello")`, the `data: [DONE]` line produces `StreamEvent::Done`, and non-data lines like `event: ping` or empty strings return `None`.

### Your task

The `parse_sse_line` function and its SSE deserialization types (`ChunkResponse`, `ChunkChoice`, `Delta`, `DeltaToolCall`, `DeltaFunction`) are in `src/streaming.rs`. Fill in the `parse_sse_line` stub.

---

## StreamAccumulator

Streaming gives the UI real-time output, but the agent loop needs a complete `AssistantTurn` to decide what to do next. The `StreamAccumulator` bridges this gap -- it collects events as they arrive and produces a finished message at the end.

```rust
pub struct StreamAccumulator {
    text: String,
    tool_calls: Vec<PartialToolCall>,
}

struct PartialToolCall {
    id: String,
    name: String,
    arguments: String,
}
```

The two key methods:

```rust
impl StreamAccumulator {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            tool_calls: Vec::new(),
        }
    }

    pub fn feed(&mut self, event: &StreamEvent) {
        match event {
            StreamEvent::TextDelta(s) => self.text.push_str(s),
            StreamEvent::ToolCallStart { index, id, name } => {
                // Ensure the Vec is large enough for this index
                while self.tool_calls.len() <= *index {
                    self.tool_calls.push(PartialToolCall {
                        id: String::new(),
                        name: String::new(),
                        arguments: String::new(),
                    });
                }
                self.tool_calls[*index].id = id.clone();
                self.tool_calls[*index].name = name.clone();
            }
            StreamEvent::ToolCallDelta { index, arguments } => {
                if let Some(tc) = self.tool_calls.get_mut(*index) {
                    tc.arguments.push_str(arguments);
                }
            }
            StreamEvent::Done => {}
        }
    }

    pub fn finish(self) -> AssistantTurn {
        let text = if self.text.is_empty() {
            None
        } else {
            Some(self.text)
        };
        let tool_calls: Vec<ToolCall> = self
            .tool_calls
            .into_iter()
            .filter(|tc| !tc.name.is_empty())
            .map(|tc| ToolCall {
                id: tc.id,
                name: tc.name,
                arguments: serde_json::from_str(&tc.arguments)
                    .unwrap_or(Value::Null),
            })
            .collect();
        let stop_reason = if tool_calls.is_empty() {
            StopReason::Stop
        } else {
            StopReason::ToolUse
        };
        AssistantTurn {
            id: new_id(),
            text,
            tool_calls,
            stop_reason,
            usage: None,
        }
    }
}
```

Design notes:

- **`feed` appends incrementally.** Text fragments concatenate into `self.text`. Tool call arguments concatenate per-index into `PartialToolCall::arguments`.
- **Sparse index handling.** The `while` loop in `ToolCallStart` pads the vector with empty entries so that `index: 2` works even if the vector only has one element. The `filter(|tc| !tc.name.is_empty())` in `finish` strips those placeholders.
- **Deferred JSON parsing.** Arguments arrive as string fragments during streaming. `finish` parses the concatenated string into `serde_json::Value` only after the stream ends, falling back to `Value::Null` on malformed JSON.
- **`stop_reason` is derived from the tool calls.** If any survived the filter, it is `ToolUse`; otherwise `Stop`. Usage is `None` because most streaming APIs do not include token counts per chunk.

The `test_ch2_accumulator_text` test feeds two `TextDelta` events and verifies the concatenated result. The `test_ch2_accumulator_tool_call` test feeds a `ToolCallStart` followed by two `ToolCallDelta` fragments (`{"path":` and `"test.txt"}`) and verifies they are reassembled into a valid `ToolCall` with parsed JSON arguments.

### Your task

The `StreamAccumulator` and `PartialToolCall` are in `src/streaming.rs`. Fill in the `new()`, `feed()`, and `finish()` stubs.

---

## OpenRouterProvider

With the parsing infrastructure in place, we can build the real provider. It targets the [OpenRouter](https://openrouter.ai/) API, which is OpenAI-compatible -- the same request/response format works with OpenAI, Together, Groq, and many others.

### API types

The provider needs serde types for the request and response payloads. Here is the request side:

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

The `skip_serializing_if` annotations keep the JSON clean -- `tools` is omitted when empty (some models choke on an empty array), and `stream` is omitted when `false` (the default for the API).

`ApiMessage`, `ApiToolCall`, `ApiFunction`, `ApiTool`, and `ApiToolDef` mirror the OpenAI message format. The response types (`ChatResponse`, `Choice`, `ResponseMessage`) deserialize the non-streaming response. The chunk types (`ChunkResponse`, `ChunkChoice`, `Delta`, `DeltaToolCall`, `DeltaFunction`) deserialize the streaming response -- you already implemented those for `parse_sse_line`.

### Conversion helpers

Two `impl` methods on `OpenRouterProvider` translate between our internal types and the API format:

- **`convert_messages`** maps each `Message` variant to its OpenAI role. `System` and `User` are straightforward. `Assistant` carries optional `tool_calls` (with arguments serialized back to a JSON string via `.to_string()`). `ToolResult` becomes `role: "tool"` with `tool_call_id` linking it to the original call. `Attachment` is injected as a `system` message since the OpenAI format has no native attachment role. `Progress` is silently dropped -- it is UI-only.
- **`convert_tools`** wraps each `ToolDefinition` in the OpenAI function-calling envelope: `{ "type": "function", "function": { "name", "description", "parameters" } }`.

### The provider struct

```rust
pub struct OpenRouterProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
    base_url: String,
}
```

The struct holds a reusable `reqwest::Client`, the API key, model name, and base URL. Constructors include `new(api_key, model)` for explicit creation, `from_env()` which loads `OPENROUTER_API_KEY` via `dotenvy`, and a `base_url(self, url)` builder method for overriding the endpoint (useful for local testing or alternative providers).

### Non-streaming impl

The `Provider` impl builds a `ChatRequest` with `stream: false`, POSTs it to `/chat/completions`, and deserializes the response into `ChatResponse`. A `parse_assistant` helper converts the API's `Choice` into our `AssistantTurn`, mapping `finish_reason: "tool_calls"` to `StopReason::ToolUse` and parsing tool call arguments from JSON strings into `Value`.

### Streaming impl

The `StreamProvider` impl is the heart of the streaming architecture. It sends the same request with `stream: true`, then processes the chunked HTTP response. Here is the core loop (abbreviated):

```rust
let mut acc = StreamAccumulator::new();
let mut buffer = String::new();

while let Some(chunk) = resp.chunk().await? {
    buffer.push_str(&String::from_utf8_lossy(&chunk));
    while let Some(pos) = buffer.find('\n') {
        let line = buffer[..pos].trim_end_matches('\r').to_string();
        buffer = buffer[pos + 1..].to_string();
        if line.is_empty() { continue; }
        if let Some(events) = parse_sse_line(&line) {
            for event in events {
                acc.feed(&event);
                let _ = tx.send(event);
            }
        }
    }
}
Ok(acc.finish())
```

Walk through it:

1. **Same request, but `stream: true`.** The API returns a chunked HTTP response instead of a single JSON body.
2. **Read raw byte chunks.** `resp.chunk()` returns `Option<Bytes>` -- the HTTP body arrives in arbitrary-sized pieces that do not align with SSE event boundaries.
3. **Buffer and split on newlines.** TCP chunks can split an SSE line in the middle. The `buffer` accumulates raw text, and the inner `while` loop extracts complete lines. This is classic line-oriented protocol parsing -- you accumulate bytes and consume lines as they become available.
4. **Parse each line.** `parse_sse_line` converts a `data:` line into `StreamEvent`s. Blank lines and non-data lines are skipped.
5. **Feed both the accumulator and the channel.** The accumulator builds the final `AssistantTurn`; the channel delivers events to the UI in real-time.
6. **Return the assembled message.** Once the stream ends (`resp.chunk()` returns `None`), the accumulator has collected everything, and `finish()` produces the `AssistantTurn`.

This dual-path design (accumulator + channel) is how Claude Code handles streaming too. The UI renders tokens as they arrive, but the agent loop sees a clean, complete response.

### Your task

The `OpenRouterProvider` lives in `src/providers/openrouter.rs`. Fill in the constructor, conversion helpers, the `Provider` impl, and the `StreamProvider` impl. The required dependencies (`reqwest`, `dotenvy`) are already in `Cargo.toml`.

---

## Putting it all together

The provider-related code lives across three files in the starter:

| File | Contents |
|------|----------|
| `src/types.rs` | `Provider` trait, `Arc<P>` blanket impl |
| `src/mock.rs` | `MockProvider` |
| `src/streaming.rs` | `StreamEvent`, `StreamProvider`, `MockStreamProvider`, `parse_sse_line`, `StreamAccumulator` |
| `src/providers/openrouter.rs` | `OpenRouterProvider` |

---

## Run the tests

```bash
cargo test -p mini-claw-code-starter test_ch1   # MockProvider tests
cargo test -p mini-claw-code-starter test_ch6   # OpenRouterProvider tests
cargo test -p mini-claw-code-starter test_ch10  # Streaming tests
```

The MockProvider tests are in `test_ch1`, the OpenRouterProvider tests are in `test_ch6`, and the streaming tests (SSE parsing, accumulator) are in `test_ch10`.

---

## Recap

You built the LLM abstraction layer. The `Provider` and `StreamProvider` traits decouple the agent from any specific backend. The `MockProvider` enables deterministic testing. The SSE parser and `StreamAccumulator` handle the real-time streaming protocol. And the `Arc<P>` blanket impl prepares you for provider sharing in later chapters.

In Chapter 3, you will explore the `Tool` trait -- the other half of the agent's interface with the outside world.

# Chapter 5b: OpenRouter & StreamingAgent

> **File(s) to edit:** `src/providers/openrouter.rs`, `src/streaming.rs` (the `StreamingAgent` block at the bottom)
> **Tests to run:** `cargo test -p mini-claw-code-starter test_openrouter_`, `cargo test -p mini-claw-code-starter test_streaming_streaming_agent_`, `cargo test -p mini-claw-code-starter test_streaming_stream_chat_`
> **Estimated time:** 35 min

## Goal

- Implement `OpenRouterProvider` so the agent can talk to a real OpenAI-compatible API — both non-streaming and streaming.
- Implement `StreamingAgent::chat` — the agent loop that forwards streaming text deltas to a UI channel while running tools.

[Chapter 5a](./ch05a-provider-foundations.md) built the abstractions (`Provider`, `StreamProvider`, `StreamEvent`), the mocks (`MockProvider`, `MockStreamProvider`), and the parse/accumulate machinery (`parse_sse_line`, `StreamAccumulator`). This chapter plugs those pieces into a real HTTP provider and wires a streaming channel through the agent loop.

If anything below assumes `parse_sse_line` or `StreamAccumulator` exists — it does, because you implemented it in 5a.

---

## OpenRouterProvider

With the parsing infrastructure in place, we can build the real provider. It targets the [OpenRouter](https://openrouter.ai/) API, which is OpenAI-compatible — the same request/response format works with OpenAI, Together, Groq, and many others.

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

The `skip_serializing_if` annotations keep the JSON clean — `tools` is omitted when empty (some models choke on an empty array), and `stream` is omitted when `false` (the default for the API).

`ApiMessage`, `ApiToolCall`, `ApiFunction`, `ApiTool`, and `ApiToolDef` mirror the OpenAI message format. The response types (`ChatResponse`, `Choice`, `ResponseMessage`) deserialize the non-streaming response. The chunk types (`ChunkResponse`, `ChunkChoice`, `Delta`, `DeltaToolCall`, `DeltaFunction`) deserialize the streaming response — you already implemented those in 5a for `parse_sse_line`.

### Conversion helpers

Two `impl` methods on `OpenRouterProvider` translate between our internal types and the API format:

- **`convert_messages`** maps each `Message` variant to its OpenAI role. `System` and `User` are straightforward. `Assistant` carries optional `tool_calls` (with arguments serialized back to a JSON string via `.to_string()`). `ToolResult` becomes `role: "tool"` with `tool_call_id` linking it to the original call. `Attachment` is injected as a `system` message since the OpenAI format has no native attachment role. `Progress` is silently dropped — it is UI-only.
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

### Non-streaming `Provider` impl

The `Provider` impl builds a `ChatRequest` with `stream: false`, POSTs it to `/chat/completions`, and deserializes the response into `ChatResponse`. A `parse_assistant` helper converts the API's `Choice` into our `AssistantTurn`, mapping `finish_reason: "tool_calls"` to `StopReason::ToolUse` and parsing tool call arguments from JSON strings into `Value`.

### Streaming `StreamProvider` impl

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
2. **Read raw byte chunks.** `resp.chunk()` returns `Option<Bytes>` — the HTTP body arrives in arbitrary-sized pieces that do not align with SSE event boundaries.
3. **Buffer and split on newlines.** TCP chunks can split an SSE line in the middle. The `buffer` accumulates raw text, and the inner `while` loop extracts complete lines. This is classic line-oriented protocol parsing — you accumulate bytes and consume lines as they become available.
4. **Parse each line.** `parse_sse_line` converts a `data:` line into `StreamEvent`s. Blank lines and non-data lines are skipped.
5. **Feed both the accumulator and the channel.** The accumulator builds the final `AssistantTurn`; the channel delivers events to the UI in real-time.
6. **Return the assembled message.** Once the stream ends (`resp.chunk()` returns `None`), the accumulator has collected everything, and `finish()` produces the `AssistantTurn`.

This dual-path design (accumulator + channel) is how Claude Code handles streaming too. The UI renders tokens as they arrive, but the agent loop sees a clean, complete response.

### Your task

The `OpenRouterProvider` lives in `src/providers/openrouter.rs`. Fill in the constructor, conversion helpers, the `Provider` impl, and the `StreamProvider` impl. The required dependencies (`reqwest`, `dotenvy`) are already in `Cargo.toml`.

---

## StreamingAgent

With streaming working at the provider level, we need an agent loop that benefits from it. Streaming an LLM reply *into a provider* is only useful if the text reaches the *user's terminal* as it arrives. That wiring is `StreamingAgent`.

`StreamingAgent` is the streaming counterpart of `SimpleAgent` from Chapter 3:

- `SimpleAgent::chat` calls `provider.chat()` and returns a complete `AssistantTurn`.
- `StreamingAgent::chat` calls `provider.stream_chat()`, **forwards text deltas to a UI channel while the LLM is still generating**, and then returns the assembled response once the stream finishes.

The struct and builder look identical to `SimpleAgent`:

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

`run()` is a thin wrapper around `chat()`. The real work is `chat()`, and it is this chapter's most subtle piece of code.

### The two channels, and the problem they solve

`StreamingAgent::chat` sits between two channels that speak *different vocabularies*:

- **Downstream (provider → agent):** the provider speaks `StreamEvent` — raw stream fragments including `TextDelta`, `ToolCallStart`, `ToolCallDelta`, and `Done`. All the low-level grammar of a streaming LLM response.
- **Upstream (agent → UI):** the UI wants `AgentEvent` — agent-level notifications: `TextDelta` for displayable text, `ToolCall` when a tool starts running, `Done` when the whole conversation finishes, `Error` if something blows up.

`StreamingAgent::chat` is the translator. It has to:

1. Hand the provider a `StreamEvent` channel so the provider can send deltas into it.
2. **Concurrently** pull from that channel, filter `TextDelta`s, and re-emit them as `AgentEvent::TextDelta` on the UI channel — all while the provider is still generating.
3. Wait for the provider to return the assembled `AssistantTurn`.
4. Decide: if the turn ended in `Stop`, emit `AgentEvent::Done` and return; if it ended in `ToolUse`, emit a `ToolCall` event per call, run the tools, append results, and loop.

The critical word is **concurrently** in step 2. We cannot `recv()` events after `stream_chat` returns — by then the generation is over and the UI has been waiting on a frozen screen. We need a separate task pulling from the stream channel *while* the provider is still writing into it.

### The forwarder-task pattern

Here is the full `chat()` implementation:

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

Step-by-step:

1. **Fresh channel per loop iteration.** A new `mpsc::unbounded_channel()` every turn. We cannot reuse one across tool rounds — dropping `stream_tx` is how the forwarder knows the turn is over (see step 4). If we kept the same channel, the forwarder would never exit.

2. **Spawn the forwarder.** `tokio::spawn` runs a task concurrently with the current one. The forwarder loops on `stream_rx.recv().await`, filtering `StreamEvent::TextDelta` into `AgentEvent::TextDelta`. Everything else is dropped — `ToolCallStart`/`ToolCallDelta`/`Done` don't show up in the UI as text. We clone the `events` sender before moving it into the task because we still need the original to send `ToolCall`/`Done`/`Error` after the forwarder exits.

3. **Call `stream_chat` and wait.** The provider is now writing `StreamEvent`s into `stream_tx`. The forwarder pulls them off as they arrive and relays text to the UI. Meanwhile the current task is blocked on the `stream_chat` future. Three tasks are making progress at once: the HTTP response reader, the forwarder, and (via the channel) the UI renderer.

4. **Await the forwarder.** When `stream_chat` returns, its local copy of `stream_tx` is dropped. That closes the channel, which makes `stream_rx.recv()` return `None`, which ends the forwarder's `while let` loop. Awaiting the `JoinHandle` does two things: it guarantees the forwarder has flushed every last delta to the UI before we move on, and it surfaces any panic the forwarder might have hit. Forgetting this `await` is the classic "last few tokens go missing" bug.

5. **Dispatch on `stop_reason`.** At this point we have a complete `AssistantTurn` and the UI has seen every `TextDelta`. If the model is done (`Stop`), we emit `AgentEvent::Done` and return. If it wants tools (`ToolUse`), we emit a `ToolCall` event per invocation (the UI uses these to show "[bash: ls]" spinners), run each tool with the same graceful-error pattern as `SimpleAgent`, append results to `messages`, and let the `loop` spin — which will spawn a fresh forwarder and `stream_chat` for the next turn.

### Why not just `rx.recv()` in the main loop?

A single-task approach — "call `stream_chat`, then drain `rx`" — deadlocks. `stream_chat` does not return until the stream is fully consumed; with an unbounded channel full of events and nobody reading, the provider keeps writing forever (technically fine, but nothing gets rendered until the end). A *bounded* channel with that approach would block the provider on `tx.send().await`, which would block `stream_chat`, which would never return. Either way the UI sees no tokens until the turn is over — defeating the point of streaming.

The forwarder pattern decouples the two halves: the provider's writer side and the UI's reader side both make progress independently.

### Your task

Fill in the `StreamingAgent::chat()` stub in `src/streaming.rs`. Use the four-step recipe: channel, forwarder, await `stream_chat`, await forwarder. Then the `match` on `stop_reason` is the same shape as `SimpleAgent::chat`.

---

## Run the tests

```bash
cargo test -p mini-claw-code-starter test_openrouter_
cargo test -p mini-claw-code-starter test_streaming_streaming_agent_
cargo test -p mini-claw-code-starter test_streaming_stream_chat_
```

### What these tests verify

**`test_openrouter_`** (OpenRouterProvider):

- **`test_openrouter_convert_messages`** — internal `Message` variants are converted to the correct OpenAI API format
- **`test_openrouter_convert_tools`** — `ToolDefinition` values are wrapped in the OpenAI function-calling envelope

**`test_streaming_streaming_agent_`** (StreamingAgent end-to-end against `MockStreamProvider`):

- **`test_streaming_streaming_agent_text_response`** — single-turn text response; UI channel sees at least one `TextDelta` and a `Done`
- **`test_streaming_streaming_agent_tool_loop`** — the agent runs a tool round and produces a final answer; UI channel sees a `ToolCall` event and a `Done`
- **`test_streaming_streaming_agent_chat_history`** — `chat()` appends the final assistant turn to the caller-provided `messages` vec

**`test_streaming_stream_chat_`** (OpenRouter streaming against a local TCP mock):

- **`test_streaming_stream_chat_events_order`** — a scripted SSE body is parsed into events in the correct order and the assembled `AssistantTurn` matches

---

## Key takeaway

`StreamingAgent` is where everything from 5a pays off. The provider produces `StreamEvent`s, the forwarder task translates them into UI-level `AgentEvent`s as they arrive, and the main loop waits on the assembled `AssistantTurn` to decide what to do next. Tokens hit the terminal in real time; the agent loop still sees a clean, complete message — no special-casing for streaming vs non-streaming.

The pattern — "split a complex stream into two concurrent sides, bridged by a task" — is the same one Claude Code uses in its renderer. Once you've written it once, it shows up everywhere you need to mix streaming I/O with step-wise decision-making.

In [Chapter 6](./ch06-tool-interface.md) we turn from providers to tools — the other half of the agent's interface with the outside world.

## Check yourself

{{#quiz ../quizzes/ch05b.toml}}

---

[← Chapter 5a: Provider & Streaming Foundations](./ch05a-provider-foundations.md) · [Contents](./ch00-overview.md) · [Chapter 6: Tool Interface →](./ch06-tool-interface.md)

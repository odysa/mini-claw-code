# Chapter 5b: OpenRouter & StreamingAgent

> **File(s) to edit:** `src/providers/openrouter.rs`, `src/streaming.rs` (the `StreamingAgent` block at the bottom)
> **Tests to run:** `cargo test -p mini-claw-code-starter test_openrouter_`, `cargo test -p mini-claw-code-starter test_streaming_streaming_agent_`, `cargo test -p mini-claw-code-starter test_streaming_stream_chat_`
> **Estimated time:** 35 min

## Goal

- Implement `OpenRouterProvider` so the agent can talk to a real OpenAI-compatible API — both non-streaming and streaming.
- Implement `StreamingAgent::chat` — the agent loop that forwards streaming text deltas to a UI channel while running tools.

[Chapter 5a](./ch05a-provider-foundations.md) built the abstractions (`Provider`, `StreamProvider`, `StreamEvent`), the mocks (`MockProvider`, `MockStreamProvider`), and the parse/accumulate machinery (`parse_sse_line`, `StreamAccumulator`). This chapter plugs those pieces into a real HTTP provider and wires a streaming channel through the agent loop.

If anything below assumes `parse_sse_line` or `StreamAccumulator` exists — it does, because you implemented it in 5a.

### Sidebar: tokio concurrency for Go devs

If Go is your native async language, here is the translation table you need
before reading the streaming code. Everything in this chapter rests on these
five primitives; skip this box if you already think in `tokio`.

| Go                                      | Tokio                                        | Notes                                                                                                                     |
|-----------------------------------------|-----------------------------------------------|---------------------------------------------------------------------------------------------------------------------------|
| `go func() { ... }()`                   | `tokio::spawn(async { ... })`                 | Both fire-and-forget. `tokio::spawn` returns a `JoinHandle` you can `await` later if you care about the result.           |
| `ch := make(chan T, n)`                 | `let (tx, rx) = tokio::sync::mpsc::channel::<T>(n)` | Bounded channel. For `unbounded_channel()` use `mpsc::unbounded_channel()` -- analogous to a channel with infinite buffer. |
| `ch <- v`                               | `tx.send(v).await`                            | Async send in Tokio (awaits when buffer full). The unbounded variant uses `tx.send(v)` with no `.await`.                  |
| `v, ok := <-ch`                         | `let Some(v) = rx.recv().await { ... }`       | `recv` returns `None` when *all* senders are dropped (equivalent to `close(ch)` + drain).                                 |
| `close(ch)`                             | drop every `tx` clone                         | Tokio has no explicit `close`. When the last sender is dropped, receivers see `None` and loops exit.                      |
| `wg.Add(1); wg.Wait()`                  | `handle.await` (or `tokio::join!`, `try_join!`) | A `JoinHandle` is like a single-goroutine WaitGroup. Multiple handles: `tokio::join!(h1, h2)` runs them concurrently.    |
| `select { case <-a: case <-b: }`        | `tokio::select! { _ = a => ..., _ = b => ... }` | Direct analogue. Loses on non-disjoint branches unless you use `biased;`.                                                 |

One non-obvious point specific to this chapter: we signal "the stream is
over" by *dropping the sender*. There is no explicit close call. The receiver
task observes `rx.recv().await == None` and exits its loop. If you forget to
drop the sender (for example by holding it inside an `Arc` that outlives the
producer), the receiver hangs forever -- this is one of the deadlock
patterns that [§"Why not just `rx.recv()` in the main loop?"](#why-not-just-rxrecv-in-the-main-loop) walks through.

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

Two `impl` methods on `OpenRouterProvider` translate between our internal
types and the API format. `convert_messages` handles the four `Message`
variants:

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

Four details worth pausing on:

- **`System` and `User` are symmetric.** Same shape, different role string.
  Everything else (`tool_calls`, `tool_call_id`) is `None`.
- **`Assistant` is the variant with the nuance.** The `text` field maps
  directly to `content`, but the tool calls have to be reserialised.
  `c.arguments` is a `serde_json::Value`; the OpenAI API wants it as a
  JSON *string*, so we call `.to_string()` to turn the `Value` back into
  text. Emitting an empty `tool_calls: []` array makes some providers
  reject the request as malformed, so we send `None` instead.
- **`ToolResult` becomes `role: "tool"`.** This is the variant that ties
  a result back to its originating call via `tool_call_id`. Without that
  id the provider cannot associate the result with the call, and the
  next response is usually an error.
- **No default branch.** Every `Message` variant is handled explicitly. If
  you add a new variant in Chapter 4, the match will fail to compile here
  until you decide how it should serialise — which is the behaviour we
  want.

`convert_tools` is simpler: wrap each `ToolDefinition` in the OpenAI
function-calling envelope.

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

The envelope is a fixed shape: `{ "type": "function", "function": { name,
description, parameters } }`. Every OpenAI-compatible provider expects
exactly this, and our `ToolDefinition` was designed in Ch4 specifically
so this mapping is a one-liner.

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

The non-streaming path is the simpler one: one POST, one JSON response, one
`AssistantTurn` returned. Here it is end to end:

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

Three decisions to notice:

- **`error_for_status()` turns HTTP 4xx/5xx into an `Err`.** Otherwise a
  403 from OpenRouter would deserialize whatever body came back as if it
  were a `ChatResponse` and fail confusingly later.
- **Tool-call arguments arrive as a JSON *string*, not a `Value`.** The
  OpenAI spec puts `"arguments": "{\"path\":\"foo.rs\"}"` in the wire
  format. We parse it back into a `Value` ourselves; on a parse failure
  we fall back to `Value::Null` so a malformed `arguments` field does
  not abort the whole turn.
- **`stop_reason` is a straight mapping of `finish_reason`.** Only
  `"tool_calls"` becomes `ToolUse`; everything else (`"stop"`,
  `"length"`, null, missing) becomes `Stop`. This matches the "the model
  decides" story from [Chapter 3's aside](./ch03-agentic-loop.md#aside-who-decides-stop-vs-tooluse) -- we are just translating the model's own stop signal.

### Streaming `StreamProvider` impl

The streaming path is the same request shape with `stream: true`, but
instead of a single JSON body we read a chunked HTTP response and parse
it as Server-Sent Events. Here is the complete impl:

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

Walk through it:

1. **Same request, but `stream: true`.** The API returns a chunked HTTP
   response instead of a single JSON body. The request construction and
   auth are identical to the non-streaming path; this is exactly what we
   want from an abstraction called "streaming".
2. **Read raw byte chunks.** `resp.chunk()` returns `Option<Bytes>` — the
   HTTP body arrives in arbitrary-sized pieces that do not align with SSE
   event boundaries. A single `chunk` could be a partial line, several
   lines, or multiple events crammed together.
3. **Buffer and split on newlines.** TCP chunks can split an SSE line in
   the middle. The `buffer` accumulates raw text, and the inner `while`
   loop extracts complete lines. This is classic line-oriented protocol
   parsing — you accumulate bytes and consume lines as they become
   available. Notice the inner loop keeps going until no more complete
   lines remain in the buffer, then we wait for the next chunk.
4. **Parse each line.** `parse_sse_line` (from 5a) converts a `data:` line
   into `StreamEvent`s. Blank lines (SSE event separators) and non-data
   lines (comments, keep-alives) return `None` and are skipped.
5. **Feed both the accumulator and the channel.** For every event, the
   accumulator updates its internal state (building the eventual
   `AssistantTurn`) and the channel delivers the same event to the UI in
   real-time. The `let _ = tx.send(event)` deliberately discards a send
   error: if the receiver has been dropped (e.g. the forwarder task has
   exited because the main loop cancelled), we still want to finish
   consuming the stream so the underlying HTTP connection can be cleanly
   released.
6. **Return the assembled message.** Once the stream ends (`resp.chunk()`
   returns `None`), the accumulator has collected everything, and
   `finish()` produces the final `AssistantTurn`. At this point `tx` is
   dropped (the function is returning), which closes the channel and
   signals the forwarder task to exit — exactly the termination flow the
   `StreamingAgent` section below depends on.

This dual-path design (accumulator + channel) is how Claude Code handles
streaming too. The UI renders tokens as they arrive, but the agent loop
sees a clean, complete response — no bespoke partial-state handling.

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

### The working pattern, end to end

Here is the same flow drawn once, after the deadlock is fixed. Four Rust
tasks, three edges that matter: the provider writes `tx`, the forwarder
pulls `rx` and re-emits onto `events`, and the main loop awaits on
`stream_chat`'s return value for control flow. Termination is purely
drop-based: when `stream_chat` returns, it drops `tx`; `rx.recv()` then
yields `None`; the forwarder loop exits; `handle.await` unblocks.

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

Three invariants keep this alive:

1. **The provider owns the sender.** Only `stream_chat` holds a `tx` — the
   main loop hands it over and does not keep a clone. When `stream_chat`
   returns, the last `tx` is dropped, which closes the channel.
2. **The forwarder owns the receiver.** It runs in its own spawned task so
   the receiver can make progress *while* `stream_chat` is still writing.
   No one else calls `rx.recv()`.
3. **The main loop awaits both.** First `stream_chat`, then the forwarder's
   `JoinHandle`. Awaiting the handle is what prevents the main loop from
   leaking a half-finished forwarder into the next iteration of the agent
   loop.

If any one of these three breaks — a stray `tx` clone held by the main
loop, the forwarder running inline on the main task, or the main loop
skipping the handle await — you get a subtle variant of the deadlock
above. This is why the pattern is worth learning once and reaching for
any time you need streaming I/O bridged into a step-wise decision loop.

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

# Chapter 14: Token Tracking

Every call to an LLM costs money. A single agent run might loop ten or twenty
times, reading files, running commands, and editing code. Without tracking how
many tokens you are spending, costs can silently spiral -- especially during
development when you are iterating fast. Claude Code shows a running token
count and cost estimate at the bottom of every session for exactly this reason.

In this chapter you will build `CostTracker`, a struct that accumulates token
usage across turns and computes an estimated cost. You will also see how the
OpenAI-compatible API reports usage in its response JSON, and how our
`OpenRouterProvider` already parses it into a `TokenUsage` struct on
`AssistantTurn`.

## Why track tokens?

There are two practical reasons:

1. **Cost control.** LLM APIs charge per token. If your agent enters a loop
   that keeps reading large files, the bill adds up fast. A cost tracker lets
   you display a running total, set budgets, or abort early.

2. **Context window awareness.** Every model has a maximum context length. As
   the conversation grows, input tokens increase with each turn (because you
   resend the full history). Tracking input tokens gives you a signal for when
   you are approaching the limit and might need to summarize or truncate.

## How APIs report usage

OpenAI-compatible APIs (OpenRouter, OpenAI, Anthropic's compatibility layer)
include a `usage` object in every chat completion response:

```json
{
  "id": "chatcmpl-abc123",
  "choices": [{ "message": { "content": "Hello!" }, "finish_reason": "stop" }],
  "usage": {
    "prompt_tokens": 42,
    "completion_tokens": 15
  }
}
```

- **`prompt_tokens`** -- how many tokens the API consumed reading your input
  (system prompt + conversation history + tool definitions).
- **`completion_tokens`** -- how many tokens the model generated in its
  response (text + tool calls).

Not every provider guarantees this field, so it is optional. But when it is
present, we want to capture it.

## Goal

Implement `CostTracker` so that:

1. You create it with per-million-token pricing for input and output.
2. You can `record()` a `TokenUsage` from each turn.
3. It accumulates totals across turns and computes estimated cost.
4. It can produce a human-readable summary string.
5. It can be reset to zero.

## The `TokenUsage` struct

Open `mini-claw-code-starter/src/types.rs`. You will see a new struct alongside
the types you already know:

```rust
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}
```

This is a simple data carrier -- just two numbers. The `Default` derive gives
us `TokenUsage { input_tokens: 0, output_tokens: 0 }` for free, which is
useful when the API omits individual fields.

The struct lives on `AssistantTurn` as an optional field:

```rust
pub struct AssistantTurn {
    pub text: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub stop_reason: StopReason,
    /// Token usage for this turn, if reported by the provider.
    pub usage: Option<TokenUsage>,
}
```

The `usage` field is `Option<TokenUsage>` because not every provider reports
it. `MockProvider` returns `None` (it does not call a real API), while
`OpenRouterProvider` parses it from the JSON response.

## How `OpenRouterProvider` parses usage

In Chapter 6 you built the HTTP provider. Now look at how it handles the
`usage` field in `openrouter.rs`. The response is deserialized into these
types:

```rust
#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
    usage: Option<ApiUsage>,
}

#[derive(Deserialize)]
struct ApiUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
}
```

Both `usage` on `ChatResponse` and the individual fields on `ApiUsage` are
optional -- some providers omit them entirely, others include the object but
leave fields null. At the end of the `chat()` method, the conversion looks
like this:

```rust
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
```

The double-`Option` pattern -- `Option<ApiUsage>` containing `Option<u64>`
fields -- is a common defensive strategy when deserializing API responses.
`resp.usage.map(...)` handles the outer option (no `usage` key at all), and
`unwrap_or(0)` handles the inner option (key present but value null).

You do not need to modify the provider. The parsing is already done. Your job
is to build the `CostTracker` that consumes these `TokenUsage` values.

## Implementing `CostTracker`

Open `mini-claw-code-starter/src/usage.rs`. You will see the struct and method
signatures already laid out with `unimplemented!()` bodies.

### The design

`CostTracker` needs to be shared across the agent loop -- you might pass it
into `run()` or hold it alongside the agent. Because the agent takes `&self`
(shared reference), the tracker must support mutation through `&self`. This is
the same interior mutability pattern you used in `MockProvider`:

```rust
pub struct CostTracker {
    inner: Mutex<CostTrackerInner>,
    /// Price per million input tokens (USD).
    input_price: f64,
    /// Price per million output tokens (USD).
    output_price: f64,
}

struct CostTrackerInner {
    total_input: u64,
    total_output: u64,
    turn_count: u64,
}
```

The prices are immutable after construction (they describe the model, which
does not change mid-session), so they live outside the `Mutex`. Only the
running totals need interior mutability.

### Step 1: Implement `new()`

The constructor takes two prices: input and output, both in dollars per million
tokens. These are the rates you find on a model's pricing page -- for example,
Claude Sonnet charges $3 per million input tokens and $15 per million output
tokens.

```rust
pub fn new(input_price_per_million: f64, output_price_per_million: f64) -> Self {
    Self {
        inner: Mutex::new(CostTrackerInner {
            total_input: 0,
            total_output: 0,
            turn_count: 0,
        }),
        input_price: input_price_per_million,
        output_price: output_price_per_million,
    }
}
```

Store the prices on `self` and initialize all counters to zero inside a
`Mutex`.

### Step 2: Implement `record()`

This is the method the agent loop calls after each provider response. It takes
a `&TokenUsage` and adds its values to the running totals:

```rust
pub fn record(&self, usage: &TokenUsage) {
    let mut inner = self.inner.lock().unwrap();
    inner.total_input += usage.input_tokens;
    inner.total_output += usage.output_tokens;
    inner.turn_count += 1;
}
```

Lock the mutex, add the token counts, bump the turn counter. That is it. The
lock is held for three additions -- fast enough that contention is never a
problem.

### Step 3: Implement the getter methods

Three simple accessors, each locking the mutex and reading a field:

```rust
pub fn total_input_tokens(&self) -> u64 {
    self.inner.lock().unwrap().total_input
}

pub fn total_output_tokens(&self) -> u64 {
    self.inner.lock().unwrap().total_output
}

pub fn turn_count(&self) -> u64 {
    self.inner.lock().unwrap().turn_count
}
```

Each method acquires and releases the lock independently. This is fine --
if you needed a consistent snapshot of all three values at once, you would
lock once and read all three. But for display purposes, slight inconsistency
between separate calls is acceptable.

### Step 4: Implement `total_cost()`

The cost formula is straightforward:

```
cost = (input_tokens * input_price + output_tokens * output_price) / 1,000,000
```

We divide by one million because the prices are per million tokens:

```rust
pub fn total_cost(&self) -> f64 {
    let inner = self.inner.lock().unwrap();
    (inner.total_input as f64 * self.input_price
        + inner.total_output as f64 * self.output_price)
        / 1_000_000.0
}
```

Notice we lock once and read both `total_input` and `total_output` together.
This ensures the cost calculation uses a consistent pair of values.

### Step 5: Implement `summary()`

This produces a human-readable string for display -- the kind of thing you
would show at the bottom of a terminal UI:

```
tokens: 1234 in + 567 out | cost: $0.0122
```

The implementation duplicates the cost calculation (instead of calling
`self.total_cost()`) to avoid locking the mutex twice:

```rust
pub fn summary(&self) -> String {
    let inner = self.inner.lock().unwrap();
    let cost = (inner.total_input as f64 * self.input_price
        + inner.total_output as f64 * self.output_price)
        / 1_000_000.0;
    format!(
        "tokens: {} in + {} out | cost: ${:.4}",
        inner.total_input, inner.total_output, cost
    )
}
```

The `{:.4}` format specifier gives four decimal places -- enough precision
for small token counts where the cost might be fractions of a cent.

### Step 6: Implement `reset()`

Reset all counters to zero. Useful when starting a new conversation in the
same session:

```rust
pub fn reset(&self) {
    let mut inner = self.inner.lock().unwrap();
    inner.total_input = 0;
    inner.total_output = 0;
    inner.turn_count = 0;
}
```

## Running the tests

Run the Chapter 14 tests:

```bash
cargo test -p mini-claw-code-starter ch14
```

### What the tests verify

- **`test_ch14_empty_tracker`**: A freshly created tracker has zero tokens,
  zero turns, and zero cost.
- **`test_ch14_record_single_turn`**: Record one usage, verify the totals
  match exactly.
- **`test_ch14_accumulates_across_turns`**: Record three usages, verify the
  totals are the sum of all three.
- **`test_ch14_cost_calculation`**: Record exactly one million input and one
  million output tokens at $3/M and $15/M. Verify cost is $18.00.
- **`test_ch14_cost_small_numbers`**: Record 1000 input and 200 output tokens.
  Verify cost is $0.006 (three tenths of a cent).
- **`test_ch14_summary_format`**: Verify the summary string contains the
  expected token counts and a dollar sign.
- **`test_ch14_reset`**: Record usage, reset, verify everything is back to
  zero.
- **`test_ch14_zero_usage`**: Record a turn with zero tokens. Turn count
  increments but cost stays zero.
- **`test_ch14_token_usage_default`**: Verify `TokenUsage::default()` gives
  zeros -- a sanity check on the `Default` derive.

## Wiring it into the agent loop

The tests cover `CostTracker` in isolation, but in practice you would wire it
into your agent loop. After each call to `self.provider.chat()`, check if the
response includes usage data and record it:

```rust
let turn = self.provider.chat(&messages, &defs).await?;

if let Some(ref usage) = turn.usage {
    cost_tracker.record(usage);
}
```

Then, after the agent finishes (or periodically during long runs), display
the summary:

```rust
println!("{}", cost_tracker.summary());
// tokens: 4521 in + 892 out | cost: $0.0270
```

This is exactly what tools like Claude Code do -- show a running cost estimate
so you know what a session is costing in real time.

## Recap

You have built a `CostTracker` that:

- **Accumulates** input and output token counts across multiple agent turns.
- **Computes cost** from per-million-token pricing.
- **Produces a summary** string for display.
- **Uses `Mutex`** for interior mutability, the same pattern as `MockProvider`.
- **Handles the full chain**: API response -> `TokenUsage` on `AssistantTurn`
  -> `CostTracker::record()` -> running totals and cost estimate.

Token tracking is a small feature in terms of code, but it is essential for
any agent you plan to use in production. Without it, you are flying blind on
costs and context window usage.

## What's next

In [Chapter 15: Safety Rails](./ch15-safety.md) you will add guardrails to
your agent -- command filtering, path validation, and permission prompts -- so
it cannot accidentally `rm -rf /` or read files outside the project directory.

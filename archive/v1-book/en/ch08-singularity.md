# Chapter 8: The Singularity

Your agent can edit itself and it starts self-evolving. You don't need to write any code starting from now.

## Extensions

The extension chapters that follow walk through the reference implementation.
You don't need to write the code yourself -- read them to understand the
design, then let your agent implement them (or do it yourself for practice):

- [Chapter 9: A Better TUI](./ch09-tui.md) -- Markdown rendering, spinners, collapsed tool calls.
- [Chapter 10: Streaming](./ch10-streaming.md) -- Stream tokens as they arrive with `StreamingAgent`.
- [Chapter 11: User Input](./ch11-user-input.md) -- Let the LLM ask you clarifying questions.
- [Chapter 12: Plan Mode](./ch12-plan-mode.md) -- Read-only planning with approval gating.

Beyond the extension chapters, here are more ideas to explore:

- **Parallel tool calls** -- Execute concurrent tool calls with `tokio::join!`.
- **Token tracking** -- Truncate old messages when approaching the context limit.
- **More tools** -- Web search, database queries, HTTP requests. The `Tool` trait makes it easy.
- **MCP** -- Expose your tools as an MCP server or connect to external ones.

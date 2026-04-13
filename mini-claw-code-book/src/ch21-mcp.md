# Chapter 21: MCP -- Model Context Protocol

Your agent has tools -- read, write, bash, subagents -- but they are all
compiled into the binary. What happens when someone wants to give your agent
access to a database, a Kubernetes cluster, or a Slack workspace?

You could write a `Tool` implementation for each one. That doesn't scale.
Every integration means new code, a new release, tight coupling.

**MCP (Model Context Protocol)** solves this. It is an open standard created
by Anthropic that lets AI agents discover and use tools from external server
processes. Claude Code uses MCP. Cursor uses MCP. There are hundreds of
community MCP servers for everything from GitHub to PostgreSQL.

The idea: spawn a separate process that speaks JSON-RPC over stdio. Your
agent asks "what tools do you have?", gets back definitions, and calls them
like any other tool. The server handles the integration. Your agent just
speaks the protocol.

In this chapter you will:

1. Understand the MCP protocol: JSON-RPC 2.0 over stdio, the handshake
   sequence, and the tool lifecycle.
2. Define the protocol types: `JsonRpcRequest`, `JsonRpcResponse`,
   `McpToolDef`.
3. Build `McpClient`: spawn a child process, perform the handshake, list
   tools, and call them.
4. Implement `McpTool`: a wrapper that bridges MCP tools into the `Tool`
   trait so the agent loop handles them transparently.
5. Wire it into the config system with `McpServerConfig`.

This is the capstone chapter. When you finish, your agent will be able to
connect to *any* MCP server and use its tools -- the same way the real Claude
Code does.

## The protocol

MCP uses **JSON-RPC 2.0** over **stdio**. The client (your agent) spawns the
server as a child process, writes JSON to its stdin, and reads JSON from its
stdout. Each message is a single line of JSON terminated by a newline.

The lifecycle has three phases:

```text
Client                          Server
  |                               |
  |--- initialize --------------->|   Phase 1: Handshake
  |<-- initialize result ---------|
  |--- notifications/initialized ->|
  |                               |
  |--- tools/list --------------->|   Phase 2: Discovery
  |<-- tools list ----------------|
  |                               |
  |--- tools/call --------------->|   Phase 3: Execution
  |<-- tool result ---------------|
  |          ...                  |
```

**Phase 1: Handshake.** The client sends `initialize` with its protocol
version and capabilities. The server responds. The client sends
`notifications/initialized` to signal completion.

**Phase 2: Discovery.** `tools/list` returns tool definitions -- name,
description, and JSON Schema for input parameters.

**Phase 3: Execution.** `tools/call` sends a tool name and arguments. The
server executes and returns the result.

Every request is `{"jsonrpc": "2.0", "id": 1, "method": "...", "params": {...}}`.
Responses carry either `"result"` or `"error"`. That's the entire protocol
surface we need. MCP has more features (resources, prompts, sampling), but
tools are the core.

## Protocol types

Create `mini-claw-code/src/mcp/types.rs`. These types map directly to the
JSON-RPC wire format.

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize)]
pub(crate) struct JsonRpcRequest {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl JsonRpcRequest {
    pub fn new(id: u64, method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            method: method.into(),
            params,
        }
    }
}
```

`jsonrpc` is always `"2.0"` -- no allocation. `params` uses
`skip_serializing_if` because JSON-RPC omits the field when absent. `id`
is a monotonically increasing `u64` for matching responses to requests.

The response side:

```rust
#[derive(Deserialize)]
pub(crate) struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: u64,
    pub result: Option<Value>,
    pub error: Option<JsonRpcError>,
}

#[derive(Deserialize, Debug)]
pub(crate) struct JsonRpcError {
    pub code: i64,
    pub message: String,
}
```

And the MCP-specific types:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct McpToolDef {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "inputSchema", default)]
    pub input_schema: Option<Value>,
}

#[derive(Deserialize)]
pub(crate) struct InitializeResult {
    pub capabilities: Option<Value>,
}

#[derive(Deserialize)]
pub(crate) struct ToolsListResult {
    pub tools: Vec<McpToolDef>,
}

#[derive(Deserialize)]
pub(crate) struct ToolCallResult {
    pub content: Vec<ToolCallContent>,
}

#[derive(Deserialize)]
pub(crate) struct ToolCallContent {
    #[serde(rename = "type")]
    pub type_: Option<String>,
    pub text: Option<String>,
}
```

`McpToolDef` is what the server returns from `tools/list`. The `inputSchema`
field uses camelCase on the wire (MCP convention), so we rename it with
serde. Both `description` and `input_schema` are optional -- a minimal tool
only needs a name.

`ToolCallResult` returns an array of content blocks (similar to Claude's
API). Each block has a `type` (usually `"text"`) and a `text` field. We will
extract and join the text blocks to produce a single string.

## Building `McpClient`

Create `mini-claw-code/src/mcp/client.rs`. The `McpClient` manages a child
process and speaks JSON-RPC to it.

```rust
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Context;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

pub struct McpClient {
    stdin: Mutex<tokio::process::ChildStdin>,
    stdout: Mutex<BufReader<tokio::process::ChildStdout>>,
    _child: Mutex<Child>,
    next_id: AtomicU64,
    server_name: String,
}
```

**Why `Mutex`?** Stdin and stdout are not `Clone`. We need shared access
(`McpTool` holds an `Arc<McpClient>`), so we wrap them in
`tokio::sync::Mutex`. The `_child` field holds ownership of the process so
it doesn't get dropped. `AtomicU64` gives us lock-free request IDs.

### Connecting and handshaking

The `connect` constructor spawns the process and performs the handshake:

```rust
impl McpClient {
    pub async fn connect(
        server_name: impl Into<String>,
        command: &str,
        args: &[String],
    ) -> anyhow::Result<Self> {
        let server_name = server_name.into();
        let mut child = Command::new(command)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .with_context(|| format!("failed to spawn MCP server: {command}"))?;

        let stdin = child.stdin.take().context("failed to get stdin")?;
        let stdout = child.stdout.take().context("failed to get stdout")?;
        let client = Self {
            stdin: Mutex::new(stdin),
            stdout: Mutex::new(BufReader::new(stdout)),
            _child: Mutex::new(child),
            next_id: AtomicU64::new(1),
            server_name,
        };
        client.initialize().await?;
        Ok(client)
    }
}
```

We use `tokio::process::Command` for async I/O. Stderr goes to null -- MCP
servers communicate exclusively over stdout. The `initialize` method sends
the two-part handshake:

```rust
async fn initialize(&self) -> anyhow::Result<()> {
    let params = serde_json::json!({
        "protocolVersion": "2024-11-05",
        "capabilities": {},
        "clientInfo": { "name": "mini-claw-code", "version": "0.1.0" }
    });

    let result = self.request("initialize", Some(params)).await?;
    let _: InitializeResult = serde_json::from_value(result)
        .context("failed to parse initialize response")?;

    // Send initialized notification
    let id = self.next_id.fetch_add(1, Ordering::Relaxed);
    let notification = JsonRpcRequest::new(id, "notifications/initialized", None);
    let mut payload = serde_json::to_string(&notification)?;
    payload.push('\n');

    let mut stdin = self.stdin.lock().await;
    stdin.write_all(payload.as_bytes()).await?;
    stdin.flush().await?;

    Ok(())
}
```

First `initialize` -- a request-response pair. Then
`notifications/initialized` -- technically a notification, but we format it
as a request for simplicity. The core method driving all communication:

```rust
async fn request(&self, method: &str, params: Option<Value>) -> anyhow::Result<Value> {
    let id = self.next_id.fetch_add(1, Ordering::Relaxed);
    let request = JsonRpcRequest::new(id, method, params);
    let mut payload = serde_json::to_string(&request)?;
    payload.push('\n');

    {
        let mut stdin = self.stdin.lock().await;
        stdin.write_all(payload.as_bytes()).await
            .context("failed to write to MCP server")?;
        stdin.flush().await
            .context("failed to flush MCP server stdin")?;
    }

    let mut line = String::new();
    {
        let mut stdout = self.stdout.lock().await;
        loop {
            line.clear();
            let bytes_read = stdout.read_line(&mut line).await
                .context("failed to read from MCP server")?;
            if bytes_read == 0 {
                anyhow::bail!("MCP server closed stdout unexpectedly");
            }
            let trimmed = line.trim();
            if trimmed.is_empty() { continue; }
            if let Ok(resp) = serde_json::from_str::<JsonRpcResponse>(trimmed) {
                if let Some(error) = resp.error {
                    anyhow::bail!("MCP server error ({}): {}", error.code, error.message);
                }
                return Ok(resp.result.unwrap_or(Value::Null));
            }
            // Not a valid response -- skip (could be a notification)
        }
    }
}
```

The read loop skips notifications and blank lines. The scope blocks drop
the stdin lock before acquiring stdout, preventing deadlocks. With
`request()` in place, the public methods are short:

```rust
pub async fn list_tools(&self) -> anyhow::Result<Vec<McpToolDef>> {
    let result = self.request("tools/list", None).await?;
    let list: ToolsListResult =
        serde_json::from_value(result).context("failed to parse tools/list")?;
    Ok(list.tools)
}

pub async fn call_tool(&self, name: &str, arguments: Value) -> anyhow::Result<String> {
    let params = serde_json::json!({ "name": name, "arguments": arguments });
    let result = self.request("tools/call", Some(params)).await?;
    let call_result: ToolCallResult =
        serde_json::from_value(result).context("failed to parse tools/call")?;

    let text: Vec<String> = call_result.content.into_iter()
        .filter_map(|c| c.text)
        .collect();
    Ok(text.join("\n"))
}
```

`call_tool` extracts just the text content blocks and joins them with
newlines -- matching how our agent represents tool results as plain strings.

## Converting MCP tools to `ToolDefinition`

There's a gap between MCP's `McpToolDef` (owned `String` fields) and our
`ToolDefinition` (`&'static str` fields). The `convert_tool_defs` method
bridges it:

```rust
pub fn convert_tool_defs(tools: &[McpToolDef], prefix: &str) -> Vec<ToolDefinition> {
    tools.iter().map(|t| {
        let name = format!("mcp__{prefix}__{}", t.name);
        let desc = t.description.clone()
            .unwrap_or_else(|| format!("MCP tool: {}", t.name));
        let params = t.input_schema.clone()
            .unwrap_or_else(|| serde_json::json!({"type": "object", "properties": {}}));

        // Leak strings for 'static lifetime (loaded once at startup)
        let name: &'static str = Box::leak(name.into_boxed_str());
        let desc: &'static str = Box::leak(desc.into_boxed_str());

        ToolDefinition { name, description: desc, parameters: params }
    }).collect()
}
```

Two important design decisions here:

**The naming convention: `mcp__servername__toolname`.** Double underscores
separate the MCP prefix, server name, and tool name. A filesystem server
named `fs` with a tool called `read_file` becomes `mcp__fs__read_file`.
This prevents collisions between MCP servers and between MCP tools and
built-in tools. Claude Code uses the exact same convention.

**String leaking with `Box::leak`.** Our `ToolDefinition` uses
`&'static str` -- a design choice from Chapter 1 that avoids lifetime
parameters everywhere. MCP tool names are dynamically constructed, so they
can't be `&'static str` naturally. `Box::leak` converts an owned `String`
by intentionally leaking the heap allocation.

Is this okay? Yes. MCP tools are loaded once at startup -- typically dozens
of strings. They live for the entire program duration anyway. This is a
well-known Rust pattern for configuration data loaded once and never freed.

## The `McpTool` wrapper

The agent works with the `Tool` trait. We need a struct that implements
`Tool` and forwards calls to the MCP server. This goes in
`mini-claw-code/src/mcp/mod.rs`:

```rust
pub(crate) mod client;
pub(crate) mod types;

pub use client::McpClient;
pub use types::McpToolDef;

use async_trait::async_trait;
use serde_json::Value;
use crate::types::{Tool, ToolDefinition};

pub struct McpTool {
    client: std::sync::Arc<McpClient>,
    definition: ToolDefinition,
    remote_name: String,
}

impl McpTool {
    pub fn new(
        client: std::sync::Arc<McpClient>,
        remote_name: String,
        definition: ToolDefinition,
    ) -> Self {
        Self { client, definition, remote_name }
    }
}

#[async_trait]
impl Tool for McpTool {
    fn definition(&self) -> &ToolDefinition {
        &self.definition
    }

    async fn call(&self, args: Value) -> anyhow::Result<String> {
        self.client.call_tool(&self.remote_name, args).await
    }
}
```

`Arc<McpClient>` gives shared ownership (multiple tools from one server
share a client). `definition` is the `mcp__server__tool` name the LLM sees.
`remote_name` is the original name the server expects. The `Tool`
implementation is glue: `definition()` returns the local definition,
`call()` forwards to `client.call_tool()` with the remote name.

## Configuration

In Chapter 16 you built the config system. MCP servers slot right in with
`McpServerConfig` in `config.rs`:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}
```

In the config file:

```toml
[[mcp_servers]]
name = "filesystem"
command = "npx"
args = ["-y", "@anthropic/mcp-filesystem-server", "/home/user/projects"]

[[mcp_servers]]
name = "github"
command = "npx"
args = ["-y", "@anthropic/mcp-github-server"]
env = { GITHUB_TOKEN = "ghp_..." }
```

At startup, iterate over configured servers, connect, discover, and
register:

```rust
use std::sync::Arc;

for server_config in &config.mcp_servers {
    let client = McpClient::connect(
        &server_config.name,
        &server_config.command,
        &server_config.args,
    ).await?;

    let client = Arc::new(client);
    let mcp_tools = client.list_tools().await?;
    let defs = McpClient::convert_tool_defs(&mcp_tools, client.server_name());

    for (mcp_def, tool_def) in mcp_tools.into_iter().zip(defs) {
        tools.push(McpTool::new(client.clone(), mcp_def.name, tool_def));
    }
}
```

The agent loop doesn't know or care that some tools are local and others are
remote MCP servers. They all implement `Tool`. The abstraction works.

## Module structure

Wire up the module and re-export from `lib.rs`:

```rust
pub mod mcp;
// ...
pub use mcp::{McpClient, McpTool};
```

The submodules `client` and `types` are `pub(crate)` -- internal
implementation details. Only `McpClient`, `McpTool`, and `McpToolDef` are
part of the public API.

## Running the tests

```bash
cargo test -p mini-claw-code ch21
```

The tests verify protocol types and conversion logic without a real MCP
server. They cover: `convert_tool_defs` with empty, single, multiple, and
missing-description inputs; `McpToolDef` deserialization (including the
`inputSchema` rename and minimal name-only definitions); `JsonRpcRequest`
serialization (with and without params, verifying `skip_serializing_if`);
and `ToolCallResult` content extraction.

Integration tests for `McpClient::connect` require a real MCP server process
and are better suited for CI.

## What you've built

Take a step back and look at what you have.

Your agent started as type definitions in Chapter 1. Now it has streaming,
subagents, safety rails, token tracking, context management, permissions --
and with MCP, it is **extensible without recompilation**. Anyone can write an
MCP server in any language and your agent will discover and use its tools at
runtime. The same protocol Claude Code and Cursor speak.

Here's the full lifecycle when a user configures an MCP server:

```text
 1. Config loads McpServerConfig from config.toml
 2. McpClient::connect() spawns the server process
 3. Client sends initialize, receives capabilities
 4. Client sends notifications/initialized
 5. Client sends tools/list, receives tool definitions
 6. convert_tool_defs() creates ToolDefinitions with mcp__ prefix
 7. McpTool wrappers are added to the ToolSet
 8. User asks a question
 9. Agent loop sends prompt + all tool definitions to the LLM
10. LLM decides to call mcp__github__search_repos
11. Agent finds the McpTool, calls it
12. McpTool forwards to McpClient::call_tool()
13. Client sends tools/call JSON-RPC to the server process
14. Server executes, returns results
15. Client parses the response, returns text
16. Agent loop adds result to the conversation
17. LLM uses the result to answer the user
```

Seventeen steps, three process boundaries, one seamless experience.

## Recap

- **MCP** is the standard protocol for AI tool servers. JSON-RPC 2.0 over
  stdio, line-delimited.
- **The handshake**: `initialize` -> `notifications/initialized` ->
  `tools/list`. Three messages and the client knows what the server can do.
- **`McpClient`** spawns the server, manages stdio via `tokio::sync::Mutex`,
  uses `AtomicU64` for request IDs. The read loop skips notifications.
- **`convert_tool_defs`** bridges MCP's owned strings to `&'static str`
  via `Box::leak`. The `mcp__server__tool` convention prevents collisions.
- **`McpTool`** wraps `Arc<McpClient>` and implements `Tool`. The agent
  loop treats MCP tools identically to built-in tools.
- **`McpServerConfig`** means zero code changes to add new servers.
- **The abstraction holds.** A tool is a tool -- whether `call()` reads a
  local file or sends JSON-RPC to a remote process.

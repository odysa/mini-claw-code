# Chapter 19: Permissions

If you've used Claude Code, you've seen this prompt:

```text
  Claude wants to use bash:
    command: git status

  Allow? (y/n/always)
```

The agent doesn't just run every tool call blindly. Before executing, it checks
a **permission system** to decide: should this tool call proceed automatically,
be blocked outright, or require user approval?

This is the permission system. Three possible decisions:

- **Allow** -- execute immediately, no questions asked.
- **Deny** -- block the call, return an error to the LLM.
- **Ask** -- pause and prompt the user for approval.

In this chapter you'll build:

1. A **`Permission` enum** with the three decisions.
2. A **`PermissionRule`** that matches tool names using glob patterns.
3. A **`PermissionEngine`** that evaluates rules in order, supports a default
   fallback, and remembers session-level overrides.

## Why permissions?

Chapter 18 introduced safety rails -- `SafeToolWrapper` blocks dangerous
arguments (path traversal, `rm -rf /`) based on static checks. But safety
checks are binary: pass or fail. They can't express "this tool is fine for
reading, but I want to approve writes."

Permissions add a human-in-the-loop layer. A typical configuration might look
like:

| Tool      | Permission |
|-----------|------------|
| `read`    | Allow      |
| `bash`    | Ask        |
| `write`   | Ask        |
| `edit`    | Ask        |
| `mcp__*`  | Deny       |
| (default) | Ask        |

The `read` tool runs freely. `bash`, `write`, and `edit` require approval.
Any MCP tool is blocked entirely. Anything else falls through to the default:
ask the user.

## The `Permission` enum

Three variants, nothing more:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Permission {
    /// Tool call is allowed without asking.
    Allow,
    /// Tool call is blocked without asking.
    Deny,
    /// User must be prompted for approval.
    Ask,
}
```

`PartialEq` lets tests assert on decisions. `Clone` is needed because
`evaluate()` returns owned values (you'll see why shortly).

## `PermissionRule`

A rule pairs a **glob pattern** with a **permission**:

```rust
#[derive(Debug, Clone)]
pub struct PermissionRule {
    /// Glob pattern matching tool names (e.g. "bash", "write", "*").
    pub tool_pattern: String,
    /// The permission to assign when the pattern matches.
    pub permission: Permission,
}
```

The `matches()` method checks whether a tool name matches the rule's pattern:

```rust
impl PermissionRule {
    pub fn new(tool_pattern: impl Into<String>, permission: Permission) -> Self {
        Self {
            tool_pattern: tool_pattern.into(),
            permission,
        }
    }

    /// Check if this rule matches a tool name.
    pub fn matches(&self, tool_name: &str) -> bool {
        if let Ok(pattern) = glob::Pattern::new(&self.tool_pattern) {
            pattern.matches(tool_name)
        } else {
            self.tool_pattern == tool_name
        }
    }
}
```

Glob patterns give you flexible matching:

- `"bash"` -- matches exactly `bash`.
- `"*"` -- matches everything (a catch-all rule).
- `"mcp__*"` -- matches any MCP tool (`mcp__fs__read`, `mcp__git__status`,
  etc.).

If the pattern string is invalid as a glob, `matches()` falls back to exact
string comparison. This means plain tool names always work even if the `glob`
crate can't parse them.

## `PermissionEngine`

The engine holds an ordered list of rules, a default permission, and a set of
session-level overrides:

```rust
pub struct PermissionEngine {
    rules: Vec<PermissionRule>,
    default_permission: Permission,
    /// Session-level overrides (tool calls the user has already approved).
    session_allows: std::collections::HashSet<String>,
}
```

### Construction

Three constructors cover the common cases:

```rust
impl PermissionEngine {
    pub fn new(rules: Vec<PermissionRule>, default_permission: Permission) -> Self {
        Self {
            rules,
            default_permission,
            session_allows: std::collections::HashSet::new(),
        }
    }

    /// Create an engine that asks for everything by default.
    pub fn ask_by_default(rules: Vec<PermissionRule>) -> Self {
        Self::new(rules, Permission::Ask)
    }

    /// Create an engine that allows everything (no permission checks).
    pub fn allow_all() -> Self {
        Self::new(vec![], Permission::Allow)
    }
}
```

`allow_all()` is useful during development or in trusted environments.
`ask_by_default()` is the safe default -- if a tool doesn't match any rule,
the user gets prompted.

### The `evaluate()` method -- your exercise

This is the core of the engine. Given a tool name and its arguments, return the
permission decision.

The evaluation order is:

1. **Session overrides first.** If the user already approved this tool during
   the current session, return `Allow`.
2. **Rules in order.** Walk the rules list. The first rule whose pattern
   matches the tool name wins -- return its permission.
3. **Default.** If no rule matches, return the default permission.

Here is the signature:

```rust
/// Evaluate permission for a tool call.
///
/// Returns the permission decision. If the result is `Ask`, the caller
/// should prompt the user and then call `record_session_allow` if approved.
pub fn evaluate(&self, tool_name: &str, _args: &Value) -> Permission {
    todo!()
}
```

The `_args` parameter is reserved for future use -- argument-level rules (e.g.
"allow `bash` only for `cargo test`") are a natural extension, but we won't
implement them here.

**Implement `evaluate()`** using the three-step logic above. The rest of this
section shows the solution.

### Solution

```rust
pub fn evaluate(&self, tool_name: &str, _args: &Value) -> Permission {
    // Check session-level overrides first
    if self.session_allows.contains(tool_name) {
        return Permission::Allow;
    }

    // Check rules in order
    for rule in &self.rules {
        if rule.matches(tool_name) {
            return rule.permission.clone();
        }
    }

    self.default_permission.clone()
}
```

Three things to note:

1. **Session overrides take priority over rules.** Even if a rule says `Ask`
   for `bash`, a session override makes it `Allow`. This is intentional -- when
   the user says "always allow" for a session, we honor that.
2. **First match wins.** If two rules match the same tool, the first one in the
   list is used. This is the same precedence model used by firewalls, `.gitignore`,
   and most rule-based systems.
3. **`clone()` on the return.** `Permission` is a simple enum, so cloning is
   cheap. We clone rather than returning a reference because the caller often
   needs to match on the owned value.

### First-match semantics

The "first match wins" rule is important. Consider:

```rust
let rules = vec![
    PermissionRule::new("bash", Permission::Allow),
    PermissionRule::new("bash", Permission::Deny),  // never reached
];
let engine = PermissionEngine::new(rules, Permission::Ask);

assert_eq!(engine.evaluate("bash", &json!({})), Permission::Allow);
```

The second rule is dead code. This lets you put specific rules before broad
ones:

```rust
let rules = vec![
    PermissionRule::new("read", Permission::Allow),   // specific
    PermissionRule::new("*", Permission::Ask),         // catch-all
];
```

`read` gets `Allow`. Everything else falls through to the wildcard and gets
`Ask`.

## Session-level overrides

When the user responds "always allow" (or just "y") to a permission prompt,
you don't want to ask again for the same tool in the same session. The engine
tracks this with a `HashSet<String>`:

```rust
/// Record that the user approved a tool for this session.
pub fn record_session_allow(&mut self, tool_name: &str) {
    self.session_allows.insert(tool_name.to_string());
}
```

The typical flow in an agent loop:

```rust
let permission = engine.evaluate("bash", &args);
match permission {
    Permission::Allow => { /* execute */ }
    Permission::Deny => { /* return error to LLM */ }
    Permission::Ask => {
        if user_approves() {
            engine.record_session_allow("bash");
            // execute
        } else {
            // return error to LLM
        }
    }
}
```

After `record_session_allow("bash")`, every subsequent `evaluate("bash", ...)`
returns `Allow` immediately -- the session override is checked before rules.

Note that session overrides are per-tool, not global:

```rust
let mut engine = PermissionEngine::ask_by_default(vec![]);
engine.record_session_allow("read");

assert_eq!(engine.evaluate("read", &json!({})), Permission::Allow);
assert_eq!(engine.evaluate("write", &json!({})), Permission::Ask); // still asks
```

Approving `read` doesn't approve `write`. Each tool must be approved
individually.

## Convenience methods

Two helpers reduce boilerplate at call sites:

```rust
/// Check if a tool is allowed (returns true for Allow, false for Deny/Ask).
pub fn is_allowed(&self, tool_name: &str, args: &Value) -> bool {
    matches!(self.evaluate(tool_name, args), Permission::Allow)
}

/// Check if a tool requires user approval.
pub fn needs_approval(&self, tool_name: &str, args: &Value) -> bool {
    matches!(self.evaluate(tool_name, args), Permission::Ask)
}
```

These are useful when you need a boolean check rather than a full `match`:

```rust
if engine.is_allowed("read", &args) {
    // fast path, no prompt needed
}
```

## Composing with SafeToolWrapper and InputHandler

Permissions, safety checks, and user input are three independent layers that
compose naturally. Here is how they fit together in an agent loop:

```text
Tool call arrives
  |
  v
PermissionEngine::evaluate()
  |-- Allow --> SafeToolWrapper::call()
  |               |-- safety check passes --> inner tool executes
  |               |-- safety check fails  --> error returned to LLM
  |
  |-- Deny  --> error returned to LLM
  |
  |-- Ask   --> InputHandler::ask("Allow bash?", &["yes", "no"])
                  |-- user says yes --> record_session_allow() + execute
                  |-- user says no  --> error returned to LLM
```

Permissions decide *whether* to run. Safety checks (Ch18) validate *how* the
tool is called. The `InputHandler` (Ch11) collects the user's answer when
permission is `Ask`.

In code, this might look like:

```rust
let permission = engine.evaluate(&call.name, &call.arguments);
match permission {
    Permission::Allow => {
        // SafeToolWrapper handles safety checks internally
        let result = tools.call(&call.name, call.arguments.clone()).await?;
        results.push((call.id.clone(), result));
    }
    Permission::Deny => {
        results.push((
            call.id.clone(),
            format!("error: tool '{}' is not permitted", call.name),
        ));
    }
    Permission::Ask => {
        let answer = input_handler
            .ask(
                &format!("Allow {} tool?", call.name),
                &["yes".into(), "no".into()],
            )
            .await?;
        if answer == "yes" {
            engine.record_session_allow(&call.name);
            let result = tools.call(&call.name, call.arguments.clone()).await?;
            results.push((call.id.clone(), result));
        } else {
            results.push((
                call.id.clone(),
                format!("error: user denied tool '{}'", call.name),
            ));
        }
    }
}
```

Each layer is optional. You can use permissions without safety checks, safety
checks without permissions, or all three together. This is the benefit of
composable design -- each piece does one job.

## Wiring it up

Add the module to `mini-claw-code/src/lib.rs`:

```rust
pub mod permissions;
// ...
pub use permissions::{Permission, PermissionEngine, PermissionRule};
```

## Running the tests

```bash
cargo test -p mini-claw-code ch19
```

The tests verify:

- **`allow_all`**: `PermissionEngine::allow_all()` returns `Allow` for any
  tool.
- **`ask_by_default`**: engine with no rules and `Ask` default returns `Ask`.
- **Rule matching**: explicit rules for `read`, `bash`, `write` each return the
  correct permission.
- **Glob pattern**: `"mcp__*"` matches `mcp__fs__read` but not `read`.
- **First rule wins**: duplicate rules for `bash` -- the first one wins.
- **Session allow**: after `record_session_allow("bash")`, `evaluate("bash")`
  returns `Allow`.
- **Session allow per tool**: approving `read` does not approve `write`.
- **`is_allowed`**: returns `true` only for `Allow`, `false` for `Deny` and
  `Ask`.
- **`needs_approval`**: returns `true` only for `Ask`.
- **Wildcard rule**: `"*"` matches any tool name.
- **Deny overrides default**: a `Deny` rule takes precedence over an `Allow`
  default.

## Recap

- **`Permission`** has three variants: `Allow`, `Deny`, `Ask`. Simple and
  exhaustive.
- **`PermissionRule`** pairs a glob pattern with a permission decision. Glob
  matching supports wildcards for tool families like `mcp__*`.
- **`PermissionEngine`** evaluates rules in order -- first match wins. When no
  rule matches, the default permission applies.
- **Session overrides** let the user approve a tool once and skip the prompt
  for the rest of the session. They take priority over rules.
- **Composable**: permissions layer on top of `SafeToolWrapper` (Ch18) and
  `InputHandler` (Ch11) without coupling to either.
- **Purely additive**: no changes to existing tools, agents, or safety checks.

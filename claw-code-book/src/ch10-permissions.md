# Chapter 10: Permission Engine

Your agent does whatever the LLM tells it to.

Think about that for a moment. In Chapters 1-9 you built a fully functional coding agent with six tools. The LLM can read files, write files, edit files, search code, and execute arbitrary shell commands. The QueryEngine dutifully dispatches every tool call the model requests. If the model says `bash("rm -rf /")`, the engine runs it. If it writes garbage over your source files, the engine writes. If it decides to `curl | sh` something from the internet, the engine curls. There is nothing between the LLM's request and the tool's execution.

This is fine for a tutorial. It is not fine for software you run on your actual codebase.

Chapter 10 changes that. We build the `PermissionEngine` -- the gatekeeper that evaluates every tool call before it executes. It sits between the QueryEngine and the tools, and for each call it returns one of three answers: allow it silently, deny it with an explanation, or ask the user for approval. The decision depends on the permission mode, any configured rules, and whether the user has already approved this tool during the session.

This is the first chapter of Part III: Safety & Control. By the end of it, your agent will no longer blindly obey the LLM. It will ask permission first.

```bash
cargo test -p claw-code test_ch10
```

---

## The problem: a spectrum of trust

Not every tool call is equally risky. Reading a file is harmless. Writing a file is recoverable (you can revert with git). Running `rm -rf /` is catastrophic. A good permission system should treat these differently.

At the same time, not every user wants the same level of control. Some users want to approve every action. Some want to approve only dangerous ones. Some are running automated pipelines and want no prompts at all. And some are in planning mode, where the agent should only observe, never modify.

This gives us two dimensions to work with:

1. **Tool risk level** -- How dangerous is this tool? (The `is_read_only()` and `is_destructive()` flags from Chapter 9.)
2. **User trust level** -- How much control does the user want? (The permission mode.)

The permission engine combines both dimensions into a single decision. Here is the table from Chapter 9, now the specification we will implement:

| Category    | Plan mode | Auto mode | Default mode |
|-------------|-----------|-----------|--------------|
| Read-only   | Allowed   | Allowed   | Allowed      |
| Write       | Denied    | Allowed   | Ask user     |
| Destructive | Denied    | Ask user  | Ask user     |

Read-only tools are always safe. Destructive tools always require caution. Everything in between depends on how much the user trusts the agent.

---

## Permission types

The permission system introduces several new types in `src/types/permission.rs`. Let's walk through each one.

### Permission: the decision

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Permission {
    /// Tool call is allowed without asking.
    Allow,
    /// Tool call is blocked.
    Deny(String),
    /// User must be prompted for approval.
    Ask(String),
}
```

Three variants, one for each possible outcome. `Allow` means execute immediately -- no prompt, no delay. `Deny` means block the call entirely -- the tool never runs, and the string explains why. `Ask` means pause and show the user a prompt -- the string is the question to display (something like "Allow write?" or "\`bash\` is destructive -- requires approval even in auto mode").

The string payload in `Deny` and `Ask` is not decorative. It flows through to the user interface and to the model. When a tool call is denied, the denial reason becomes a `ToolResult::error(...)` that the LLM sees in the conversation history. The model can read "permission mode is DontAsk" and understand why its request was rejected. When a tool call needs approval, the `Ask` message becomes the prompt the user sees in the terminal.

### PermissionMode: user trust level

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionMode {
    Default,
    Auto,
    Bypass,
    Plan,
    DontAsk,
}
```

Five modes, ordered roughly from most restrictive to least:

- **DontAsk** -- Deny everything without prompting. Useful when running the agent in a context where no human is available to answer prompts and you want maximum safety.
- **Plan** -- Only read-only tools execute. The agent can observe and reason but never modify. This is the "look but don't touch" mode.
- **Default** -- Read-only tools execute freely. Everything else prompts the user. This is the standard interactive experience -- what you see when Claude Code asks "Allow bash: rm -rf target?" before running a command.
- **Auto** -- Read-only and non-destructive write tools execute freely. Only destructive tools prompt. This is for users who trust the model with file operations but want a checkpoint before shell commands.
- **Bypass** -- Allow everything without prompting. No guardrails. Used for testing, CI pipelines, and people who like to live dangerously.

### PermissionRule: explicit overrides

```rust
#[derive(Debug, Clone)]
pub struct PermissionRule {
    pub tool_pattern: String,
    pub behavior: PermissionBehavior,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PermissionBehavior {
    Allow,
    Deny,
    Ask,
}
```

Rules let users override the mode-based defaults for specific tools. A `PermissionRule` matches tool names with a glob-style pattern and assigns a behavior: always allow, always deny, or always ask.

For example, you might configure Default mode but add a rule that allows `write` without prompting -- because you trust the model with file writes in this particular project. Or you might use Auto mode but add a rule that denies `bash` entirely -- because this is a read-heavy analysis task and you want to prevent any command execution.

Rules take priority over mode-based defaults. This is the key design principle: specific overrides beat general policies.

### PermissionSource: audit trail

```rust
#[derive(Debug, Clone)]
pub enum PermissionSource {
    Rule(PermissionRule),
    Mode(PermissionMode),
    Hook(String),
    Safety(String),
    Session,
}
```

Every permission decision comes with a source that explains *why* the decision was made. Was it a rule match? The permission mode? A session approval? A safety check? A hook?

The source is returned alongside the `Permission` from the `check()` method. Callers can use it for logging, debugging, or displaying context to the user. When the terminal shows "Allowed (auto mode)" or "Denied by rule: bash", that information comes from the `PermissionSource`.

The `Hook` and `Safety` variants are used by the hook system (Chapter 12) and safety checker (Chapter 11) respectively. We define them here so the type is complete, but we will not use them until those chapters.

---

## The PermissionEngine

With the types defined, we can build the engine itself. Open `src/permission/mod.rs`:

```rust
pub struct PermissionEngine {
    mode: PermissionMode,
    rules: Vec<PermissionRule>,
    /// Tools the user has approved during this session.
    session_approvals: Mutex<HashSet<String>>,
}
```

Three fields:

- **`mode`** -- The active permission mode. Set once at construction.
- **`rules`** -- An ordered list of permission rules. First match wins. Set via the builder.
- **`session_approvals`** -- A set of tool names the user has approved during this session. Protected by a `Mutex` because the engine might be shared across async tasks.

The constructor and builder are minimal:

```rust
impl PermissionEngine {
    pub fn new(mode: PermissionMode) -> Self {
        Self {
            mode,
            rules: Vec::new(),
            session_approvals: Mutex::new(HashSet::new()),
        }
    }

    pub fn with_rules(mut self, rules: Vec<PermissionRule>) -> Self {
        self.rules = rules;
        self
    }

    pub fn mode(&self) -> &PermissionMode {
        &self.mode
    }
}
```

Nothing surprising. The mode is required at construction. Rules are optional. Session approvals start empty and accumulate as the user interacts with the agent.

---

## The check pipeline

The core of the engine is the `check` method. It takes a tool name and a reference to the tool, and returns a `(Permission, PermissionSource)` pair. The pipeline has six stages, evaluated in order. The first stage that produces a definitive answer wins.

```rust
pub fn check(&self, tool_name: &str, tool: &dyn Tool) -> (Permission, PermissionSource) {
    // 1. Bypass — skip all checks
    // 2. DontAsk — deny everything
    // 3. Plan — allow read-only, deny rest
    // 4. Check rules (first match wins)
    // 5. Session approvals
    // 6. Mode-based default (Auto or Default)
}
```

Let's walk through each stage.

### Stage 1: Bypass mode

```rust
if self.mode == PermissionMode::Bypass {
    return (
        Permission::Allow,
        PermissionSource::Mode(PermissionMode::Bypass),
    );
}
```

If the mode is Bypass, allow everything immediately. No rules are checked, no flags are examined. This is the "I know what I'm doing" escape hatch.

Bypass is checked first because it short-circuits everything. There is no point evaluating rules or tool flags if the answer is always "yes."

### Stage 2: DontAsk mode

```rust
if self.mode == PermissionMode::DontAsk {
    return (
        Permission::Deny("permission mode is DontAsk".into()),
        PermissionSource::Mode(PermissionMode::DontAsk),
    );
}
```

If the mode is DontAsk, deny everything immediately. Even read-only tools are denied. This might seem extreme, but the intent is clear: if no human is available to answer prompts, nothing should execute. The denial reason flows to the model, so it understands why its requests are being rejected.

### Stage 3: Plan mode

```rust
if self.mode == PermissionMode::Plan {
    if tool.is_read_only() {
        return (
            Permission::Allow,
            PermissionSource::Mode(PermissionMode::Plan),
        );
    } else {
        return (
            Permission::Deny(format!(
                "`{}` is not read-only — blocked in plan mode",
                tool_name
            )),
            PermissionSource::Mode(PermissionMode::Plan),
        );
    }
}
```

Plan mode allows read-only tools and denies everything else. This is where the `is_read_only()` flag from Chapter 9 gets enforced. The agent can read files, search code, and list directories, but it cannot write, edit, or execute commands.

Note that Plan mode does not check rules. If you are in Plan mode, the mode wins -- period. This is a deliberate safety decision. You do not want a misconfigured rule accidentally allowing writes in Plan mode.

### Stage 4: Permission rules

```rust
for rule in &self.rules {
    if pattern_matches(&rule.tool_pattern, tool_name) {
        let permission = match &rule.behavior {
            PermissionBehavior::Allow => Permission::Allow,
            PermissionBehavior::Deny => {
                Permission::Deny(format!("denied by rule: {}", rule.tool_pattern))
            }
            PermissionBehavior::Ask => {
                Permission::Ask(format!(
                    "rule requires approval: {}", rule.tool_pattern
                ))
            }
        };
        return (permission, PermissionSource::Rule(rule.clone()));
    }
}
```

If Bypass, DontAsk, and Plan did not short-circuit, we check the configured rules. Rules are evaluated in order -- the first rule whose pattern matches the tool name wins.

This is a critical design choice: **first match wins**. If you have two rules:

```
1. bash  -> Deny
2. *     -> Allow
```

Then `bash` hits rule 1 and is denied. Everything else hits rule 2 and is allowed. If the order were reversed, rule 2 would match everything first and rule 1 would never fire.

The rule source is included in the return value, so callers can display which rule triggered the decision.

### Stage 5: Session approvals

```rust
if self.is_session_approved(tool_name) {
    return (Permission::Allow, PermissionSource::Session);
}
```

If no rule matched, check whether the user has already approved this tool during the current session. Session approvals are recorded when the user says "yes" to an `Ask` prompt. Once approved, the tool runs without prompting for the rest of the session.

Session approvals are per-tool, not global. Approving `write` does not approve `bash`. This is deliberate -- the user should make a conscious choice for each tool they trust.

### Stage 6: Mode-based defaults

```rust
match self.mode {
    PermissionMode::Auto => {
        if tool.is_destructive() {
            (
                Permission::Ask(format!(
                    "`{}` is destructive — requires approval even in auto mode",
                    tool_name
                )),
                PermissionSource::Mode(PermissionMode::Auto),
            )
        } else {
            (
                Permission::Allow,
                PermissionSource::Mode(PermissionMode::Auto),
            )
        }
    }
    PermissionMode::Default => {
        if tool.is_read_only() {
            (
                Permission::Allow,
                PermissionSource::Mode(PermissionMode::Default),
            )
        } else {
            (
                Permission::Ask(format!("Allow {}?", tool_name)),
                PermissionSource::Mode(PermissionMode::Default),
            )
        }
    }
    _ => unreachable!(),
}
```

If nothing else has produced a decision, fall back to the mode's default behavior.

**Auto mode**: Allow everything except destructive tools. The `is_destructive()` flag from Chapter 9 is the dividing line. Read-only tools? Allowed. Write tools? Allowed -- they are recoverable. Destructive tools (bash)? Ask the user. This is the sweet spot for experienced users who trust the model with file operations but want a checkpoint before shell commands.

**Default mode**: Allow read-only tools, ask for everything else. The `is_read_only()` flag is the dividing line. This is the most conservative interactive mode -- every write, edit, and command requires explicit approval.

The `_ => unreachable!()` arm covers Bypass, DontAsk, and Plan, which were already handled in stages 1-3. The code will never reach this point for those modes.

---

## Pattern matching

The `pattern_matches` helper implements simple glob-style matching for tool name patterns:

```rust
fn pattern_matches(pattern: &str, name: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return name.starts_with(prefix);
    }
    pattern == name
}
```

Three cases:

- **`"*"`** -- Matches everything. A rule with `tool_pattern: "*"` applies to all tools.
- **`"prefix*"`** -- Matches any tool name starting with the prefix. `"file_*"` matches `"file_read"`, `"file_write"`, `"file_edit"`.
- **Exact match** -- `"bash"` matches only `"bash"`.

This is intentionally simple. Claude Code uses a more sophisticated matching system with nested path patterns and regex support. Our version handles the common cases -- exact names, namespaced prefixes, and the catch-all wildcard -- without pulling in a glob library.

---

## Session approvals

When the `check` method returns `Permission::Ask`, the caller (typically the QueryEngine or UI layer) prompts the user. If the user says yes, the caller records the approval:

```rust
pub fn approve_session(&self, tool_name: &str) {
    self.session_approvals
        .lock()
        .unwrap()
        .insert(tool_name.to_string());
}
```

Subsequent checks for the same tool will find it in the session approvals set (stage 5) and return `Permission::Allow` without prompting again.

```rust
pub fn is_session_approved(&self, tool_name: &str) -> bool {
    self.session_approvals
        .lock()
        .unwrap()
        .contains(tool_name)
}
```

The approval is stored as a tool name string in a `HashSet`, protected by a `Mutex`. The `Mutex` is necessary because the `PermissionEngine` might be shared across async tasks (for example, the QueryEngine and the UI running concurrently). The lock is held only for the duration of the `insert` or `contains` call -- microseconds at most -- so contention is not a concern.

Session approvals can be cleared explicitly:

```rust
pub fn clear_session(&self) {
    self.session_approvals.lock().unwrap().clear();
}
```

This is useful when the user wants to reset their approvals mid-session, or when the agent transitions between tasks that require different trust levels.

Three properties of session approvals are worth emphasizing:

1. **Per-tool, not global.** Approving `write` does not approve `bash`. Each tool is a separate trust decision.
2. **Session-scoped, not persistent.** Approvals live in memory and vanish when the process exits. There is no file, no database, no persistence. If you restart the agent, you start with a clean slate.
3. **Below rules in priority.** A rule that denies `bash` will deny it even if the user previously approved `bash` in the session. Rules are checked at stage 4; session approvals at stage 5. Rules win.

---

## Putting it all together: a complete trace

Let's trace through a realistic scenario to see how the pipeline works end to end.

A user starts the agent in Default mode with one rule: `write` is always allowed.

```rust
let engine = PermissionEngine::new(PermissionMode::Default)
    .with_rules(vec![
        PermissionRule {
            tool_pattern: "write".into(),
            behavior: PermissionBehavior::Allow,
        },
    ]);
```

Now the LLM makes three tool calls in sequence. Here is what happens at each one:

**Call 1: `read("src/main.rs")`**

```
Stage 1: Mode is Default, not Bypass.   -> continue
Stage 2: Mode is Default, not DontAsk.  -> continue
Stage 3: Mode is Default, not Plan.     -> continue
Stage 4: Rule "write" does not match "read". No more rules. -> continue
Stage 5: "read" not in session approvals. -> continue
Stage 6: Default mode. ReadTool.is_read_only() == true. -> Allow
```

Result: `(Allow, Mode(Default))`. The read executes silently.

**Call 2: `write("src/main.rs", ...)`**

```
Stage 1: Not Bypass.   -> continue
Stage 2: Not DontAsk.  -> continue
Stage 3: Not Plan.     -> continue
Stage 4: Rule "write" matches "write". Behavior: Allow. -> Allow
```

Result: `(Allow, Rule("write"))`. The write executes silently -- the rule overrides what Default mode would normally do (ask the user).

**Call 3: `bash("cargo test")`**

```
Stage 1: Not Bypass.   -> continue
Stage 2: Not DontAsk.  -> continue
Stage 3: Not Plan.     -> continue
Stage 4: Rule "write" does not match "bash". No more rules. -> continue
Stage 5: "bash" not in session approvals. -> continue
Stage 6: Default mode. BashTool.is_read_only() == false. -> Ask
```

Result: `(Ask("Allow bash?"), Mode(Default))`. The UI prompts the user. If the user approves, the caller calls `engine.approve_session("bash")`, and subsequent bash calls will be allowed via stage 5.

---

## How the engine integrates with the QueryEngine

The `PermissionEngine` is designed to be called from inside the QueryEngine's `execute_tools` method. The integration point is conceptually simple:

```
For each tool call from the LLM:
    1. Look up the tool in the ToolSet
    2. Call permission_engine.check(tool_name, tool)
    3. Match on the Permission:
       - Allow  -> execute the tool
       - Deny   -> return ToolResult::error(reason)
       - Ask    -> prompt the user, then execute or deny
```

We will wire this up fully in later chapters. For now, the `PermissionEngine` is a standalone component with a clean interface: give it a tool name and a tool reference, get back a decision. This separation makes it testable in isolation -- which is exactly what the chapter 10 tests do.

---

## How Claude Code does it

Claude Code's permission system follows the same architecture but with more granularity.

**Permission modes.** Claude Code has the same core modes -- a default interactive mode, an auto-approve mode, and a plan mode. The mode is set via CLI flags (`--dangerously-skip-permissions` for bypass, `--plan` for plan mode) or interactively during the session.

**Tool groups.** Rather than individual tool flags, Claude Code organizes tools into permission groups. File tools, git tools, shell tools, and MCP tools each have group-level policies. A single rule can allow or deny an entire group. Our per-tool `is_read_only()` and `is_destructive()` flags achieve a similar effect but at the individual tool level.

**Per-path rules.** Claude Code's rules can match not just tool names but also tool arguments -- specifically file paths. A rule like "allow write to `src/**`" permits writes within the source directory but blocks writes elsewhere. Our rules match only on tool names, which is simpler but less precise.

**Session approvals.** Claude Code's session approval system works the same way -- once the user approves a tool, it stays approved for the session. The approval is per-tool-name, stored in memory, and cleared on session reset.

**Layered evaluation.** The evaluation pipeline is the same: check mode-level short-circuits first, then match rules, then fall back to mode-based defaults. The ordering ensures that specific policies override general ones, just as in our implementation.

The core insight is the same in both systems: the permission engine is a pure function from `(mode, rules, session_state, tool_metadata)` to `(Permission, PermissionSource)`. It does not execute tools. It does not modify state (except session approvals). It just answers the question: should this tool call proceed?

---

## Tests

Run all chapter 10 tests:

```bash
cargo test -p claw-code test_ch10
```

The tests use three minimal `Tool` implementations -- `ReadOnlyTool`, `WriteTool`, and `DestructiveTool` -- that return the correct flag values without doing any actual work. This lets us test permission logic in isolation, without real file I/O or process execution.

Here is what each test covers:

### Mode tests

- **`test_ch10_bypass_allows_everything`** -- Creates a `PermissionEngine` in Bypass mode and checks a destructive tool. Result must be `Allow` with source `Mode(Bypass)`. Even the most dangerous tool sails through.

- **`test_ch10_dontask_denies_everything`** -- Creates an engine in DontAsk mode and checks a read-only tool. Result must be `Deny`. Even the safest tool is blocked.

- **`test_ch10_plan_allows_read_only`** -- Plan mode with a read-only tool. Result: `Allow`.

- **`test_ch10_plan_denies_write`** -- Plan mode with a write tool. Result: `Deny`.

- **`test_ch10_plan_denies_destructive`** -- Plan mode with a destructive tool. Result: `Deny`.

### Auto mode tests

- **`test_ch10_auto_allows_read_only`** -- Auto mode with a read-only tool. Result: `Allow`.

- **`test_ch10_auto_allows_non_destructive_write`** -- Auto mode with a write tool (not destructive). Result: `Allow`. This is the key distinction between Auto and Default -- Auto trusts writes.

- **`test_ch10_auto_asks_for_destructive`** -- Auto mode with a destructive tool. Result: `Ask`. Even in Auto mode, destructive tools require confirmation.

### Default mode tests

- **`test_ch10_default_allows_read_only`** -- Default mode with a read-only tool. Result: `Allow`.

- **`test_ch10_default_asks_for_write`** -- Default mode with a write tool. Result: `Ask`. This is the standard interactive behavior -- "Allow write?"

- **`test_ch10_default_asks_for_destructive`** -- Default mode with a destructive tool. Result: `Ask`.

### Rule tests

- **`test_ch10_rule_allow_overrides_default`** -- Default mode with a rule that allows `write`. The rule overrides the mode's default behavior (which would ask). Result: `Allow` with source `Rule(...)`.

- **`test_ch10_rule_deny_overrides_auto`** -- Auto mode with a rule that denies `write`. The rule overrides Auto's permissiveness. Result: `Deny`.

- **`test_ch10_wildcard_rule`** -- A rule with pattern `"*"` and behavior `Allow`. Matches everything, including destructive tools.

- **`test_ch10_prefix_wildcard_rule`** -- A rule with pattern `"file_*"` and behavior `Allow`. Matches `"file_read"` but not `"bash"`. The non-matching tool falls through to mode defaults.

- **`test_ch10_first_rule_wins`** -- Two rules: `bash -> Deny`, then `* -> Allow`. The `bash` tool hits the first rule and is denied. This verifies that rule evaluation stops at the first match.

### Session approval tests

- **`test_ch10_session_approval`** -- In Default mode, a write tool initially returns `Ask`. After calling `approve_session("write")`, the same check returns `Allow` with source `Session`.

- **`test_ch10_session_approval_clear`** -- Approves a tool, verifies it is approved, clears all session approvals, verifies it is no longer approved.

- **`test_ch10_session_approval_does_not_cross_tools`** -- Approves `write`, then checks that `bash` is not approved. Per-tool isolation.

### Integration test

- **`test_ch10_permission_hierarchy`** -- The comprehensive test. Creates engines in Plan, Auto, and Default modes and checks all three tool categories against each mode. Verifies the complete permission table:

```
              Plan      Auto      Default
Read-only     Allow     Allow     Allow
Write         Deny      Allow     Ask
Destructive   Deny      Ask       Ask
```

This single test encodes the entire permission policy. If it passes, the hierarchy is correctly implemented.

---

## Recap

In this chapter you built the `PermissionEngine` -- the gatekeeper between the LLM's requests and your tools. The key ideas:

- **Three outcomes** -- `Allow`, `Deny`, `Ask`. Every tool call gets one of these before it runs.
- **Five modes** -- Bypass, DontAsk, Plan, Auto, Default. Each represents a different trust level, from "allow everything" to "deny everything."
- **Ordered pipeline** -- Mode short-circuits first (Bypass, DontAsk, Plan), then rules, then session approvals, then mode defaults. Specific policies beat general ones.
- **First-match rules** -- Glob-style patterns evaluated in order. The first matching rule wins. This gives users fine-grained control over which tools require approval.
- **Session approvals** -- Once the user says yes, that tool is approved for the session. Per-tool, in-memory, not persistent.
- **Audit trail** -- Every decision comes with a `PermissionSource` explaining why.

The engine is pure logic -- it does not execute tools, and it does not interact with the user. It takes a tool name and a tool reference, and returns a decision. This separation makes it testable, composable, and easy to reason about.

---

## What's next

The permission engine decides *whether* a tool call should run based on who the tool is and what mode the user is in. But it does not look at *what the tool is being asked to do*. A bash tool is bash whether it runs `ls` or `rm -rf /`. A write tool is a write tool whether it targets `src/main.rs` or `.env`.

Chapter 11 adds the `SafetyChecker` -- static analysis of tool arguments that catches dangerous patterns before the permission prompt even appears. It checks paths against allowed directories, matches filenames against protected patterns (`.env`, `.git/config`), and scans bash commands for blocked substrings (`rm -rf /`, `sudo`, fork bombs). The safety checker runs alongside the permission engine, and its denials override everything.

# Chapter 16: Configuration

Every production agent needs configurable behavior. Which model should it use?
What is the context window limit? Are there directories it should never touch?
Hardcoding these values works for a tutorial, but a real tool needs to let
users override them -- and override them at different levels.

Claude Code solves this with a multi-level configuration hierarchy: defaults,
project settings, user settings, and environment variables. Each layer can
override the one below it. This chapter walks through our implementation of the
same pattern.

## The layered config model

The core idea is simple: start with sensible defaults, then let each successive
layer override specific values while leaving the rest untouched.

```text
Priority (highest wins)
========================
  4. Environment variables   MINI_CLAW_MODEL=...
  3. User config             ~/.config/mini-claw/config.toml
  2. Project config          .mini-claw/config.toml
  1. Defaults                compiled into the binary
```

Why four layers?

- **Defaults** ensure the agent works out of the box with zero configuration.
- **Project config** lives in the repository (`.mini-claw/config.toml`). It
  sets project-specific rules: blocked commands, protected files, MCP servers.
  Every contributor on the project shares these settings.
- **User config** lives in the user's home directory
  (`~/.config/mini-claw/config.toml` on Linux/macOS). It captures personal
  preferences: preferred model, API base URL, custom instructions. These apply
  across all projects.
- **Environment variables** override everything. They are useful for CI
  pipelines, one-off experiments, or temporarily switching models without
  editing any file.

This is the same pattern used by Git (system, global, local config), npm
(`.npmrc` at multiple levels), and many other CLI tools. It is worth
understanding because you will see it everywhere and can reuse it in your own
projects.

## The Config struct

Open `mini-claw-code/src/config.rs`. The top-level struct holds every
configurable value:

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub model: String,
    pub base_url: String,
    pub max_context_tokens: u64,
    pub preserve_recent: usize,
    pub allowed_directory: Option<String>,
    pub protected_patterns: Vec<String>,
    pub blocked_commands: Vec<String>,
    pub mcp_servers: Vec<McpServerConfig>,
    pub hooks: HooksConfig,
    pub instructions: Option<String>,
}
```

A quick field-by-field tour:

| Field | Purpose |
|---|---|
| `model` | LLM model identifier, e.g. `"anthropic/claude-sonnet-4"` |
| `base_url` | API endpoint URL |
| `max_context_tokens` | Token budget before the agent triggers context compaction |
| `preserve_recent` | Number of recent messages to keep during compaction |
| `allowed_directory` | If set, tools cannot access files outside this directory |
| `protected_patterns` | Glob patterns for files that tools should never write to |
| `blocked_commands` | Shell command patterns that the bash tool should reject |
| `mcp_servers` | MCP server definitions (name, command, args, env) |
| `hooks` | Pre/post tool execution hooks |
| `instructions` | Custom system prompt text |

The `#[serde(default)]` attribute on the struct is critical. It tells serde:
"if a field is missing from the TOML input, use its `Default` value instead of
returning an error." This means a config file can specify just one field and
every other field gets a sensible default.

### Config vs ConfigOverlay

`Config` is the fully-resolved shape we hand to the rest of the agent -- every
field already has a value. When we *load* a layer, though, we need to know
which fields that layer actually set, because the merge logic has to
distinguish "the user did not mention this field" from "the user explicitly
set this field, even if the value happens to equal the default." `Config`
alone cannot tell those two cases apart.

The answer is a sibling struct, `ConfigOverlay`, whose fields are `Option<T>`.
Serde deserializes a missing TOML key as `None` and a present one as
`Some(value)`:

```rust
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ConfigOverlay {
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub max_context_tokens: Option<u64>,
    pub preserve_recent: Option<usize>,
    pub allowed_directory: Option<String>,
    pub protected_patterns: Option<Vec<String>>,
    pub blocked_commands: Option<Vec<String>>,
    pub mcp_servers: Option<Vec<McpServerConfig>>,
    pub hooks: Option<HooksConfig>,
    pub instructions: Option<String>,
}
```

The struct-level `#[serde(default)]` makes serde fall back to `Default::default()` for any missing TOML key. For `Option<T>`, that is `None` — which is exactly the "field absent → unset" mapping the merge logic needs.

Every field gets `Option<T>`, even the `Vec<T>` ones. That last detail matters:
an overlay that sets `protected_patterns = []` in TOML means "clear the list,"
which is genuinely different from "did not mention the list." `Option<Vec<T>>`
represents both cases cleanly; a bare `Vec<T>` cannot.

`Config` and `ConfigOverlay` have the same shape but play different roles.
`Config` is the output -- a complete set of values downstream code can read.
`ConfigOverlay` is the transport format -- a partial, optional view used only
while merging layers together.

## Defaults

The `Default` implementation defines the baseline:

```rust
impl Default for Config {
    fn default() -> Self {
        Self {
            model: "openrouter/free".into(),
            base_url: "https://openrouter.ai/api/v1".into(),
            max_context_tokens: 100_000,
            preserve_recent: 6,
            allowed_directory: None,
            protected_patterns: vec![
                ".env".into(),
                ".env.*".into(),
                ".git/**".into(),
            ],
            blocked_commands: vec![
                "rm -rf /".into(),
                "sudo *".into(),
                "curl * | bash".into(),
                "curl * | sh".into(),
            ],
            mcp_servers: Vec::new(),
            hooks: HooksConfig::default(),
            instructions: None,
        }
    }
}
```

The defaults are deliberately conservative. The free model keeps the barrier to
entry low. The protected patterns prevent the agent from overwriting `.env`
files or anything inside `.git/`. The blocked commands stop the most dangerous
shell operations. A user who wants to loosen these restrictions can do so in
their config file.

## Nested config types

### McpServerConfig

MCP servers are defined as a list of entries. Each entry describes how to spawn
a server process:

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

In TOML, this uses the double-bracket array-of-tables syntax:

```toml
[[mcp_servers]]
name = "filesystem"
command = "npx"
args = ["@anthropic/mcp-server-filesystem"]
```

The `#[serde(default)]` on `args` and `env` means you can omit them if the
server needs no arguments or extra environment variables.

### HooksConfig and ShellHookConfig

Hooks let you run shell commands before or after the agent executes a tool.
For example, you might lint a file after the agent writes to it, or log every
bash command.

```rust
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct HooksConfig {
    pub pre_tool: Vec<ShellHookConfig>,
    pub post_tool: Vec<ShellHookConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShellHookConfig {
    pub tool_pattern: Option<String>,
    pub command: String,
    #[serde(default = "default_hook_timeout")]
    pub timeout_ms: u64,
}

fn default_hook_timeout() -> u64 {
    5000
}
```

A few things to note:

- `HooksConfig` uses `#[serde(default)]` at the struct level, so a config file
  that does not mention hooks at all will get empty `pre_tool` and `post_tool`
  vectors.
- `ShellHookConfig` uses `#[serde(default = "default_hook_timeout")]` on
  `timeout_ms`. This is a different form of the default attribute: instead of
  using the type's `Default` trait, it calls a specific function. Here,
  `default_hook_timeout()` returns 5000 milliseconds.
- `tool_pattern` is an `Option<String>`. When `None`, the hook runs for every
  tool. When set to something like `"bash"`, it only runs for the bash tool.

In TOML:

```toml
[[hooks.pre_tool]]
command = "echo pre"
tool_pattern = "bash"
timeout_ms = 3000
```

## TOML deserialization

The `toml` crate handles deserialization. Because `Config` derives
`Deserialize` and has `#[serde(default)]`, parsing a minimal TOML file works
seamlessly:

```rust
let toml_str = r#"
    model = "anthropic/claude-sonnet-4"
    max_context_tokens = 50000
"#;
let config: Config = toml::from_str(toml_str).unwrap();
```

This produces a `Config` where `model` is `"anthropic/claude-sonnet-4"`,
`max_context_tokens` is `50000`, and every other field has its default value.
The `#[serde(default)]` attribute is doing all the heavy lifting -- without it,
serde would require every field to be present in the TOML.

This is also why we chose TOML over JSON for configuration files. TOML is
designed for human-editable config: it supports comments, has clean syntax for
nested tables and arrays, and does not require trailing commas or quoting of
simple strings.

## ConfigLoader

The `ConfigLoader` struct ties everything together. It has no fields -- it is
just a namespace for the loading logic:

```rust
pub struct ConfigLoader;
```

### The load() method

`ConfigLoader::load()` is the main entry point. It applies all four layers in
order:

```rust
impl ConfigLoader {
    pub fn load() -> Config {
        let mut config = Config::default();

        // Layer 1: Project config
        if let Some(project_overlay) = Self::load_file(".mini-claw/config.toml") {
            Self::merge(&mut config, project_overlay);
        }

        // Layer 2: User config
        if let Some(user_dir) = dirs::config_dir() {
            let user_path = user_dir.join("mini-claw/config.toml");
            if let Some(user_overlay) = Self::load_overlay(&user_path) {
                Self::merge(&mut config, user_overlay);
            }
        }

        // Layer 3: Environment variable overrides
        if let Ok(model) = std::env::var("MINI_CLAW_MODEL") {
            config.model = model;
        }
        if let Ok(url) = std::env::var("MINI_CLAW_BASE_URL") {
            config.base_url = url;
        }
        if let Ok(tokens) = std::env::var("MINI_CLAW_MAX_TOKENS")
            && let Ok(n) = tokens.parse()
        {
            config.max_context_tokens = n;
        }

        config
    }
}
```

The flow:

1. Start with `Config::default()`.
2. If `.mini-claw/config.toml` exists in the current directory, parse it and
   merge it into the config.
3. Use the `dirs` crate to find the platform-appropriate user config directory
   (`~/.config` on Linux, `~/Library/Application Support` on macOS). If
   `mini-claw/config.toml` exists there, merge it in.
4. Check three environment variables (`MINI_CLAW_MODEL`, `MINI_CLAW_BASE_URL`,
   `MINI_CLAW_MAX_TOKENS`) and override the corresponding fields if set.

Each file loading step uses `if let Some(...)` -- if the file does not exist or
cannot be parsed, the step is silently skipped. This is intentional: config
files are optional at every level.

Notice the `let ... && let ...` syntax in the environment variable parsing for
`MINI_CLAW_MAX_TOKENS`. This is a let-chain: the inner `let Ok(n) =
tokens.parse()` only runs if the outer `let Ok(tokens)` succeeds. If the
environment variable exists but is not a valid number, the override is skipped.

### File loading helpers

Three helpers handle reading and parsing TOML files:

```rust
pub fn load_path(path: &Path) -> Option<Config> {
    let content = std::fs::read_to_string(path).ok()?;
    toml::from_str(&content).ok()
}

pub fn load_overlay(path: &Path) -> Option<ConfigOverlay> {
    let content = std::fs::read_to_string(path).ok()?;
    toml::from_str(&content).ok()
}

fn load_file(relative_path: &str) -> Option<ConfigOverlay> {
    let path = PathBuf::from(relative_path);
    Self::load_overlay(&path)
}
```

The `?` operator on `.ok()` converts `Result` into `Option`, so any I/O or
parse error produces `None` and the layer is skipped.

`load_overlay` is what the layered loader actually calls -- it returns the
partial `ConfigOverlay` so `merge` can see which fields were set.
`load_path` is kept as a convenience for callers that just want a
pre-resolved `Config` from a single file without going through the layered
pipeline.

### The merge strategy

The `merge()` method is where the layered override logic lives. Because the
overlay is `ConfigOverlay`, each field is already tagged with whether the
layer set it (`Some(_)`) or not (`None`). Merge becomes a straight walk:

```rust
fn merge(base: &mut Config, overlay: ConfigOverlay) {
    if let Some(model) = overlay.model {
        base.model = model;
    }
    if let Some(base_url) = overlay.base_url {
        base.base_url = base_url;
    }
    if let Some(tokens) = overlay.max_context_tokens {
        base.max_context_tokens = tokens;
    }
    if let Some(preserve_recent) = overlay.preserve_recent {
        base.preserve_recent = preserve_recent;
    }
    if overlay.allowed_directory.is_some() {
        base.allowed_directory = overlay.allowed_directory;
    }
    if let Some(patterns) = overlay.protected_patterns {
        base.protected_patterns = patterns;
    }
    if let Some(commands) = overlay.blocked_commands {
        base.blocked_commands = commands;
    }
    if let Some(servers) = overlay.mcp_servers {
        base.mcp_servers = servers;
    }
    if let Some(hooks) = overlay.hooks {
        base.hooks = hooks;
    }
    if overlay.instructions.is_some() {
        base.instructions = overlay.instructions;
    }
}
```

Every field follows the same rule: if the overlay has `Some(_)`, the base is
replaced; otherwise the base is left alone. No value comparisons, no special
cases.

An earlier version of this code tried to detect "was this field set?" by
comparing the overlay value against `Config::default()`. That heuristic silently
conflates "not set" with "set to a value that happens to equal the default," so
a higher-priority layer explicitly setting a field to the default value was
dropped and the previous layer's value leaked through -- a last-write-wins bug
(see issue #10). Moving the "set vs unset" distinction into the type system via
`Option<T>` on the overlay makes the merge correct and the intent obvious.

One subtlety worth noting: `Vec` fields on the overlay are `Option<Vec<T>>`,
not plain `Vec<T>`. That lets a layer explicitly set `protected_patterns = []`
in TOML to *clear* the list, which is different from omitting the field
entirely. `Option<Vec<T>>` encodes both cases; a bare `Vec<T>` cannot.

## Environment variable overrides

The environment variable layer is the simplest -- no file loading, no merging,
just direct assignment:

```rust
if let Ok(model) = std::env::var("MINI_CLAW_MODEL") {
    config.model = model;
}
```

Only three fields are exposed as environment variables: `model`, `base_url`,
and `max_context_tokens`. These are the values most likely to change between
runs. Complex structures like `mcp_servers` and `hooks` are not practical to
express as environment variables, so they are only configurable through files.

This is a common pattern in CLI tools: environment variables handle the "quick
override" case, while config files handle the "persistent, structured
settings" case.

## Running the tests

```bash
cargo test -p mini-claw-code ch16
```

The tests cover each layer and their interactions:

- **`test_ch16_default_config`** -- verifies that `Config::default()` returns
  sensible values: the free model, 100k token limit, non-empty protected
  patterns and blocked commands.

- **`test_ch16_load_from_toml`** -- parses a TOML string with two fields and
  checks that both are set correctly.

- **`test_ch16_default_fills_missing_fields`** -- parses a TOML string with
  only `model` set. Verifies that unspecified fields fall back to their
  defaults. This is the `#[serde(default)]` attribute in action.

- **`test_ch16_load_nonexistent_path`** -- calls `ConfigLoader::load_path()`
  on a path that does not exist. Confirms it returns `None` instead of
  panicking.

- **`test_ch16_mcp_server_config`** -- parses TOML with a `[[mcp_servers]]`
  block. Verifies that the array-of-tables syntax deserializes into a
  `Vec<McpServerConfig>` correctly.

- **`test_ch16_hooks_config`** -- parses TOML with a `[[hooks.pre_tool]]`
  block. Verifies the hook's command, tool pattern, and timeout.

- **`test_ch16_env_override`** -- sets `MINI_CLAW_MODEL` as an environment
  variable, calls `ConfigLoader::load()`, and verifies the model was
  overridden. Note that the test uses `unsafe` blocks around `set_var` and
  `remove_var` -- as of Rust 2024 edition, modifying environment variables is
  unsafe because it can cause undefined behavior when another thread reads the
  environment concurrently.

- **`test_ch16_protected_patterns_default`** -- verifies that the default
  protected patterns include `.env` and `.git/**`.

## Recap

- **Layered configuration** is a widely-used design pattern: defaults, project
  settings, user settings, and environment variables, each overriding the layer
  below.
- The `Config` struct uses `#[serde(default)]` so that TOML files only need to
  specify the fields they want to change.
- A sibling `ConfigOverlay` struct with `Option<T>` fields is what makes merge
  correct. `None` means the layer did not mention the field; `Some(v)` means it
  did, regardless of what `v` is.
- Nested types (`McpServerConfig`, `HooksConfig`, `ShellHookConfig`) model
  structured configuration with their own serde attributes and defaults.
- `ConfigLoader::load()` applies all four layers in order. Each file layer is
  parsed into a `ConfigOverlay` and merged with a single rule: every `Some(_)`
  replaces the base.
- Environment variables provide the highest-priority override for the most
  commonly changed fields.
- File loading is resilient: missing or unparseable files are silently skipped.

This pattern is reusable well beyond coding agents. Any CLI tool that needs
per-project and per-user settings can use the same approach: define a resolved
`Config` struct plus a partial `ConfigOverlay` with `Option<T>` fields, load
files from known paths into the overlay, merge `Some(_)` fields onto the base,
and apply environment variable overrides last.

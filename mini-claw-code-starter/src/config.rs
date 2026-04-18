use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Agent configuration with layered overrides.
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

impl Default for Config {
    fn default() -> Self {
        Self {
            model: "openrouter/free".into(),
            base_url: "https://openrouter.ai/api/v1".into(),
            max_context_tokens: 100_000,
            preserve_recent: 6,
            allowed_directory: None,
            protected_patterns: vec![".env".into(), ".env.*".into(), ".git/**".into()],
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

#[derive(Debug, Clone, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

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

/// A partial configuration used as an overlay.
///
/// Every field is `Option<T>` so the loader can distinguish between
/// "not set in the TOML file" (`None`) and "explicitly set to this
/// value, even if it equals the struct default" (`Some(x)`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ConfigOverlay {
    pub(crate) model: Option<String>,
    pub(crate) base_url: Option<String>,
    pub(crate) max_context_tokens: Option<u64>,
    pub(crate) preserve_recent: Option<usize>,
    pub(crate) allowed_directory: Option<String>,
    pub(crate) protected_patterns: Option<Vec<String>>,
    pub(crate) blocked_commands: Option<Vec<String>>,
    pub(crate) mcp_servers: Option<Vec<McpServerConfig>>,
    pub(crate) hooks: Option<HooksConfig>,
    pub(crate) instructions: Option<String>,
}

/// Loads and merges configuration from multiple sources.
pub struct ConfigLoader;

impl ConfigLoader {
    /// Load config by merging default -> project -> user -> env layers.
    ///
    /// Hints:
    /// - Start from `Config::default()`.
    /// - Merge `.mini-claw/config.toml` from the current directory (project layer).
    /// - Merge `<dirs::config_dir()>/mini-claw/config.toml` (user layer).
    /// - Apply env overrides: `MINI_CLAW_MODEL`, `MINI_CLAW_BASE_URL`,
    ///   `MINI_CLAW_MAX_TOKENS`.
    pub fn load() -> Config {
        unimplemented!(
            "TODO ch17: merge default → project → user → env layers into a single Config"
        )
    }

    /// Load a full, pre-merged `Config` from a path.
    ///
    /// Hint: `std::fs::read_to_string(path).ok().and_then(|s| toml::from_str(&s).ok())`.
    pub fn load_path(_path: &Path) -> Option<Config> {
        unimplemented!("TODO ch17: read the TOML file at path and parse it into a Config")
    }

    /// Load a partial `ConfigOverlay` from a path. The layered loader
    /// uses this so it can tell "unset" from "set to the default".
    #[allow(dead_code)]
    pub fn load_overlay(_path: &Path) -> Option<ConfigOverlay> {
        unimplemented!("TODO ch17: read the TOML file at path and parse it into a ConfigOverlay")
    }

    #[allow(dead_code)]
    fn load_file(relative_path: &str) -> Option<ConfigOverlay> {
        let path = PathBuf::from(relative_path);
        Self::load_overlay(&path)
    }

    /// Apply `overlay` onto `base`. Every `Some(_)` field replaces the
    /// corresponding field on `base`; `None` fields leave it untouched.
    ///
    /// Hints:
    /// - Pattern-match each `Option<T>` field: `if let Some(v) = overlay.x { base.x = v; }`.
    /// - For fields already `Option<T>` on `Config` (`allowed_directory`,
    ///   `instructions`), assign the whole overlay value when it is `Some(_)`.
    /// - Do NOT compare against `Config::default()` — that heuristic cannot
    ///   distinguish "unset" from "explicitly set to the default value" and
    ///   will silently drop later-layer overrides (see issue #10).
    #[allow(dead_code)]
    fn merge(_base: &mut Config, _overlay: ConfigOverlay) {
        unimplemented!("TODO ch17: apply every Some(_) field from overlay onto base")
    }
}

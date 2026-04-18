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
            "TODO ch14: merge default → project → user → env layers into a single Config"
        )
    }

    /// Load config from a path.
    ///
    /// Hint: `std::fs::read_to_string(path).ok().and_then(|s| toml::from_str(&s).ok())`.
    pub fn load_path(_path: &Path) -> Option<Config> {
        unimplemented!("TODO ch14: read the TOML file at path and parse it into a Config")
    }

    #[allow(dead_code)]
    fn load_file(relative_path: &str) -> Option<Config> {
        let path = PathBuf::from(relative_path);
        Self::load_path(&path)
    }

    /// Merge overlay into base. Non-default values in overlay override base.
    ///
    /// Hints:
    /// - Compare each field against `Config::default()` — if overlay differs, overwrite base.
    /// - For `Option<T>` fields, overlay wins when it is `Some(_)`.
    /// - For `Vec<T>` fields, overlay wins when non-empty and different from defaults.
    #[allow(dead_code)]
    fn merge(_base: &mut Config, _overlay: Config) {
        unimplemented!(
            "TODO ch14: copy each overlay field into base when it differs from the default"
        )
    }
}

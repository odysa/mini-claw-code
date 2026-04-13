use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Agent configuration with a 4-level hierarchy.
///
/// Priority (highest to lowest):
/// 1. Environment variables (`CLAW_*`)
/// 2. User config (`~/.config/claw-code/config.toml`)
/// 3. Project config (`.claw/config.toml`)
/// 4. Defaults
///
/// Each level can override fields from lower-priority levels.
/// Non-default values in higher-priority configs take precedence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// The model identifier (e.g., "anthropic/claude-sonnet-4").
    #[serde(default = "default_model")]
    pub model: String,

    /// Base URL for the API provider.
    #[serde(default = "default_base_url")]
    pub base_url: String,

    /// Maximum context window size in tokens.
    #[serde(default = "default_max_tokens")]
    pub max_context_tokens: u64,

    /// Number of recent messages to preserve during compaction.
    #[serde(default = "default_preserve_recent")]
    pub preserve_recent: usize,

    /// Restrict file operations to this directory.
    #[serde(default)]
    pub allowed_directory: Option<String>,

    /// Glob patterns for protected files that cannot be written to.
    #[serde(default)]
    pub protected_patterns: Vec<String>,

    /// Command substrings blocked in bash tool.
    #[serde(default)]
    pub blocked_commands: Vec<String>,

    /// Custom instructions injected into the system prompt.
    #[serde(default)]
    pub instructions: Option<String>,
}

fn default_model() -> String {
    "anthropic/claude-sonnet-4-20250514".into()
}

fn default_base_url() -> String {
    "https://openrouter.ai/api/v1".into()
}

fn default_max_tokens() -> u64 {
    200_000
}

fn default_preserve_recent() -> usize {
    10
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model: default_model(),
            base_url: default_base_url(),
            max_context_tokens: default_max_tokens(),
            preserve_recent: default_preserve_recent(),
            allowed_directory: None,
            protected_patterns: Vec::new(),
            blocked_commands: Vec::new(),
            instructions: None,
        }
    }
}

/// Loads and merges configuration from multiple sources.
///
/// The loader checks each layer in priority order and merges
/// non-default values. The result is a single `Config` with
/// all layers applied.
pub struct ConfigLoader {
    /// Directory to look for project config.
    project_dir: Option<PathBuf>,
}

impl ConfigLoader {
    pub fn new() -> Self {
        Self { project_dir: None }
    }

    pub fn project_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.project_dir = Some(dir.into());
        self
    }

    /// Load and merge all configuration layers.
    pub fn load(&self) -> Config {
        let mut config = Config::default();

        // Layer 1: Project config (.claw/config.toml)
        if let Some(ref dir) = self.project_dir {
            let project_path = dir.join(".claw").join("config.toml");
            if let Some(overlay) = Self::load_file(&project_path) {
                config = Self::merge(config, overlay);
            }
        }

        // Layer 2: User config (~/.config/claw-code/config.toml)
        if let Some(user_dir) = dirs::config_dir() {
            let user_path = user_dir.join("claw-code").join("config.toml");
            if let Some(overlay) = Self::load_file(&user_path) {
                config = Self::merge(config, overlay);
            }
        }

        // Layer 3: Environment variables (highest priority)
        config = Self::apply_env(config);

        config
    }

    /// Load a single TOML config file. Returns `None` if the file
    /// does not exist or cannot be parsed.
    pub fn load_file(path: &Path) -> Option<Config> {
        let content = std::fs::read_to_string(path).ok()?;
        toml::from_str(&content).ok()
    }

    /// Merge an overlay config onto a base config.
    ///
    /// Non-default values in the overlay replace the base.
    /// Collections (protected_patterns, blocked_commands) are replaced, not appended.
    pub fn merge(base: Config, overlay: Config) -> Config {
        let defaults = Config::default();

        Config {
            model: if overlay.model != defaults.model {
                overlay.model
            } else {
                base.model
            },
            base_url: if overlay.base_url != defaults.base_url {
                overlay.base_url
            } else {
                base.base_url
            },
            max_context_tokens: if overlay.max_context_tokens != defaults.max_context_tokens {
                overlay.max_context_tokens
            } else {
                base.max_context_tokens
            },
            preserve_recent: if overlay.preserve_recent != defaults.preserve_recent {
                overlay.preserve_recent
            } else {
                base.preserve_recent
            },
            allowed_directory: overlay.allowed_directory.or(base.allowed_directory),
            protected_patterns: if !overlay.protected_patterns.is_empty() {
                overlay.protected_patterns
            } else {
                base.protected_patterns
            },
            blocked_commands: if !overlay.blocked_commands.is_empty() {
                overlay.blocked_commands
            } else {
                base.blocked_commands
            },
            instructions: overlay.instructions.or(base.instructions),
        }
    }

    /// Apply environment variable overrides.
    ///
    /// Supported variables:
    /// - `CLAW_MODEL` — model identifier
    /// - `CLAW_BASE_URL` — API base URL
    /// - `CLAW_MAX_TOKENS` — max context tokens
    fn apply_env(mut config: Config) -> Config {
        if let Ok(model) = std::env::var("CLAW_MODEL") {
            config.model = model;
        }
        if let Ok(url) = std::env::var("CLAW_BASE_URL") {
            config.base_url = url;
        }
        if let Ok(tokens) = std::env::var("CLAW_MAX_TOKENS")
            && let Ok(n) = tokens.parse::<u64>()
        {
            config.max_context_tokens = n;
        }
        config
    }
}

impl Default for ConfigLoader {
    fn default() -> Self {
        Self::new()
    }
}

/// Token usage tracking across a session.
///
/// Accumulates token counts and computes estimated cost.
pub struct CostTracker {
    input_tokens: u64,
    output_tokens: u64,
    turn_count: u64,
    input_price_per_million: f64,
    output_price_per_million: f64,
}

impl CostTracker {
    pub fn new(input_price_per_million: f64, output_price_per_million: f64) -> Self {
        Self {
            input_tokens: 0,
            output_tokens: 0,
            turn_count: 0,
            input_price_per_million,
            output_price_per_million,
        }
    }

    /// Record token usage from a single turn.
    pub fn record(&mut self, usage: &crate::types::TokenUsage) {
        self.input_tokens += usage.input_tokens;
        self.output_tokens += usage.output_tokens;
        self.turn_count += 1;
    }

    pub fn total_input_tokens(&self) -> u64 {
        self.input_tokens
    }

    pub fn total_output_tokens(&self) -> u64 {
        self.output_tokens
    }

    pub fn turn_count(&self) -> u64 {
        self.turn_count
    }

    /// Compute the estimated cost in USD.
    pub fn total_cost(&self) -> f64 {
        let input_cost = self.input_tokens as f64 * self.input_price_per_million / 1_000_000.0;
        let output_cost = self.output_tokens as f64 * self.output_price_per_million / 1_000_000.0;
        input_cost + output_cost
    }

    /// Format a human-readable summary.
    pub fn summary(&self) -> String {
        format!(
            "tokens: {} in + {} out | cost: ${:.4}",
            self.input_tokens,
            self.output_tokens,
            self.total_cost()
        )
    }

    /// Reset all counters.
    pub fn reset(&mut self) {
        self.input_tokens = 0;
        self.output_tokens = 0;
        self.turn_count = 0;
    }
}

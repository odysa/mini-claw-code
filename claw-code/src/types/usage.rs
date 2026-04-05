use serde::{Deserialize, Serialize};

/// Token usage reported by the API for a single request.
///
/// Mirrors the `usage` field in OpenAI-compatible API responses.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Tokens served from prompt cache (cost-reduced).
    #[serde(default)]
    pub cache_read_tokens: u64,
    /// Tokens written to prompt cache (first access).
    #[serde(default)]
    pub cache_creation_tokens: u64,
}

impl TokenUsage {
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }
}

/// Accumulated usage across multiple turns, per model.
#[derive(Debug, Clone, Default)]
pub struct ModelUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cost_usd: f64,
    pub turn_count: u64,
}

impl ModelUsage {
    pub fn record(&mut self, usage: &TokenUsage, cost: f64) {
        self.input_tokens += usage.input_tokens;
        self.output_tokens += usage.output_tokens;
        self.cache_read_tokens += usage.cache_read_tokens;
        self.cache_creation_tokens += usage.cache_creation_tokens;
        self.cost_usd += cost;
        self.turn_count += 1;
    }
}

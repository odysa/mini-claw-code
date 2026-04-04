use std::sync::Mutex;

use crate::types::TokenUsage;

/// Tracks cumulative token usage and estimated cost across multiple turns.
pub struct CostTracker {
    inner: Mutex<CostTrackerInner>,
    /// Price per million input tokens (USD).
    input_price: f64,
    /// Price per million output tokens (USD).
    output_price: f64,
}

struct CostTrackerInner {
    total_input: u64,
    total_output: u64,
    turn_count: u64,
}

impl CostTracker {
    /// Create a new tracker with per-million-token pricing.
    ///
    /// For example, for a model charging $3/M input and $15/M output:
    /// ```
    /// use mini_claw_code::CostTracker;
    /// let tracker = CostTracker::new(3.0, 15.0);
    /// ```
    pub fn new(input_price_per_million: f64, output_price_per_million: f64) -> Self {
        Self {
            inner: Mutex::new(CostTrackerInner {
                total_input: 0,
                total_output: 0,
                turn_count: 0,
            }),
            input_price: input_price_per_million,
            output_price: output_price_per_million,
        }
    }

    /// Record usage from a single turn.
    pub fn record(&self, usage: &TokenUsage) {
        let mut inner = self.inner.lock().unwrap();
        inner.total_input += usage.input_tokens;
        inner.total_output += usage.output_tokens;
        inner.turn_count += 1;
    }

    /// Total input tokens across all recorded turns.
    pub fn total_input_tokens(&self) -> u64 {
        self.inner.lock().unwrap().total_input
    }

    /// Total output tokens across all recorded turns.
    pub fn total_output_tokens(&self) -> u64 {
        self.inner.lock().unwrap().total_output
    }

    /// Total number of turns recorded.
    pub fn turn_count(&self) -> u64 {
        self.inner.lock().unwrap().turn_count
    }

    /// Estimated total cost in USD.
    pub fn total_cost(&self) -> f64 {
        let inner = self.inner.lock().unwrap();
        (inner.total_input as f64 * self.input_price
            + inner.total_output as f64 * self.output_price)
            / 1_000_000.0
    }

    /// Format a summary string: `"tokens: 1234 in + 567 out | cost: $0.0123"`
    pub fn summary(&self) -> String {
        let inner = self.inner.lock().unwrap();
        let cost = (inner.total_input as f64 * self.input_price
            + inner.total_output as f64 * self.output_price)
            / 1_000_000.0;
        format!(
            "tokens: {} in + {} out | cost: ${:.4}",
            inner.total_input, inner.total_output, cost
        )
    }

    /// Reset all counters to zero.
    pub fn reset(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.total_input = 0;
        inner.total_output = 0;
        inner.turn_count = 0;
    }
}

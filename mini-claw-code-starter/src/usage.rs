use std::sync::Mutex;

use crate::types::TokenUsage;

/// Tracks cumulative token usage and estimated cost across multiple turns.
///
/// # Chapter 14: Token & Cost Tracking
pub struct CostTracker {
    inner: Mutex<CostTrackerInner>,
    input_price: f64,
    output_price: f64,
}

struct CostTrackerInner {
    total_input: u64,
    total_output: u64,
    turn_count: u64,
}

impl CostTracker {
    /// Create a tracker with per-million-token pricing.
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

    pub fn record(&self, usage: &TokenUsage) {
        let mut inner = self.inner.lock().unwrap();
        inner.total_input += usage.input_tokens;
        inner.total_output += usage.output_tokens;
        inner.turn_count += 1;
    }

    pub fn total_input_tokens(&self) -> u64 {
        self.inner.lock().unwrap().total_input
    }

    pub fn total_output_tokens(&self) -> u64 {
        self.inner.lock().unwrap().total_output
    }

    pub fn turn_count(&self) -> u64 {
        self.inner.lock().unwrap().turn_count
    }

    pub fn total_cost(&self) -> f64 {
        let inner = self.inner.lock().unwrap();
        Self::compute_cost(
            inner.total_input,
            inner.total_output,
            self.input_price,
            self.output_price,
        )
    }

    fn compute_cost(input: u64, output: u64, input_price: f64, output_price: f64) -> f64 {
        (input as f64 * input_price + output as f64 * output_price) / 1_000_000.0
    }

    pub fn summary(&self) -> String {
        let inner = self.inner.lock().unwrap();
        let cost = Self::compute_cost(
            inner.total_input,
            inner.total_output,
            self.input_price,
            self.output_price,
        );
        format!(
            "tokens: {} in + {} out | cost: ${:.4}",
            inner.total_input, inner.total_output, cost
        )
    }

    pub fn reset(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.total_input = 0;
        inner.total_output = 0;
        inner.turn_count = 0;
    }
}

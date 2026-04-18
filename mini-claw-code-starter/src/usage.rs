use std::sync::Mutex;

use crate::types::TokenUsage;

/// Tracks cumulative token usage and estimated cost across multiple turns.
///
/// # Chapter 17: Settings Hierarchy — Token & Cost Tracking
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

    /// Add a single turn's `TokenUsage` to the running totals and bump turn_count.
    pub fn record(&self, _usage: &TokenUsage) {
        unimplemented!("TODO ch17: lock the mutex, add input/output tokens, increment turn_count")
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

    /// Return the accumulated cost in dollars.
    ///
    /// Hint: `(total_input * input_price + total_output * output_price) / 1_000_000.0`.
    pub fn total_cost(&self) -> f64 {
        unimplemented!("TODO ch17: compute cost from tokens and per-million pricing")
    }

    /// Render a one-line human-readable summary like
    /// `"tokens: 123 in + 45 out | cost: $0.0012"`.
    pub fn summary(&self) -> String {
        unimplemented!("TODO ch17: format 'tokens: N in + M out | cost: $X.XXXX'")
    }

    /// Reset all counters to zero.
    pub fn reset(&self) {
        unimplemented!("TODO ch17: zero out total_input, total_output, and turn_count")
    }
}

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
        unimplemented!("Initialize inner Mutex with zeros, store prices")
    }

    pub fn record(&self, usage: &TokenUsage) {
        unimplemented!("Lock mutex, add tokens, increment turn_count")
    }

    pub fn total_input_tokens(&self) -> u64 {
        unimplemented!("Lock and return total_input")
    }

    pub fn total_output_tokens(&self) -> u64 {
        unimplemented!("Lock and return total_output")
    }

    pub fn turn_count(&self) -> u64 {
        unimplemented!("Lock and return turn_count")
    }

    pub fn total_cost(&self) -> f64 {
        unimplemented!(
            "Lock, compute cost: (input * input_price + output * output_price) / 1_000_000"
        )
    }

    /// Format: "tokens: N in + M out | cost: $X.XXXX"
    pub fn summary(&self) -> String {
        unimplemented!("Lock, compute cost, format string")
    }

    pub fn reset(&self) {
        unimplemented!("Lock, zero all counters")
    }
}

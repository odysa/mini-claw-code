use crate::types::TokenUsage;
use crate::usage::CostTracker;

#[test]
fn test_cost_tracker_empty_tracker() {
    let tracker = CostTracker::new(3.0, 15.0);
    assert_eq!(tracker.total_input_tokens(), 0);
    assert_eq!(tracker.total_output_tokens(), 0);
    assert_eq!(tracker.turn_count(), 0);
    assert!((tracker.total_cost() - 0.0).abs() < f64::EPSILON);
}

#[test]
fn test_cost_tracker_record_single_turn() {
    let tracker = CostTracker::new(3.0, 15.0);
    tracker.record(&TokenUsage {
        input_tokens: 1000,
        output_tokens: 500,
    });
    assert_eq!(tracker.total_input_tokens(), 1000);
    assert_eq!(tracker.total_output_tokens(), 500);
    assert_eq!(tracker.turn_count(), 1);
}

#[test]
fn test_cost_tracker_accumulates_across_turns() {
    let tracker = CostTracker::new(3.0, 15.0);
    tracker.record(&TokenUsage {
        input_tokens: 100,
        output_tokens: 50,
    });
    tracker.record(&TokenUsage {
        input_tokens: 200,
        output_tokens: 100,
    });
    tracker.record(&TokenUsage {
        input_tokens: 300,
        output_tokens: 150,
    });
    assert_eq!(tracker.total_input_tokens(), 600);
    assert_eq!(tracker.total_output_tokens(), 300);
    assert_eq!(tracker.turn_count(), 3);
}

#[test]
fn test_cost_tracker_cost_calculation() {
    // $3/M input, $15/M output
    let tracker = CostTracker::new(3.0, 15.0);
    tracker.record(&TokenUsage {
        input_tokens: 1_000_000,
        output_tokens: 1_000_000,
    });
    // Cost = 3.0 + 15.0 = 18.0
    assert!((tracker.total_cost() - 18.0).abs() < 0.001);
}

#[test]
fn test_cost_tracker_cost_small_numbers() {
    let tracker = CostTracker::new(3.0, 15.0);
    tracker.record(&TokenUsage {
        input_tokens: 1000,
        output_tokens: 200,
    });
    // Cost = 1000*3/1M + 200*15/1M = 0.003 + 0.003 = 0.006
    assert!((tracker.total_cost() - 0.006).abs() < 0.0001);
}

#[test]
fn test_cost_tracker_summary_format() {
    let tracker = CostTracker::new(3.0, 15.0);
    tracker.record(&TokenUsage {
        input_tokens: 1234,
        output_tokens: 567,
    });
    let summary = tracker.summary();
    assert!(summary.contains("1234 in"));
    assert!(summary.contains("567 out"));
    assert!(summary.contains("$"));
}

#[test]
fn test_cost_tracker_reset() {
    let tracker = CostTracker::new(3.0, 15.0);
    tracker.record(&TokenUsage {
        input_tokens: 1000,
        output_tokens: 500,
    });
    assert_eq!(tracker.turn_count(), 1);
    tracker.reset();
    assert_eq!(tracker.total_input_tokens(), 0);
    assert_eq!(tracker.total_output_tokens(), 0);
    assert_eq!(tracker.turn_count(), 0);
}

#[test]
fn test_cost_tracker_zero_usage() {
    let tracker = CostTracker::new(3.0, 15.0);
    tracker.record(&TokenUsage {
        input_tokens: 0,
        output_tokens: 0,
    });
    assert_eq!(tracker.turn_count(), 1);
    assert!((tracker.total_cost() - 0.0).abs() < f64::EPSILON);
}

#[test]
fn test_cost_tracker_token_usage_default() {
    let usage = TokenUsage::default();
    assert_eq!(usage.input_tokens, 0);
    assert_eq!(usage.output_tokens, 0);
}

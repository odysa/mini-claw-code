use crate::config::{Config, ConfigLoader, ConfigOverlay, CostTracker};
use crate::types::TokenUsage;

// ---------------------------------------------------------------------------
// Config defaults
// ---------------------------------------------------------------------------

#[test]
fn test_ch14_config_defaults() {
    let config = Config::default();
    assert!(config.model.contains("claude"));
    assert!(config.base_url.contains("openrouter"));
    assert_eq!(config.max_context_tokens, 200_000);
    assert_eq!(config.preserve_recent, 10);
    assert!(config.allowed_directory.is_none());
    assert!(config.protected_patterns.is_empty());
    assert!(config.blocked_commands.is_empty());
    assert!(config.instructions.is_none());
}

// ---------------------------------------------------------------------------
// Config merging
// ---------------------------------------------------------------------------

#[test]
fn test_ch14_merge_override_model() {
    let base = Config::default();
    let overlay = ConfigOverlay {
        model: Some("custom/model".into()),
        ..ConfigOverlay::default()
    };
    let merged = ConfigLoader::merge(base, overlay);
    assert_eq!(merged.model, "custom/model");
}

#[test]
fn test_ch14_merge_unset_field_keeps_base() {
    let base = Config {
        model: "custom/model".into(),
        ..Config::default()
    };
    let overlay = ConfigOverlay::default();
    let merged = ConfigLoader::merge(base, overlay);
    // Overlay did not set model (None), so base value is kept.
    assert_eq!(merged.model, "custom/model");
}

#[test]
fn test_ch14_second_overlay_always_wins() {
    // A later overlay that sets a field must override the previous
    // layer even when the new value equals the struct default —
    // otherwise "compare to default" ambiguity breaks last-write-wins.
    let base = Config::default();
    let first_overlay = ConfigOverlay {
        model: Some("some/model".into()),
        ..ConfigOverlay::default()
    };
    let second_overlay = ConfigOverlay {
        model: Some("anthropic/claude-sonnet-4-20250514".into()),
        ..ConfigOverlay::default()
    };
    let merged = ConfigLoader::merge(ConfigLoader::merge(base, first_overlay), second_overlay);
    assert_eq!(merged.model, "anthropic/claude-sonnet-4-20250514");
}

#[test]
fn test_ch14_merge_optional_fields() {
    let base = Config {
        allowed_directory: Some("/home/user".into()),
        ..Config::default()
    };
    let overlay = ConfigOverlay::default();
    let merged = ConfigLoader::merge(base, overlay);
    // Unset overlay doesn't override Some.
    assert_eq!(merged.allowed_directory, Some("/home/user".into()));
}

#[test]
fn test_ch14_merge_overlay_replaces_optional() {
    let base = Config {
        allowed_directory: Some("/home/user".into()),
        ..Config::default()
    };
    let overlay = ConfigOverlay {
        allowed_directory: Some("/workspace".into()),
        ..ConfigOverlay::default()
    };
    let merged = ConfigLoader::merge(base, overlay);
    assert_eq!(merged.allowed_directory, Some("/workspace".into()));
}

#[test]
fn test_ch14_merge_collections_replace() {
    let base = Config {
        protected_patterns: vec![".env".into()],
        ..Config::default()
    };
    let overlay = ConfigOverlay {
        protected_patterns: Some(vec![".secret".into(), ".key".into()]),
        ..ConfigOverlay::default()
    };
    let merged = ConfigLoader::merge(base, overlay);
    // Collections are replaced, not appended.
    assert_eq!(merged.protected_patterns, vec![".secret", ".key"]);
}

#[test]
fn test_ch14_merge_unset_collection_keeps_base() {
    let base = Config {
        blocked_commands: vec!["rm -rf /".into()],
        ..Config::default()
    };
    let overlay = ConfigOverlay::default();
    let merged = ConfigLoader::merge(base, overlay);
    assert_eq!(merged.blocked_commands, vec!["rm -rf /"]);
}

#[test]
fn test_ch14_merge_explicit_empty_collection_overrides() {
    // Some(vec![]) means "clear the base list" — distinct from None ("not set").
    let base = Config {
        blocked_commands: vec!["rm -rf /".into()],
        ..Config::default()
    };
    let overlay = ConfigOverlay {
        blocked_commands: Some(Vec::new()),
        ..ConfigOverlay::default()
    };
    let merged = ConfigLoader::merge(base, overlay);
    assert!(merged.blocked_commands.is_empty());
}

// ---------------------------------------------------------------------------
// Config file loading
// ---------------------------------------------------------------------------

#[test]
fn test_ch14_load_toml_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        r#"
model = "test/model"
max_context_tokens = 50000
protected_patterns = [".env", ".secret"]
"#,
    )
    .unwrap();

    let overlay = ConfigLoader::load_file(&path).unwrap();
    assert_eq!(overlay.model.as_deref(), Some("test/model"));
    assert_eq!(overlay.max_context_tokens, Some(50000));
    assert_eq!(
        overlay.protected_patterns.as_deref(),
        Some([".env".to_string(), ".secret".to_string()].as_slice()),
    );
    // Fields not present in the TOML stay None.
    assert!(overlay.base_url.is_none());
    assert!(overlay.blocked_commands.is_none());
}

#[test]
fn test_ch14_load_missing_file() {
    let result = ConfigLoader::load_file(std::path::Path::new("/nonexistent/config.toml"));
    assert!(result.is_none());
}

#[test]
fn test_ch14_load_invalid_toml() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "not valid toml {{{").unwrap();

    let result = ConfigLoader::load_file(&path);
    assert!(result.is_none());
}

// ---------------------------------------------------------------------------
// ConfigLoader integration
// ---------------------------------------------------------------------------

#[test]
fn test_ch14_loader_with_project_dir() {
    let dir = tempfile::tempdir().unwrap();
    let config_dir = dir.path().join(".claw");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        config_dir.join("config.toml"),
        r#"
model = "project/model"
blocked_commands = ["rm -rf /"]
"#,
    )
    .unwrap();

    let config = ConfigLoader::new().project_dir(dir.path()).load();
    assert_eq!(config.model, "project/model");
    assert_eq!(config.blocked_commands, vec!["rm -rf /"]);
    // Other fields remain default
    assert!(config.base_url.contains("openrouter"));
}

#[test]
fn test_ch14_loader_no_config_files() {
    let dir = tempfile::tempdir().unwrap();
    let config = ConfigLoader::new().project_dir(dir.path()).load();
    // All defaults
    assert!(config.model.contains("claude"));
    assert_eq!(config.max_context_tokens, 200_000);
}

// ---------------------------------------------------------------------------
// CostTracker
// ---------------------------------------------------------------------------

#[test]
fn test_ch14_cost_tracker_empty() {
    let tracker = CostTracker::new(3.0, 15.0);
    assert_eq!(tracker.total_input_tokens(), 0);
    assert_eq!(tracker.total_output_tokens(), 0);
    assert_eq!(tracker.turn_count(), 0);
    assert_eq!(tracker.total_cost(), 0.0);
}

#[test]
fn test_ch14_cost_tracker_single_turn() {
    let mut tracker = CostTracker::new(3.0, 15.0);
    tracker.record(&TokenUsage {
        input_tokens: 1000,
        output_tokens: 500,
        ..Default::default()
    });
    assert_eq!(tracker.total_input_tokens(), 1000);
    assert_eq!(tracker.total_output_tokens(), 500);
    assert_eq!(tracker.turn_count(), 1);
}

#[test]
fn test_ch14_cost_tracker_accumulation() {
    let mut tracker = CostTracker::new(3.0, 15.0);
    tracker.record(&TokenUsage {
        input_tokens: 1000,
        output_tokens: 200,
        ..Default::default()
    });
    tracker.record(&TokenUsage {
        input_tokens: 2000,
        output_tokens: 300,
        ..Default::default()
    });
    assert_eq!(tracker.total_input_tokens(), 3000);
    assert_eq!(tracker.total_output_tokens(), 500);
    assert_eq!(tracker.turn_count(), 2);
}

#[test]
fn test_ch14_cost_calculation() {
    let mut tracker = CostTracker::new(3.0, 15.0);
    // 1M input tokens at $3/M = $3.00
    // 1M output tokens at $15/M = $15.00
    tracker.record(&TokenUsage {
        input_tokens: 1_000_000,
        output_tokens: 1_000_000,
        ..Default::default()
    });
    let cost = tracker.total_cost();
    assert!((cost - 18.0).abs() < 0.001);
}

#[test]
fn test_ch14_cost_small_numbers() {
    let mut tracker = CostTracker::new(3.0, 15.0);
    tracker.record(&TokenUsage {
        input_tokens: 100,
        output_tokens: 50,
        ..Default::default()
    });
    // 100 * 3.0 / 1M + 50 * 15.0 / 1M = 0.0003 + 0.00075 = 0.00105
    let cost = tracker.total_cost();
    assert!((cost - 0.00105).abs() < 0.00001);
}

#[test]
fn test_ch14_cost_summary_format() {
    let mut tracker = CostTracker::new(3.0, 15.0);
    tracker.record(&TokenUsage {
        input_tokens: 5000,
        output_tokens: 1000,
        ..Default::default()
    });
    let summary = tracker.summary();
    assert!(summary.contains("5000 in"));
    assert!(summary.contains("1000 out"));
    assert!(summary.contains("$"));
}

#[test]
fn test_ch14_cost_tracker_reset() {
    let mut tracker = CostTracker::new(3.0, 15.0);
    tracker.record(&TokenUsage {
        input_tokens: 1000,
        output_tokens: 500,
        ..Default::default()
    });
    assert_eq!(tracker.turn_count(), 1);

    tracker.reset();
    assert_eq!(tracker.total_input_tokens(), 0);
    assert_eq!(tracker.total_output_tokens(), 0);
    assert_eq!(tracker.turn_count(), 0);
    assert_eq!(tracker.total_cost(), 0.0);
}

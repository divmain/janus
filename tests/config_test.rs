#[path = "common/mod.rs"]
mod common;

use common::JanusTest;
use std::fs;

// ============================================================================
// Config command tests
// ============================================================================

#[test]
fn test_config_show_empty() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["config", "show"]);
    assert!(output.contains("Configuration"));
    assert!(output.contains("not configured"));
}

#[test]
fn test_config_set_default_remote() {
    let janus = JanusTest::new();

    janus.run_success(&["config", "set", "default.remote", "github:myorg/myrepo"]);
    let output = janus.run_success(&["config", "show"]);
    assert!(output.contains("github"));
    assert!(output.contains("myorg"));
}

#[test]
fn test_config_set_linear_default_remote() {
    let janus = JanusTest::new();

    janus.run_success(&["config", "set", "default.remote", "linear:myorg"]);
    let output = janus.run_success(&["config", "show"]);
    assert!(output.contains("linear"));
    assert!(output.contains("myorg"));
}

#[test]
fn test_config_get_not_set() {
    let janus = JanusTest::new();

    let stderr = janus.run_failure(&["config", "get", "github.token"]);
    assert!(stderr.contains("not set"));
}

#[test]
fn test_config_set_invalid_key() {
    let janus = JanusTest::new();

    let stderr = janus.run_failure(&["config", "set", "invalid.key", "value"]);
    assert!(stderr.contains("unknown config key"));
}

#[test]
fn test_config_set_invalid_default_remote_format() {
    let janus = JanusTest::new();

    let stderr = janus.run_failure(&["config", "set", "default.remote", "invalid"]);
    assert!(stderr.contains("invalid") || stderr.contains("format"));
}

#[test]
fn test_config_file_created() {
    let janus = JanusTest::new();

    janus.run_success(&["config", "set", "default.remote", "github:owner/repo"]);

    let config_path = janus.temp_dir.path().join(".janus").join("config.yaml");
    assert!(config_path.exists(), "Config file should be created");

    let content = fs::read_to_string(config_path).unwrap();
    assert!(content.contains("github"));
    assert!(content.contains("owner"));
}

#[test]
fn test_config_rejects_unknown_keys() {
    let janus = JanusTest::new();

    // Test that invalid/unknown keys are rejected
    let stderr = janus.run_failure(&["config", "set", "invalid.key", "value"]);
    assert!(stderr.contains("unknown config key"));
    assert!(stderr.contains("Valid keys"));

    // Test with underscore notation (no longer converted, just unknown)
    let stderr = janus.run_failure(&["config", "set", "default_remote", "github:myorg/myrepo"]);
    assert!(stderr.contains("unknown config key"));

    // Test with linear_api_key
    let stderr = janus.run_failure(&["config", "set", "linear_api_key", "some_key"]);
    assert!(stderr.contains("unknown config key"));

    // Test with github_token
    let stderr = janus.run_failure(&["config", "set", "github_token", "some_token"]);
    assert!(stderr.contains("unknown config key"));
}

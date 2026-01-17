#[path = "common/mod.rs"]
mod common;

use common::JanusTest;
use serial_test::serial;
use std::fs;

// ============================================================================
// Config command tests
// ============================================================================

#[test]
#[serial]
fn test_config_show_empty() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["config", "show"]);
    assert!(output.contains("Configuration"));
    assert!(output.contains("not configured"));
}

#[test]
#[serial]
fn test_config_set_default_remote() {
    let janus = JanusTest::new();

    janus.run_success(&["config", "set", "default_remote", "github:myorg/myrepo"]);
    let output = janus.run_success(&["config", "show"]);
    assert!(output.contains("github"));
    assert!(output.contains("myorg"));
}

#[test]
#[serial]
fn test_config_set_linear_default_remote() {
    let janus = JanusTest::new();

    janus.run_success(&["config", "set", "default_remote", "linear:myorg"]);
    let output = janus.run_success(&["config", "show"]);
    assert!(output.contains("linear"));
    assert!(output.contains("myorg"));
}

#[test]
#[serial]
fn test_config_get_not_set() {
    let janus = JanusTest::new();

    let stderr = janus.run_failure(&["config", "get", "github.token"]);
    assert!(stderr.contains("not set"));
}

#[test]
#[serial]
fn test_config_set_invalid_key() {
    let janus = JanusTest::new();

    let stderr = janus.run_failure(&["config", "set", "invalid.key", "value"]);
    assert!(stderr.contains("unknown config key"));
}

#[test]
#[serial]
fn test_config_set_invalid_default_remote_format() {
    let janus = JanusTest::new();

    let stderr = janus.run_failure(&["config", "set", "default_remote", "invalid"]);
    assert!(stderr.contains("invalid") || stderr.contains("format"));
}

#[test]
#[serial]
fn test_config_file_created() {
    let janus = JanusTest::new();

    janus.run_success(&["config", "set", "default_remote", "github:owner/repo"]);

    let config_path = janus.temp_dir.path().join(".janus").join("config.yaml");
    assert!(config_path.exists(), "Config file should be created");

    let content = fs::read_to_string(config_path).unwrap();
    assert!(content.contains("github"));
    assert!(content.contains("owner"));
}

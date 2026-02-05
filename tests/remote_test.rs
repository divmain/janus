#[path = "common/mod.rs"]
mod common;

use common::JanusTest;
use serial_test::serial;

// ============================================================================
// Remote sync command tests (without actual API calls)
// ============================================================================

#[test]
#[serial]
fn test_adopt_invalid_ref() {
    let janus = JanusTest::new();

    let stderr = janus.run_failure(&["remote", "adopt", "invalid"]);
    assert!(stderr.contains("invalid") || stderr.contains("expected"));
}

#[test]
#[serial]
fn test_adopt_with_reserved_prefix_fails() {
    let janus = JanusTest::new();

    let stderr = janus.run_failure(&[
        "remote",
        "adopt",
        "github:test/test/123",
        "--prefix",
        "plan",
    ]);
    assert!(
        stderr.contains("reserved"),
        "Error should mention the prefix is reserved, got: {stderr}"
    );
}

#[test]
#[serial]
fn test_adopt_with_invalid_prefix_characters_fails() {
    let janus = JanusTest::new();

    let stderr = janus.run_failure(&[
        "remote",
        "adopt",
        "github:test/test/123",
        "--prefix",
        "invalid/prefix",
    ]);
    assert!(
        stderr.contains("invalid characters"),
        "Error should mention invalid characters, got: {stderr}"
    );
}

#[test]
#[serial]
fn test_push_not_configured() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    let stderr = janus.run_failure(&["remote", "push", &id]);
    // Should fail due to no default.remote config
    assert!(
        stderr.contains("not configured") || stderr.contains("default.remote"),
        "Should fail due to missing config: {stderr}"
    );
}

#[test]
#[serial]
fn test_remote_link_invalid_ref() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    let stderr = janus.run_failure(&["remote", "link", &id, "invalid"]);
    assert!(stderr.contains("invalid") || stderr.contains("expected"));
}

#[test]
#[serial]
fn test_sync_not_linked() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    let stderr = janus.run_failure(&["remote", "sync", &id]);
    assert!(stderr.contains("not linked"));
}

#[test]
#[serial]
fn test_help_shows_new_commands() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["--help"]);
    assert!(output.contains("remote"), "Should show remote command");
    assert!(output.contains("config"), "Should show config command");
}

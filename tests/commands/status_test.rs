#[path = "../common/mod.rs"]
mod common;
use common::JanusTest;
use serial_test::serial;

// ============================================================================
// Status command tests
// ============================================================================

#[test]
#[serial]
fn test_status_start() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    janus.run_success(&["status", &id, "complete"]);

    let output = janus.run_success(&["show", &id]);
    assert!(output.contains("status: complete"));

    janus.run_success(&["start", &id]);
    let output = janus.run_success(&["show", &id]);
    assert!(output.contains("status: in_progress"));
}

#[test]
#[serial]
fn test_status_close_no_summary() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    janus.run_success(&["close", &id, "--no-summary"]);

    let output = janus.run_success(&["show", &id]);
    assert!(output.contains("status: complete"));
    // Should not contain completion summary section
    assert!(!output.contains("## Completion Summary"));
}

#[test]
#[serial]
fn test_status_close_with_summary() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    janus.run_success(&["close", &id, "--summary", "Fixed the bug successfully"]);

    let output = janus.run_success(&["show", &id]);
    assert!(output.contains("status: complete"));
    assert!(output.contains("## Completion Summary"));
    assert!(output.contains("Fixed the bug successfully"));
}

#[test]
#[serial]
fn test_status_close_requires_summary_flag() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();

    // Should fail without --summary or --no-summary
    let stderr = janus.run_failure(&["close", &id]);
    assert!(stderr.contains("--summary") || stderr.contains("--no-summary"));
}

#[test]
#[serial]
fn test_status_close_summary_and_no_summary_conflict() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();

    // Should fail when both are specified
    let stderr = janus.run_failure(&["close", &id, "--summary", "Test", "--no-summary"]);
    assert!(
        stderr.contains("cannot be used with") || stderr.contains("conflicts"),
        "Expected conflict error, got: {}",
        stderr
    );
}

#[test]
#[serial]
fn test_status_reopen() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    janus.run_success(&["close", &id, "--no-summary"]);
    janus.run_success(&["reopen", &id]);

    let output = janus.run_success(&["show", &id]);
    assert!(output.contains("status: new"));
}

#[test]
#[serial]
fn test_status_cancelled() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    janus.run_success(&["status", &id, "cancelled"]);

    let output = janus.run_success(&["show", &id]);
    assert!(output.contains("status: cancelled"));
}

#[test]
#[serial]
fn test_status_next() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    janus.run_success(&["status", &id, "next"]);

    let output = janus.run_success(&["show", &id]);
    assert!(output.contains("status: next"));
}

#[test]
#[serial]
fn test_status_in_progress() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    janus.run_success(&["status", &id, "in_progress"]);

    let output = janus.run_success(&["show", &id]);
    assert!(output.contains("status: in_progress"));
}

#[test]
#[serial]
fn test_start_sets_in_progress() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    janus.run_success(&["start", &id]);

    let output = janus.run_success(&["show", &id]);
    assert!(output.contains("status: in_progress"));
}

#[test]
#[serial]
fn test_status_invalid() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    let stderr = janus.run_failure(&["status", &id, "invalid"]);
    assert!(stderr.contains("Invalid status"));
}

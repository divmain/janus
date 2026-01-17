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

    let content = janus.read_ticket(&id);
    assert!(content.contains("status: complete"));

    janus.run_success(&["start", &id]);
    let content = janus.read_ticket(&id);
    assert!(content.contains("status: in_progress"));
}

#[test]
#[serial]
fn test_status_close() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    janus.run_success(&["close", &id]);

    let content = janus.read_ticket(&id);
    assert!(content.contains("status: complete"));
}

#[test]
#[serial]
fn test_status_reopen() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    janus.run_success(&["close", &id]);
    janus.run_success(&["reopen", &id]);

    let content = janus.read_ticket(&id);
    assert!(content.contains("status: new"));
}

#[test]
#[serial]
fn test_status_cancelled() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    janus.run_success(&["status", &id, "cancelled"]);

    let content = janus.read_ticket(&id);
    assert!(content.contains("status: cancelled"));
}

#[test]
#[serial]
fn test_status_next() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    janus.run_success(&["status", &id, "next"]);

    let content = janus.read_ticket(&id);
    assert!(content.contains("status: next"));
}

#[test]
#[serial]
fn test_status_in_progress() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    janus.run_success(&["status", &id, "in_progress"]);

    let content = janus.read_ticket(&id);
    assert!(content.contains("status: in_progress"));
}

#[test]
#[serial]
fn test_start_sets_in_progress() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    janus.run_success(&["start", &id]);

    let content = janus.read_ticket(&id);
    assert!(content.contains("status: in_progress"));
}

#[test]
#[serial]
fn test_status_invalid() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    let stderr = janus.run_failure(&["status", &id, "invalid"]);
    assert!(stderr.contains("Invalid status"));
}

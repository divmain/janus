#[path = "../common/mod.rs"]
mod common;
use common::JanusTest;

// ============================================================================
// Create command tests
// ============================================================================

// Note: Duplicate test functions from lines 12-145 have been removed.
// These tests are exact duplicates of tests in tests/integration_test.rs
// and were causing ~16 duplicate assertions.

#[test]
fn test_create_with_hyphen_prefix() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["create", "Test ticket", "--prefix", "my-prefix"]);
    let id = output.trim();

    assert!(
        id.starts_with("my-prefix-"),
        "ID should start with 'my-prefix-'"
    );
    assert!(janus.ticket_exists(id), "Ticket file should exist");
}

#[test]
fn test_create_with_underscore_prefix() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["create", "Test ticket", "--prefix", "my_prefix"]);
    let id = output.trim();

    assert!(
        id.starts_with("my_prefix-"),
        "ID should start with 'my_prefix-'"
    );
    assert!(janus.ticket_exists(id), "Ticket file should exist");
}

#[test]
fn test_create_multiple_tickets_same_prefix() {
    let janus = JanusTest::new();

    let output1 = janus.run_success(&["create", "Ticket 1", "--prefix", "perf"]);
    let output2 = janus.run_success(&["create", "Ticket 2", "--prefix", "perf"]);
    let id1 = output1.trim();
    let id2 = output2.trim();

    assert!(id1.starts_with("perf-"), "ID1 should start with 'perf-'");
    assert!(id2.starts_with("perf-"), "ID2 should start with 'perf-'");
    assert_ne!(id1, id2, "IDs should be unique even with same prefix");
    assert!(janus.ticket_exists(id1), "Ticket1 should exist");
    assert!(janus.ticket_exists(id2), "Ticket2 should exist");
}

#[test]
fn test_create_tickets_different_prefixes() {
    let janus = JanusTest::new();

    let output1 = janus.run_success(&["create", "Bug fix", "--prefix", "bug"]);
    let output2 = janus.run_success(&["create", "Feature", "--prefix", "feat"]);
    let output3 = janus.run_success(&["create", "Task"]);
    let id1 = output1.trim();
    let id2 = output2.trim();
    let id3 = output3.trim();

    assert!(id1.starts_with("bug-"), "ID1 should start with 'bug-'");
    assert!(id2.starts_with("feat-"), "ID2 should start with 'feat-'");
    assert!(!id3.starts_with("bug-"), "ID3 should not start with 'bug-'");
    assert!(
        !id3.starts_with("feat-"),
        "ID3 should not start with 'feat-'"
    );
}

#[test]
fn test_create_with_reserved_prefix_fails() {
    let janus = JanusTest::new();

    let stderr = janus.run_failure(&["create", "Test ticket", "--prefix", "plan"]);
    assert!(
        stderr.contains("reserved"),
        "Error should mention the prefix is reserved"
    );
    assert!(
        stderr.contains("plan"),
        "Error should mention the prefix 'plan'"
    );
}

#[test]
fn test_create_with_invalid_prefix_characters_fails() {
    let janus = JanusTest::new();

    let invalid_prefixes = vec![
        ("invalid/prefix", "invalid characters"),
        ("invalid@prefix", "invalid characters"),
        ("invalid prefix", "invalid characters"),
        ("invalid.prefix", "invalid characters"),
    ];

    for (prefix, expected_error) in invalid_prefixes {
        let stderr = janus.run_failure(&["create", "Test ticket", "--prefix", prefix]);
        assert!(
            stderr.contains(expected_error),
            "Error for prefix '{prefix}' should contain '{expected_error}'"
        );
        assert!(
            stderr.contains(prefix),
            "Error should mention the invalid prefix '{prefix}'"
        );
    }
}

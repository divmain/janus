#[path = "../common/mod.rs"]
mod common;
use common::JanusTest;
use serial_test::serial;

// ============================================================================
// Create command tests
// ============================================================================

#[test]
#[serial]
fn test_create_basic() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["create", "Test ticket"]);
    let id = output.trim();

    assert!(!id.is_empty(), "Should output a ticket ID");
    assert!(id.contains('-'), "ID should contain a dash");
    assert!(janus.ticket_exists(id), "Ticket file should exist");

    let content = janus.read_ticket(id);
    assert!(content.contains("# Test ticket"));
    assert!(content.contains("status: new"));
    assert!(content.contains("deps: []"));
    assert!(content.contains("links: []"));
    assert!(content.contains("type: task"));
    assert!(content.contains("priority: 2"));
}

#[test]
#[serial]
fn test_create_with_options() {
    let janus = JanusTest::new();

    let output = janus.run_success(&[
        "create",
        "Bug ticket",
        "-d",
        "This is a description",
        "-p",
        "0",
        "-t",
        "bug",
        "--external-ref",
        "gh-123",
    ]);
    let id = output.trim();

    let content = janus.read_ticket(id);
    assert!(content.contains("# Bug ticket"));
    assert!(content.contains("This is a description"));
    assert!(content.contains("priority: 0"));
    assert!(content.contains("type: bug"));
    assert!(content.contains("external-ref: gh-123"));
}

#[test]
#[serial]
fn test_create_with_parent() {
    let janus = JanusTest::new();

    let parent_id = janus
        .run_success(&["create", "Parent ticket"])
        .trim()
        .to_string();
    let child_id = janus
        .run_success(&["create", "Child ticket", "--parent", &parent_id])
        .trim()
        .to_string();

    let child_content = janus.read_ticket(&child_id);
    assert!(child_content.contains(&format!("parent: {}", parent_id)));
}

#[test]
#[serial]
fn test_create_all_types() {
    let janus = JanusTest::new();

    for ticket_type in &["bug", "feature", "task", "epic", "chore"] {
        let output = janus.run_success(&["create", "Test", "-t", ticket_type]);
        let id = output.trim();
        let content = janus.read_ticket(id);
        assert!(
            content.contains(&format!("type: {}", ticket_type)),
            "Type should be {}",
            ticket_type
        );
    }
}

#[test]
#[serial]
fn test_create_all_priorities() {
    let janus = JanusTest::new();

    for priority in &["0", "1", "2", "3", "4"] {
        let output = janus.run_success(&["create", "Test", "-p", priority]);
        let id = output.trim();
        let content = janus.read_ticket(id);
        assert!(
            content.contains(&format!("priority: {}", priority)),
            "Priority should be {}",
            priority
        );
    }
}

#[test]
#[serial]
fn test_create_invalid_priority() {
    let janus = JanusTest::new();
    let stderr = janus.run_failure(&["create", "Test", "-p", "5"]);
    assert!(stderr.contains("Invalid priority"));
}

#[test]
#[serial]
fn test_create_invalid_type() {
    let janus = JanusTest::new();
    let stderr = janus.run_failure(&["create", "Test", "-t", "invalid"]);
    assert!(stderr.contains("Invalid type"));
}

#[test]
#[serial]
fn test_create_with_custom_prefix() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["create", "Test ticket", "--prefix", "perf"]);
    let id = output.trim();

    assert!(id.starts_with("perf-"), "ID should start with 'perf-'");
    assert!(janus.ticket_exists(id), "Ticket file should exist");

    let content = janus.read_ticket(id);
    assert!(content.contains("# Test ticket"));
    assert!(content.contains("uuid:"), "Ticket should have a UUID");
}

#[test]
#[serial]
fn test_create_with_empty_uses_default() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["create", "Test ticket", "--prefix", ""]);
    let id = output.trim();

    assert!(!id.is_empty(), "Should output a ticket ID");
    assert!(id.contains('-'), "ID should contain a dash");
    assert!(janus.ticket_exists(id), "Ticket file should exist");
}

#[test]
#[serial]
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
#[serial]
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
#[serial]
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
#[serial]
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
#[serial]
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
#[serial]
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
            "Error for prefix '{}' should contain '{}'",
            prefix,
            expected_error
        );
        assert!(
            stderr.contains(prefix),
            "Error should mention the invalid prefix '{}'",
            prefix
        );
    }
}

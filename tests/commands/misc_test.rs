#[path = "../common/mod.rs"]
mod common;
use common::JanusTest;
use serial_test::serial;

// ============================================================================
// Add-note command tests
// ============================================================================

#[test]
#[serial]
fn test_add_note() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    janus.run_success(&["add-note", &id, "This is a note"]);

    let content = janus.read_ticket(&id);
    assert!(content.contains("## Notes"));
    assert!(content.contains("This is a note"));
    // Should have a timestamp
    assert!(content.contains("**20")); // Year prefix
}

#[test]
#[serial]
fn test_add_note_multiple() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    janus.run_success(&["add-note", &id, "Note 1"]);
    janus.run_success(&["add-note", &id, "Note 2"]);

    let content = janus.read_ticket(&id);
    assert!(content.contains("Note 1"));
    assert!(content.contains("Note 2"));
}

// ============================================================================
// Edit command tests
// ============================================================================

#[test]
#[serial]
fn test_edit_non_tty() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    // In non-TTY mode (like tests), it should fail with an error message
    let stderr = janus.run_failure(&["edit", &id]);
    assert!(stderr.contains("Cannot open editor in non-interactive mode"));
    assert!(stderr.contains(&id));
    assert!(stderr.contains(".janus"));
}

// ============================================================================
// Query command tests
// ============================================================================

#[test]
#[serial]
fn test_query_basic() {
    let janus = JanusTest::new();

    let id = janus
        .run_success(&["create", "Test ticket"])
        .trim()
        .to_string();

    let output = janus.run_success(&["query"]);
    assert!(output.contains(&id));
    assert!(output.contains("Test ticket"));
    assert!(output.contains("\"status\":\"new\""));
}

#[test]
#[serial]
fn test_query_json_format() {
    let janus = JanusTest::new();

    janus.run_success(&["create", "Test"]);

    let output = janus.run_success(&["query"]);

    // Should be valid JSON on each line
    for line in output.lines() {
        if !line.trim().is_empty() {
            let _: serde_json::Value =
                serde_json::from_str(line).expect("Output should be valid JSON");
        }
    }
}

#[test]
#[serial]
fn test_query_includes_children_count() {
    let janus = JanusTest::new();

    // Create a parent ticket
    let parent_id = janus.run_success(&["create", "Parent"]).trim().to_string();

    // Create 2 child tickets spawned from parent
    janus.run_success(&["create", "Child 1", "--spawned-from", &parent_id]);
    janus.run_success(&["create", "Child 2", "--spawned-from", &parent_id]);

    // Query all tickets and find the parent
    let output = janus.run_success(&["query"]);

    // Find the parent ticket line and verify children_count
    for line in output.lines() {
        if !line.trim().is_empty() {
            let json: serde_json::Value =
                serde_json::from_str(line).expect("Output should be valid JSON");
            if json["id"] == parent_id {
                assert_eq!(
                    json["children_count"], 2,
                    "Expected children_count to be 2 for parent ticket:\n{line}"
                );
            }
        }
    }
}

#[test]
#[serial]
fn test_query_children_count_zero_for_leaf_tickets() {
    let janus = JanusTest::new();

    // Create a ticket with no spawned children
    let id = janus
        .run_success(&["create", "Leaf ticket"])
        .trim()
        .to_string();

    // Query all tickets and find the ticket
    let output = janus.run_success(&["query"]);

    // Find the ticket line and verify children_count is 0
    for line in output.lines() {
        if !line.trim().is_empty() {
            let json: serde_json::Value =
                serde_json::from_str(line).expect("Output should be valid JSON");
            if json["id"] == id {
                assert_eq!(
                    json["children_count"], 0,
                    "Expected children_count to be 0 for leaf ticket:\n{line}"
                );
            }
        }
    }
}

// ============================================================================
// Error handling tests
// ============================================================================

#[test]
#[serial]
fn test_ticket_not_found() {
    let janus = JanusTest::new();

    let output = janus.run(&["show", "nonexistent"]);
    assert!(
        !output.status.success(),
        "Should fail for nonexistent resource"
    );
}

#[test]
#[serial]
fn test_ambiguous_id() {
    let janus = JanusTest::new();

    // Create two tickets - they'll have the same prefix
    let id1 = janus
        .run_success(&["create", "Ticket 1"])
        .trim()
        .to_string();
    let id2 = janus
        .run_success(&["create", "Ticket 2"])
        .trim()
        .to_string();

    // Get the common prefix (before the hash)
    let prefix = id1.split('-').next().unwrap();

    // If both tickets share the prefix, this should be ambiguous
    if id2.starts_with(prefix) && id1.split('-').next_back() != id2.split('-').next_back() {
        let stderr = janus.run_failure(&["show", prefix]);
        assert!(stderr.contains("ambiguous") || stderr.contains("multiple"));
    }
}

#[test]
#[serial]
fn test_dep_add_nonexistent() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    let stderr = janus.run_failure(&["dep", "add", &id, "nonexistent"]);
    assert!(
        stderr.contains("not found")
            || stderr.contains("not_found")
            || stderr.contains("unknown")
            || stderr.contains("does not exist"),
        "Error should indicate resource was not found"
    );
}

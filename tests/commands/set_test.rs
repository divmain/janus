#[path = "../common/mod.rs"]
mod common;
use common::JanusTest;
use serial_test::serial;

// ============================================================================
// Set command tests
// ============================================================================

#[test]
#[serial]
fn test_set_priority() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();

    // Default priority is 2, change to 0
    let output = janus.run_success(&["set", &id, "priority", "0"]);
    assert!(output.contains("Updated"));
    assert!(output.contains("priority"));

    let content = janus.read_ticket(&id);
    assert!(content.contains("priority: 0"));
}

#[test]
#[serial]
fn test_set_priority_all_values() {
    let janus = JanusTest::new();

    for priority in &["0", "1", "2", "3", "4"] {
        let id = janus.run_success(&["create", "Test"]).trim().to_string();
        janus.run_success(&["set", &id, "priority", priority]);

        let content = janus.read_ticket(&id);
        assert!(
            content.contains(&format!("priority: {}", priority)),
            "Priority should be set to {}",
            priority
        );
    }
}

#[test]
#[serial]
fn test_set_priority_invalid() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    let stderr = janus.run_failure(&["set", &id, "priority", "5"]);
    assert!(stderr.contains("invalid value"));
    assert!(stderr.contains("priority"));
}

#[test]
#[serial]
fn test_set_type() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();

    // Default type is task, change to bug
    let output = janus.run_success(&["set", &id, "type", "bug"]);
    assert!(output.contains("Updated"));
    assert!(output.contains("type"));

    let content = janus.read_ticket(&id);
    assert!(content.contains("type: bug"));
}

#[test]
#[serial]
fn test_set_type_all_values() {
    let janus = JanusTest::new();

    for ticket_type in &["bug", "feature", "task", "epic", "chore"] {
        let id = janus.run_success(&["create", "Test"]).trim().to_string();
        janus.run_success(&["set", &id, "type", ticket_type]);

        let content = janus.read_ticket(&id);
        assert!(
            content.contains(&format!("type: {}", ticket_type)),
            "Type should be set to {}",
            ticket_type
        );
    }
}

#[test]
#[serial]
fn test_set_type_invalid() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    let stderr = janus.run_failure(&["set", &id, "type", "invalid"]);
    assert!(stderr.contains("invalid value"));
    assert!(stderr.contains("type"));
}

#[test]
#[serial]
fn test_set_parent() {
    let janus = JanusTest::new();

    let parent_id = janus
        .run_success(&["create", "Parent ticket"])
        .trim()
        .to_string();
    let child_id = janus
        .run_success(&["create", "Child ticket"])
        .trim()
        .to_string();

    // Set parent
    let output = janus.run_success(&["set", &child_id, "parent", &parent_id]);
    assert!(output.contains("Updated"));
    assert!(output.contains("parent"));

    let content = janus.read_ticket(&child_id);
    assert!(content.contains(&format!("parent: {}", parent_id)));
}

#[test]
#[serial]
fn test_set_parent_clear() {
    let janus = JanusTest::new();

    let parent_id = janus
        .run_success(&["create", "Parent ticket"])
        .trim()
        .to_string();
    let child_id = janus
        .run_success(&["create", "Child ticket", "--parent", &parent_id])
        .trim()
        .to_string();

    // Verify parent is set
    let content = janus.read_ticket(&child_id);
    assert!(content.contains(&format!("parent: {}", parent_id)));

    // Clear parent with empty string
    let output = janus.run_success(&["set", &child_id, "parent", ""]);
    assert!(output.contains("Updated"));

    let content = janus.read_ticket(&child_id);
    assert!(!content.contains("parent:"));
}

#[test]
#[serial]
fn test_set_parent_nonexistent() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    let stderr = janus.run_failure(&["set", &id, "parent", "nonexistent"]);
    assert!(stderr.contains("not found"));
}

#[test]
#[serial]
fn test_set_invalid_field() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    let stderr = janus.run_failure(&["set", &id, "invalid_field", "value"]);
    assert!(stderr.contains("invalid field"));
    assert!(stderr.contains("must be one of"));
}

#[test]
#[serial]
fn test_set_json_output() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    let output = janus.run_success(&["set", &id, "priority", "1", "--json"]);

    // Verify JSON output
    let json: serde_json::Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert_eq!(json["action"], "field_updated");
    assert_eq!(json["field"], "priority");
    assert_eq!(json["new_value"], "1");
    assert_eq!(json["id"], id);
}

#[test]
#[serial]
fn test_set_ticket_not_found() {
    let janus = JanusTest::new();

    let stderr = janus.run_failure(&["set", "nonexistent", "priority", "1"]);
    assert!(stderr.contains("not found"));
}

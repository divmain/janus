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

    let output = janus.run_success(&["show", &id, "--json"]);
    let json: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["priority"], 0);
}

#[test]
#[serial]
fn test_set_priority_all_values() {
    let janus = JanusTest::new();

    for priority in &["0", "1", "2", "3", "4"] {
        let id = janus.run_success(&["create", "Test"]).trim().to_string();
        janus.run_success(&["set", &id, "priority", priority]);

        let output = janus.run_success(&["show", &id, "--json"]);
        let json: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(
            json["priority"],
            priority.parse::<u8>().unwrap(),
            "Priority should be set to {priority}"
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

    let output = janus.run_success(&["show", &id, "--json"]);
    let json: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["type"], "bug");
}

#[test]
#[serial]
fn test_set_type_all_values() {
    let janus = JanusTest::new();

    for ticket_type in &["bug", "feature", "task", "epic", "chore"] {
        let id = janus.run_success(&["create", "Test"]).trim().to_string();
        janus.run_success(&["set", &id, "type", ticket_type]);

        let output = janus.run_success(&["show", &id, "--json"]);
        let json: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(
            json["type"], *ticket_type,
            "Type should be set to {ticket_type}"
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

    // Verify parent is set using show command
    let output = janus.run_success(&["show", &child_id]);
    assert!(output.contains(&parent_id));
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

    // Verify parent is set using show command
    let output = janus.run_success(&["show", &child_id]);
    assert!(output.contains(&parent_id));

    // Clear parent by omitting the value argument
    let output = janus.run_success(&["set", &child_id, "parent"]);
    assert!(output.contains("Updated"));

    let output = janus.run_success(&["show", &child_id]);
    assert!(!output.contains(&parent_id));
}

#[test]
#[serial]
fn test_set_parent_nonexistent() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    let output = janus.run(&["set", &id, "parent", "nonexistent"]);
    assert!(
        !output.status.success(),
        "Should fail for nonexistent parent"
    );
}

#[test]
#[serial]
fn test_set_invalid_field() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    let stderr = janus.run_failure(&["set", &id, "invalid_field", "value"]);
    assert!(stderr.contains("invalid field"));
    assert!(stderr.contains("Must be one of"));
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

    let output = janus.run(&["set", "nonexistent", "priority", "1"]);
    assert!(
        !output.status.success(),
        "Should fail for nonexistent ticket"
    );
}

#[test]
#[serial]
fn test_set_design() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();

    // Set design section
    let output = janus.run_success(&["set", &id, "design", "This is the design content"]);
    assert!(output.contains("Updated"));
    assert!(output.contains("design"));

    // Verify design is set using show command
    let output = janus.run_success(&["show", &id]);
    assert!(output.contains("This is the design content"));
}

#[test]
#[serial]
fn test_set_design_clear() {
    let janus = JanusTest::new();

    let id = janus
        .run_success(&["create", "Test", "--design", "Initial design"])
        .trim()
        .to_string();

    // Verify design is set
    let output = janus.run_success(&["show", &id]);
    assert!(output.contains("Initial design"));

    // Clear design by omitting the value argument
    let output = janus.run_success(&["set", &id, "design"]);
    assert!(output.contains("Updated"));

    // Verify design is cleared
    let output = janus.run_success(&["show", &id, "--json"]);
    let json: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert!(
        json["body"]
            .as_str()
            .map(|b| !b.contains("Initial design"))
            .unwrap_or(true),
        "Design should be cleared"
    );
}

#[test]
#[serial]
fn test_set_acceptance() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();

    // Set acceptance criteria section
    let output = janus.run_success(&["set", &id, "acceptance", "User can log in and log out"]);
    assert!(output.contains("Updated"));
    assert!(output.contains("acceptance"));

    // Verify acceptance is set using show command
    let output = janus.run_success(&["show", &id]);
    assert!(output.contains("User can log in and log out"));
}

#[test]
#[serial]
fn test_set_acceptance_clear() {
    let janus = JanusTest::new();

    let id = janus
        .run_success(&["create", "Test", "--acceptance", "Initial criteria"])
        .trim()
        .to_string();

    // Verify acceptance is set
    let output = janus.run_success(&["show", &id]);
    assert!(output.contains("Initial criteria"));

    // Clear acceptance by omitting the value argument
    let output = janus.run_success(&["set", &id, "acceptance"]);
    assert!(output.contains("Updated"));

    // Verify acceptance is cleared
    let output = janus.run_success(&["show", &id, "--json"]);
    let json: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert!(
        json["body"]
            .as_str()
            .map(|b| !b.contains("Initial criteria"))
            .unwrap_or(true),
        "Acceptance should be cleared"
    );
}

#[test]
#[serial]
fn test_set_description() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();

    // Set description (main body content)
    let output = janus.run_success(&["set", &id, "description", "This is the description"]);
    assert!(output.contains("Updated"));
    assert!(output.contains("description"));

    // Verify description is set using show command
    let output = janus.run_success(&["show", &id]);
    assert!(output.contains("This is the description"));
}

#[test]
#[serial]
fn test_set_description_clear() {
    let janus = JanusTest::new();

    let id = janus
        .run_success(&["create", "Test", "--description", "Initial description"])
        .trim()
        .to_string();

    // Verify description is set
    let output = janus.run_success(&["show", &id]);
    assert!(output.contains("Initial description"));

    // Clear description by omitting the value argument
    let output = janus.run_success(&["set", &id, "description"]);
    assert!(output.contains("Updated"));

    // Verify description is cleared
    let output = janus.run_success(&["show", &id, "--json"]);
    let json: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert!(
        json["body"]
            .as_str()
            .map(|b| !b.contains("Initial description"))
            .unwrap_or(true),
        "Description should be cleared"
    );
}

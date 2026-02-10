#[path = "../common/mod.rs"]
mod common;
use common::JanusTest;

// ============================================================================
// Show command tests
// ============================================================================

#[test]
fn test_show_basic() {
    let janus = JanusTest::new();

    let id = janus
        .run_success(&["create", "Test ticket", "-d", "Description"])
        .trim()
        .to_string();
    let output = janus.run_success(&["show", &id]);

    assert!(output.contains("# Test ticket"));
    assert!(output.contains("Description"));
    assert!(output.contains(&format!("id: {id}")));
}

#[test]
fn test_show_partial_id() {
    let janus = JanusTest::new();

    let id = janus
        .run_success(&["create", "Test ticket"])
        .trim()
        .to_string();
    // Use just the hash part (after the dash)
    let partial = id.split('-').next_back().unwrap();
    let output = janus.run_success(&["show", partial]);

    assert!(output.contains("# Test ticket"));
}

#[test]
fn test_show_with_blockers() {
    let janus = JanusTest::new();

    let dep_id = janus
        .run_success(&["create", "Dependency"])
        .trim()
        .to_string();
    let id = janus
        .run_success(&["create", "Main ticket"])
        .trim()
        .to_string();
    janus.run_success(&["dep", "add", &id, &dep_id]);

    let output = janus.run_success(&["show", &id]);
    assert!(output.contains("## Blockers"));
    assert!(output.contains(&dep_id));
}

#[test]
fn test_show_with_blocking() {
    let janus = JanusTest::new();

    let id = janus
        .run_success(&["create", "Main ticket"])
        .trim()
        .to_string();
    let blocked_id = janus
        .run_success(&["create", "Blocked ticket"])
        .trim()
        .to_string();
    janus.run_success(&["dep", "add", &blocked_id, &id]);

    let output = janus.run_success(&["show", &id]);
    assert!(output.contains("## Blocking"));
    assert!(output.contains(&blocked_id));
}

#[test]
fn test_show_with_children() {
    let janus = JanusTest::new();

    let parent_id = janus.run_success(&["create", "Parent"]).trim().to_string();
    let child_id = janus
        .run_success(&["create", "Child", "--parent", &parent_id])
        .trim()
        .to_string();

    let output = janus.run_success(&["show", &parent_id]);
    assert!(output.contains("## Children"));
    assert!(output.contains(&child_id));
}

#[test]
fn test_show_with_links() {
    let janus = JanusTest::new();

    let id1 = janus
        .run_success(&["create", "Ticket 1"])
        .trim()
        .to_string();
    let id2 = janus
        .run_success(&["create", "Ticket 2"])
        .trim()
        .to_string();
    janus.run_success(&["link", "add", &id1, &id2]);

    let output = janus.run_success(&["show", &id1]);
    assert!(output.contains("## Linked"));
    assert!(output.contains(&id2));
}

#[test]
fn test_show_not_found() {
    let janus = JanusTest::new();
    let stderr = janus.run_failure(&["show", "nonexistent"]);
    assert!(
        stderr.contains("not found")
            || stderr.contains("not_found")
            || stderr.contains("unknown")
            || stderr.contains("does not exist"),
        "Error should indicate ticket was not found"
    );
}

#[test]
fn test_show_children_count_displayed() {
    let janus = JanusTest::new();

    // Create a parent ticket
    let parent_id = janus.run_success(&["create", "Parent"]).trim().to_string();

    // Create 3 child tickets spawned from parent
    janus.run_success(&["create", "Child 1", "--spawned-from", &parent_id]);
    janus.run_success(&["create", "Child 2", "--spawned-from", &parent_id]);
    janus.run_success(&["create", "Child 3", "--spawned-from", &parent_id]);

    // Show should display children count
    let output = janus.run_success(&["show", &parent_id]);
    assert!(
        output.contains("3 spawned from this ticket"),
        "Expected '3 spawned from this ticket' in output:\n{output}"
    );
}

#[test]
fn test_show_children_count_not_displayed_when_zero() {
    let janus = JanusTest::new();

    // Create a ticket with no children
    let id = janus
        .run_success(&["create", "Solo ticket"])
        .trim()
        .to_string();

    // Show should NOT display children count when 0
    let output = janus.run_success(&["show", &id]);
    assert!(
        !output.contains("spawned from this ticket"),
        "Expected no children count for ticket with no spawned children:\n{output}"
    );
}

#[test]
fn test_show_children_count_in_json() {
    let janus = JanusTest::new();

    // Create a parent ticket
    let parent_id = janus.run_success(&["create", "Parent"]).trim().to_string();

    // Create 2 child tickets spawned from parent
    janus.run_success(&["create", "Child 1", "--spawned-from", &parent_id]);
    janus.run_success(&["create", "Child 2", "--spawned-from", &parent_id]);

    // Show with JSON output should include children_count
    let output = janus.run_success(&["show", &parent_id, "--json"]);
    let json: serde_json::Value = serde_json::from_str(&output).expect("Invalid JSON");

    assert_eq!(
        json["children_count"], 2,
        "Expected children_count to be 2 in JSON output:\n{output}"
    );
}

use serial_test::serial;

use crate::common::JanusTest;

#[test]
fn test_plan_create_simple() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["plan", "create", "Test Plan"]);
    let id = output.trim();

    assert!(!id.is_empty(), "Should output a plan ID");
    assert!(id.starts_with("plan-"), "ID should start with 'plan-'");
    assert!(janus.plan_exists(id), "Plan file should exist");

    let content = janus.read_plan(id);
    assert!(content.contains("# Test Plan"));
    assert!(content.contains(&format!("id: {}", id)));
    assert!(content.contains("uuid:"));
    assert!(content.contains("created:"));
    // Simple plan should have a Tickets section
    assert!(content.contains("## Tickets"));
}

#[test]
fn test_plan_create_with_phases() {
    let janus = JanusTest::new();

    let output = janus.run_success(&[
        "plan",
        "create",
        "Phased Plan",
        "--phase",
        "Infrastructure",
        "--phase",
        "Implementation",
        "--phase",
        "Testing",
    ]);
    let id = output.trim();

    assert!(janus.plan_exists(id), "Plan file should exist");

    let content = janus.read_plan(id);
    assert!(content.contains("# Phased Plan"));
    assert!(content.contains("## Phase 1: Infrastructure"));
    assert!(content.contains("## Phase 2: Implementation"));
    assert!(content.contains("## Phase 3: Testing"));
    // Phased plan should NOT have a top-level Tickets section
    // (tickets are inside phases)
}

#[test]
fn test_plan_delete() {
    let janus = JanusTest::new();

    // Create a plan
    let plan_id = janus
        .run_success(&["plan", "create", "Plan to Delete"])
        .trim()
        .to_string();

    // Verify plan exists
    assert!(janus.plan_exists(&plan_id));

    // Delete with --force (non-interactive)
    let output = janus.run_success(&["plan", "delete", &plan_id, "--force"]);
    assert!(output.contains("Deleted"));

    // Verify plan is gone
    assert!(!janus.plan_exists(&plan_id));
}

#[test]
fn test_plan_rename() {
    let janus = JanusTest::new();

    // Create a plan
    let plan_id = janus
        .run_success(&["plan", "create", "Original Title"])
        .trim()
        .to_string();

    // Rename it
    let output = janus.run_success(&["plan", "rename", &plan_id, "New Title"]);
    assert!(output.contains("Renamed"));
    assert!(output.contains("Original Title"));
    assert!(output.contains("New Title"));

    // Verify new title
    let content = janus.read_plan(&plan_id);
    assert!(content.contains("# New Title"));
    assert!(!content.contains("# Original Title"));
}

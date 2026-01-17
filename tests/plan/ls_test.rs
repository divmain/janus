use std::fs;

use serial_test::serial;

use crate::common::JanusTest;

#[test]
fn test_plan_ls_basic() {
    let janus = JanusTest::new();

    let id1 = janus
        .run_success(&["plan", "create", "First Plan"])
        .trim()
        .to_string();
    let id2 = janus
        .run_success(&["plan", "create", "Second Plan"])
        .trim()
        .to_string();

    let output = janus.run_success(&["plan", "ls"]);
    assert!(output.contains(&id1));
    assert!(output.contains(&id2));
    assert!(output.contains("First Plan"));
    assert!(output.contains("Second Plan"));
}

#[test]
fn test_plan_ls_status_filter() {
    let janus = JanusTest::new();

    // Create a plan with completed tickets
    let ticket_content = r#"---
id: j-done2
status: complete
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Done Task

Completed.
"#;
    janus.write_ticket("j-done2", ticket_content);

    let complete_plan = r#"---
id: plan-complete
uuid: 550e8400-e29b-41d4-a716-446655440001
created: 2024-01-01T00:00:00Z
---
# Complete Plan

## Tickets

1. j-done2
"#;
    janus.write_plan("plan-complete", &complete_plan);

    // Create a plan with new tickets (no actual tickets, so it's "new")
    let new_id = janus
        .run_success(&["plan", "create", "New Plan"])
        .trim()
        .to_string();

    // Test status filter for complete
    let output = janus.run_success(&["plan", "ls", "--status", "complete"]);
    assert!(output.contains("plan-complete"));
    assert!(!output.contains(&new_id));

    // Test status filter for new
    let output = janus.run_success(&["plan", "ls", "--status", "new"]);
    assert!(!output.contains("plan-complete"));
    assert!(output.contains(&new_id));
}

#[test]
fn test_plan_move_ticket_simple_plan_fails() {
    let janus = JanusTest::new();

    // Create a simple plan
    let plan_id = janus
        .run_success(&["plan", "create", "Simple Plan"])
        .trim()
        .to_string();

    // Create and add a ticket
    let ticket_id = janus
        .run_success(&["create", "Test Ticket"])
        .trim()
        .to_string();
    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket_id]);

    // Try to move ticket (should fail - simple plans don't have phases)
    let output = janus.run_failure(&[
        "plan",
        "move-ticket",
        &plan_id,
        &ticket_id,
        "--to-phase",
        "Nonexistent",
    ]);
    assert!(output.contains("simple plan"));
}

#[test]
fn test_plan_remove_phase_with_tickets_fails_without_force() {
    let janus = JanusTest::new();

    // Create a phased plan
    let plan_id = janus
        .run_success(&["plan", "create", "Phased Plan", "--phase", "Phase One"])
        .trim()
        .to_string();

    // Create and add a ticket to the phase
    let ticket_id = janus
        .run_success(&["create", "Test Ticket"])
        .trim()
        .to_string();
    janus.run_success(&[
        "plan",
        "add-ticket",
        &plan_id,
        &ticket_id,
        "--phase",
        "Phase One",
    ]);

    // Try to remove phase (should fail without --force)
    let output = janus.run_failure(&["plan", "remove-phase", &plan_id, "Phase One"]);
    assert!(output.contains("contains tickets"));
}

#[test]
fn test_plan_ls_json_format() {
    let janus = JanusTest::new();

    // Create a couple of plans
    janus.run_success(&["plan", "create", "Plan One"]);
    janus.run_success(&["plan", "create", "Plan Two"]);

    // Run with --json
    let output = janus.run_success(&["plan", "ls", "--json"]);

    // Should be valid JSON array
    assert!(output.starts_with("["), "Should be JSON array");
    assert!(output.contains("\"id\""), "Should have id field");
    assert!(output.contains("\"title\""), "Should have title field");
    assert!(output.contains("Plan One"), "Should contain first plan");
    assert!(output.contains("Plan Two"), "Should contain second plan");
}

#[test]
fn test_plan_ls_json_format_with_status_filter() {
    let janus = JanusTest::new();

    // Create plans - they will all be "new" status since no tickets
    janus.run_success(&["plan", "create", "New Plan"]);

    // Run with --json and --status filter
    let output = janus.run_success(&["plan", "ls", "--json", "--status", "new"]);

    // Should be valid JSON
    assert!(output.starts_with("["), "Should be JSON array");
    assert!(output.contains("New Plan"), "Should contain the new plan");
}

// ============================================================================
// Plan Show --verbose-phase Tests
// ============================================================================

#[test]
fn test_import_checklist_tasks() {
    let janus = JanusTest::new();

    // Create a plan with H4 tasks (checklist-style no longer supported)
    let plan_doc = r#"# Checklist Plan

## Design

Design details.

## Implementation

### Phase 1: Tasks

#### Unchecked task one

Description.

#### Completed task two [x]

Description.

#### Task three

Description.
"#;

    let plan_path = janus.temp_dir.path().join("checklist_plan.md");
    fs::write(&plan_path, plan_doc).expect("Failed to write plan file");

    // Import the plan
    let output = janus.run_success(&["plan", "import", plan_path.to_str().unwrap()]);
    let plan_id = output.trim();

    // Verify plan was created
    assert!(janus.plan_exists(plan_id), "Plan file should exist");
}

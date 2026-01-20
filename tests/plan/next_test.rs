use serial_test::serial;

use crate::common::JanusTest;

#[test]
fn test_plan_next_simple() {
    let janus = JanusTest::new();

    // Create a simple plan
    let plan_id = janus
        .run_success(&["plan", "create", "Simple Plan"])
        .trim()
        .to_string();

    // Create tickets
    let ticket1 = janus
        .run_success(&["create", "Ticket 1"])
        .trim()
        .to_string();
    let ticket2 = janus
        .run_success(&["create", "Ticket 2"])
        .trim()
        .to_string();

    // Add tickets to plan
    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket1]);
    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket2]);

    // Get next item
    let output = janus.run_success(&["plan", "next", &plan_id]);
    assert!(
        output.contains(&ticket1),
        "Should show first ticket as next"
    );
    assert!(output.contains("[new]"), "Should show status badge");
}

#[test]
fn test_plan_next_skips_complete() {
    let janus = JanusTest::new();

    // Create a simple plan
    let plan_id = janus
        .run_success(&["plan", "create", "Simple Plan"])
        .trim()
        .to_string();

    // Create tickets
    let ticket1 = janus
        .run_success(&["create", "Ticket 1"])
        .trim()
        .to_string();
    let ticket2 = janus
        .run_success(&["create", "Ticket 2"])
        .trim()
        .to_string();

    // Add tickets to plan
    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket1]);
    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket2]);

    // Complete first ticket
    janus.run_success(&["close", &ticket1, "--no-summary"]);

    // Get next item - should be ticket2
    let output = janus.run_success(&["plan", "next", &plan_id]);
    assert!(
        output.contains(&ticket2),
        "Should show second ticket as next"
    );
    assert!(
        !output.contains(&ticket1),
        "Should not show completed ticket"
    );
}

#[test]
fn test_plan_next_phased() {
    let janus = JanusTest::new();

    // Create a phased plan
    let plan_id = janus
        .run_success(&[
            "plan",
            "create",
            "Phased Plan",
            "--phase",
            "Phase 1",
            "--phase",
            "Phase 2",
        ])
        .trim()
        .to_string();

    // Create tickets
    let ticket1 = janus
        .run_success(&["create", "Ticket 1"])
        .trim()
        .to_string();
    let ticket2 = janus
        .run_success(&["create", "Ticket 2"])
        .trim()
        .to_string();

    // Add tickets to different phases
    janus.run_success(&[
        "plan",
        "add-ticket",
        &plan_id,
        &ticket1,
        "--phase",
        "Phase 1",
    ]);
    janus.run_success(&[
        "plan",
        "add-ticket",
        &plan_id,
        &ticket2,
        "--phase",
        "Phase 2",
    ]);

    // Get next item - should show from Phase 1
    let output = janus.run_success(&["plan", "next", &plan_id]);
    assert!(output.contains("Phase 1"), "Should show phase name");
    assert!(output.contains(&ticket1), "Should show ticket from Phase 1");
}

#[test]
fn test_plan_next_phased_skips_complete_phase() {
    let janus = JanusTest::new();

    // Create a phased plan
    let plan_id = janus
        .run_success(&[
            "plan",
            "create",
            "Phased Plan",
            "--phase",
            "Phase 1",
            "--phase",
            "Phase 2",
        ])
        .trim()
        .to_string();

    // Create tickets
    let ticket1 = janus
        .run_success(&["create", "Ticket 1"])
        .trim()
        .to_string();
    let ticket2 = janus
        .run_success(&["create", "Ticket 2"])
        .trim()
        .to_string();

    // Add tickets to different phases
    janus.run_success(&[
        "plan",
        "add-ticket",
        &plan_id,
        &ticket1,
        "--phase",
        "Phase 1",
    ]);
    janus.run_success(&[
        "plan",
        "add-ticket",
        &plan_id,
        &ticket2,
        "--phase",
        "Phase 2",
    ]);

    // Complete Phase 1 ticket
    janus.run_success(&["close", &ticket1, "--no-summary"]);

    // Get next item - should show from Phase 2
    let output = janus.run_success(&["plan", "next", &plan_id]);
    assert!(output.contains("Phase 2"), "Should show Phase 2");
    assert!(output.contains(&ticket2), "Should show ticket from Phase 2");
}

#[test]
fn test_plan_next_all_complete() {
    let janus = JanusTest::new();

    // Create a simple plan
    let plan_id = janus
        .run_success(&["plan", "create", "Simple Plan"])
        .trim()
        .to_string();

    // Create and add a ticket
    let ticket1 = janus
        .run_success(&["create", "Ticket 1"])
        .trim()
        .to_string();
    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket1]);

    // Complete the ticket
    janus.run_success(&["close", &ticket1, "--no-summary"]);

    // Get next item - should say no actionable items
    let output = janus.run_success(&["plan", "next", &plan_id]);
    assert!(
        output.contains("No actionable items"),
        "Should indicate no more items"
    );
}

#[test]
fn test_plan_next_with_count() {
    let janus = JanusTest::new();

    // Create a simple plan
    let plan_id = janus
        .run_success(&["plan", "create", "Simple Plan"])
        .trim()
        .to_string();

    // Create tickets
    let ticket1 = janus
        .run_success(&["create", "Ticket 1"])
        .trim()
        .to_string();
    let ticket2 = janus
        .run_success(&["create", "Ticket 2"])
        .trim()
        .to_string();
    let ticket3 = janus
        .run_success(&["create", "Ticket 3"])
        .trim()
        .to_string();

    // Add tickets to plan
    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket1]);
    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket2]);
    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket3]);

    // Get next 2 items
    let output = janus.run_success(&["plan", "next", &plan_id, "--count", "2"]);
    assert!(output.contains(&ticket1), "Should show first ticket");
    assert!(output.contains(&ticket2), "Should show second ticket");
    // Third ticket may or may not be shown depending on implementation
}

#[test]
fn test_plan_next_phased_all_flag() {
    let janus = JanusTest::new();

    // Create a phased plan
    let plan_id = janus
        .run_success(&[
            "plan",
            "create",
            "Phased Plan",
            "--phase",
            "Phase 1",
            "--phase",
            "Phase 2",
        ])
        .trim()
        .to_string();

    // Create tickets
    let ticket1 = janus
        .run_success(&["create", "Ticket 1"])
        .trim()
        .to_string();
    let ticket2 = janus
        .run_success(&["create", "Ticket 2"])
        .trim()
        .to_string();

    // Add tickets to different phases
    janus.run_success(&[
        "plan",
        "add-ticket",
        &plan_id,
        &ticket1,
        "--phase",
        "Phase 1",
    ]);
    janus.run_success(&[
        "plan",
        "add-ticket",
        &plan_id,
        &ticket2,
        "--phase",
        "Phase 2",
    ]);

    // Get next item from all phases
    let output = janus.run_success(&["plan", "next", &plan_id, "--all"]);
    assert!(output.contains("Phase 1"), "Should show Phase 1");
    assert!(output.contains("Phase 2"), "Should show Phase 2");
    assert!(output.contains(&ticket1), "Should show ticket from Phase 1");
    assert!(output.contains(&ticket2), "Should show ticket from Phase 2");
}

// ============================================================================
// Plan Status command tests
// ============================================================================

#[test]
fn test_plan_next_not_found() {
    let janus = JanusTest::new();

    // Try to get next from non-existent plan
    let output = janus.run_failure(&["plan", "next", "nonexistent"]);
    assert!(output.contains("not found"));
}

// ============================================================================
// Additional Plan Edge Case Tests (Phase 9)
// ============================================================================

#[test]
fn test_plan_status_all_next() {
    let janus = JanusTest::new();

    // Create tickets with 'next' status
    let ticket1_content = r#"---
id: j-next1
status: next
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Next Task 1

Ready to start.
"#;
    janus.write_ticket("j-next1", ticket1_content);

    let ticket2_content = r#"---
id: j-next2
status: next
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Next Task 2

Also ready.
"#;
    janus.write_ticket("j-next2", ticket2_content);

    let plan_content = r#"---
id: plan-allnext
uuid: 550e8400-e29b-41d4-a716-446655440002
created: 2024-01-01T00:00:00Z
---
# All Next Plan

## Tickets

1. j-next1
2. j-next2
"#;
    janus.write_plan("plan-allnext", &plan_content);

    let output = janus.run_success(&["plan", "status", "plan-allnext"]);
    // All new/next should show as new
    assert!(
        output.contains("new") || output.contains("[new]"),
        "All next tickets should show plan as new"
    );
}

#[test]
fn test_plan_next_empty_plan() {
    let janus = JanusTest::new();

    // Create an empty simple plan
    let plan_id = janus
        .run_success(&["plan", "create", "Empty Plan"])
        .trim()
        .to_string();

    // Get next should indicate no actionable items
    let output = janus.run_success(&["plan", "next", &plan_id]);
    assert!(
        output.contains("No actionable items") || output.contains("no tickets"),
        "Should indicate no actionable items"
    );
}

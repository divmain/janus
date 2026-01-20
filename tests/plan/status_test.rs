use serial_test::serial;

use crate::common::JanusTest;

#[test]
fn test_plan_status_simple() {
    let janus = JanusTest::new();

    // Create a simple plan
    let plan_id = janus
        .run_success(&["plan", "create", "Simple Plan"])
        .trim()
        .to_string();

    // Create and add tickets
    let ticket1 = janus
        .run_success(&["create", "Ticket 1"])
        .trim()
        .to_string();
    let ticket2 = janus
        .run_success(&["create", "Ticket 2"])
        .trim()
        .to_string();

    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket1]);
    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket2]);

    // Get status
    let output = janus.run_success(&["plan", "status", &plan_id]);
    assert!(output.contains("Plan:"), "Should show plan header");
    assert!(output.contains("Simple Plan"), "Should show plan title");
    assert!(output.contains("Status:"), "Should show status label");
    assert!(output.contains("Progress:"), "Should show progress label");
    assert!(output.contains("0/2"), "Should show 0 of 2 complete");
}

#[test]
fn test_plan_status_with_progress() {
    let janus = JanusTest::new();

    // Create a simple plan
    let plan_id = janus
        .run_success(&["plan", "create", "Simple Plan"])
        .trim()
        .to_string();

    // Create and add tickets
    let ticket1 = janus
        .run_success(&["create", "Ticket 1"])
        .trim()
        .to_string();
    let ticket2 = janus
        .run_success(&["create", "Ticket 2"])
        .trim()
        .to_string();

    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket1]);
    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket2]);

    // Complete one ticket
    janus.run_success(&["close", &ticket1, "--no-summary"]);

    // Get status
    let output = janus.run_success(&["plan", "status", &plan_id]);
    assert!(output.contains("1/2"), "Should show 1 of 2 complete");
    assert!(
        output.contains("in_progress") || output.contains("[in_progress]"),
        "Should show in_progress status"
    );
}

#[test]
fn test_plan_status_phased() {
    let janus = JanusTest::new();

    // Create a phased plan
    let plan_id = janus
        .run_success(&[
            "plan",
            "create",
            "Phased Plan",
            "--phase",
            "Infrastructure",
            "--phase",
            "Implementation",
        ])
        .trim()
        .to_string();

    // Create and add tickets
    let ticket1 = janus
        .run_success(&["create", "Setup database"])
        .trim()
        .to_string();
    let ticket2 = janus
        .run_success(&["create", "Create API"])
        .trim()
        .to_string();

    janus.run_success(&[
        "plan",
        "add-ticket",
        &plan_id,
        &ticket1,
        "--phase",
        "Infrastructure",
    ]);
    janus.run_success(&[
        "plan",
        "add-ticket",
        &plan_id,
        &ticket2,
        "--phase",
        "Implementation",
    ]);

    // Get status
    let output = janus.run_success(&["plan", "status", &plan_id]);
    assert!(output.contains("Phases:"), "Should show phases section");
    assert!(
        output.contains("Infrastructure"),
        "Should show phase name Infrastructure"
    );
    assert!(
        output.contains("Implementation"),
        "Should show phase name Implementation"
    );
}

#[test]
fn test_plan_status_complete() {
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

    // Get status
    let output = janus.run_success(&["plan", "status", &plan_id]);
    assert!(output.contains("1/1"), "Should show 1 of 1 complete");
    assert!(
        output.contains("complete") || output.contains("[complete]"),
        "Should show complete status"
    );
}

#[test]
fn test_plan_status_empty_plan() {
    let janus = JanusTest::new();

    // Create a simple plan with no tickets
    let plan_id = janus
        .run_success(&["plan", "create", "Empty Plan"])
        .trim()
        .to_string();

    // Get status
    let output = janus.run_success(&["plan", "status", &plan_id]);
    assert!(output.contains("Empty Plan"), "Should show plan title");
    assert!(output.contains("0/0"), "Should show 0 of 0");
}

#[test]
fn test_plan_status_not_found() {
    let janus = JanusTest::new();

    // Try to get status of non-existent plan
    let output = janus.run_failure(&["plan", "status", "nonexistent"]);
    assert!(output.contains("not found"));
}

#[test]
fn test_plan_status_all_cancelled() {
    let janus = JanusTest::new();

    // Create a plan with cancelled tickets
    let ticket1_content = r#"---
id: j-canc1
status: cancelled
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Cancelled Task 1

Cancelled.
"#;
    janus.write_ticket("j-canc1", ticket1_content);

    let ticket2_content = r#"---
id: j-canc2
status: cancelled
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Cancelled Task 2

Also cancelled.
"#;
    janus.write_ticket("j-canc2", ticket2_content);

    let plan_content = r#"---
id: plan-allcanc
uuid: 550e8400-e29b-41d4-a716-446655440000
created: 2024-01-01T00:00:00Z
---
# All Cancelled Plan

## Tickets

1. j-canc1
2. j-canc2
"#;
    janus.write_plan("plan-allcanc", &plan_content);

    let output = janus.run_success(&["plan", "status", "plan-allcanc"]);
    assert!(
        output.contains("cancelled") || output.contains("[cancelled]"),
        "Should show cancelled status"
    );
}

#[test]
fn test_plan_status_mixed_complete_cancelled() {
    let janus = JanusTest::new();

    // Create tickets with mixed complete/cancelled statuses
    let ticket1_content = r#"---
id: j-comp1
status: complete
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Completed Task

Done!
"#;
    janus.write_ticket("j-comp1", ticket1_content);

    let ticket2_content = r#"---
id: j-canc3
status: cancelled
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Cancelled Task

Cancelled.
"#;
    janus.write_ticket("j-canc3", ticket2_content);

    let plan_content = r#"---
id: plan-mixfinish
uuid: 550e8400-e29b-41d4-a716-446655440001
created: 2024-01-01T00:00:00Z
---
# Mixed Finished Plan

## Tickets

1. j-comp1
2. j-canc3
"#;
    janus.write_plan("plan-mixfinish", &plan_content);

    let output = janus.run_success(&["plan", "status", "plan-mixfinish"]);
    // Mixed complete/cancelled should show as complete
    assert!(
        output.contains("complete") || output.contains("[complete]"),
        "Mixed complete/cancelled should show as complete"
    );
}

#[test]
fn test_plan_status_with_in_progress_tickets() {
    let janus = JanusTest::new();

    // Create tickets with in_progress status
    let ticket1_content = r#"---
id: j-inprog1
status: in_progress
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# In Progress Task 1

Working on it.
"#;
    janus.write_ticket("j-inprog1", ticket1_content);

    let ticket2_content = r#"---
id: j-newt
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# New Task

Not started.
"#;
    janus.write_ticket("j-newt", ticket2_content);

    let plan_content = r#"---
id: plan-inprog
uuid: 550e8400-e29b-41d4-a716-446655440008
created: 2024-01-01T00:00:00Z
---
# In Progress Plan

## Tickets

1. j-inprog1
2. j-newt
"#;
    janus.write_plan("plan-inprog", &plan_content);

    let output = janus.run_success(&["plan", "status", "plan-inprog"]);
    assert!(
        output.contains("in_progress") || output.contains("[in_progress]"),
        "Should show in_progress status"
    );
}

#[test]
fn test_plan_phased_status_first_complete_second_new() {
    let janus = JanusTest::new();

    // Phase 1 complete, Phase 2 not started - should be in_progress overall
    let ticket1_content = r#"---
id: j-ph1done
status: complete
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Phase 1 Complete Task

Done.
"#;
    janus.write_ticket("j-ph1done", ticket1_content);

    let ticket2_content = r#"---
id: j-ph2new
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Phase 2 New Task

Not started.
"#;
    janus.write_ticket("j-ph2new", ticket2_content);

    let plan_content = r#"---
id: plan-ph12
uuid: 550e8400-e29b-41d4-a716-446655440009
created: 2024-01-01T00:00:00Z
---
# Two Phase Plan

## Phase 1: Done

### Tickets

1. j-ph1done

## Phase 2: Not Started

### Tickets

1. j-ph2new
"#;
    janus.write_plan("plan-ph12", &plan_content);

    let output = janus.run_success(&["plan", "status", "plan-ph12"]);
    // Overall plan should be in_progress (some complete, some new)
    assert!(
        output.contains("in_progress") || output.contains("[in_progress]"),
        "Overall plan should be in_progress"
    );

    // Phase 1 should show as complete
    assert!(output.contains("Done"));
    // Phase 2 should show
    assert!(output.contains("Not Started"));
}

// ============================================================================
// Plan Show/Ls Format Option Tests
// ============================================================================

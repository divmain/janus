use serial_test::serial;

use crate::common::JanusTest;

#[test]
fn test_plan_show_simple() {
    let janus = JanusTest::new();

    let id = janus
        .run_success(&["plan", "create", "Show Test Plan"])
        .trim()
        .to_string();

    let output = janus.run_success(&["plan", "show", &id]);
    assert!(output.contains("Show Test Plan"));
    assert!(output.contains("Progress:"));
    assert!(output.contains("[new]"));
}

#[test]
fn test_plan_show_raw() {
    let janus = JanusTest::new();

    let id = janus
        .run_success(&["plan", "create", "Raw Test Plan"])
        .trim()
        .to_string();

    let output = janus.run_success(&["plan", "show", &id, "--raw"]);
    // Raw output should contain the frontmatter delimiters
    assert!(output.contains("---"));
    assert!(output.contains(&format!("id: {}", id)));
    assert!(output.contains("# Raw Test Plan"));
}

#[test]
fn test_plan_show_with_tickets() {
    let janus = JanusTest::new();

    // Create tickets with known IDs
    let ticket1_content = r#"---
id: j-task1
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Task One

First task.
"#;
    janus.write_ticket("j-task1", ticket1_content);

    let ticket2_content = r#"---
id: j-task2
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Task Two

Second task.
"#;
    janus.write_ticket("j-task2", ticket2_content);

    // Create a simple plan with these tickets
    let content = r#"---
id: plan-test
uuid: 550e8400-e29b-41d4-a716-446655440000
created: 2024-01-01T00:00:00Z
---
# Plan with Tickets

Test plan description.

## Tickets

1. j-task1
2. j-task2
"#;
    janus.write_plan("plan-test", &content);

    let output = janus.run_success(&["plan", "show", "plan-test"]);
    assert!(output.contains("Plan with Tickets"));
    assert!(output.contains("j-task1"));
    assert!(output.contains("j-task2"));
    assert!(output.contains("Task One"));
    assert!(output.contains("Task Two"));
    assert!(output.contains("[new]"));
}

#[test]
fn test_plan_show_phased_with_status() {
    let janus = JanusTest::new();

    // Create tickets with different statuses
    let ticket1_content = r#"---
id: j-done1
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
    janus.write_ticket("j-done1", ticket1_content);

    let ticket2_content = r#"---
id: j-prog1
status: in_progress
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# In Progress Task

Working on it.
"#;
    janus.write_ticket("j-prog1", ticket2_content);

    let ticket3_content = r#"---
id: j-new1
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
    janus.write_ticket("j-new1", ticket3_content);

    // Create a phased plan
    let plan_content = r#"---
id: plan-phased
uuid: 550e8400-e29b-41d4-a716-446655440000
created: 2024-01-01T00:00:00Z
---
# Phased Plan Test

Test plan with phases.

## Phase 1: Complete Phase

First phase description.

### Tickets

1. j-done1

## Phase 2: In Progress Phase

Second phase.

### Tickets

1. j-prog1
2. j-new1
"#;
    janus.write_plan("plan-phased", &plan_content);

    let output = janus.run_success(&["plan", "show", "plan-phased"]);

    // Check plan shows overall in_progress status
    assert!(output.contains("[in_progress]"));

    // Check phase statuses
    assert!(output.contains("Phase 1: Complete Phase"));
    assert!(output.contains("Phase 2: In Progress Phase"));

    // Check ticket statuses are shown
    assert!(output.contains("[complete]"));
    assert!(output.contains("Completed Task"));
    assert!(output.contains("In Progress Task"));
    assert!(output.contains("[new]"));
    assert!(output.contains("New Task"));
}

#[test]
fn test_plan_show_missing_ticket() {
    let janus = JanusTest::new();

    // Create a plan referencing a non-existent ticket
    let content = r#"---
id: plan-missing
uuid: 550e8400-e29b-41d4-a716-446655440000
created: 2024-01-01T00:00:00Z
---
# Plan with Missing Ticket

## Tickets

1. j-nonexistent
"#;
    janus.write_plan("plan-missing", &content);

    let output = janus.run_success(&["plan", "show", "plan-missing"]);
    assert!(output.contains("[missing]"));
    assert!(output.contains("j-nonexistent"));
}

#[test]
fn test_plan_show_partial_id() {
    let janus = JanusTest::new();

    // Create a plan - the ID will be like plan-xxxx
    let id = janus
        .run_success(&["plan", "create", "Partial ID Test"])
        .trim()
        .to_string();

    // Should be able to find it with partial ID (just the hash part)
    let hash_part = id.strip_prefix("plan-").unwrap();
    let output = janus.run_success(&["plan", "show", hash_part]);
    assert!(output.contains("Partial ID Test"));
}

#[test]
fn test_plan_show_with_freeform_sections() {
    let janus = JanusTest::new();

    let content = r#"---
id: plan-freeform
uuid: 550e8400-e29b-41d4-a716-446655440000
created: 2024-01-01T00:00:00Z
---
# Plan with Free-form Sections

Description here.

## Overview

This is the overview section with details.

### Nested Header

Some nested content.

## Phase 1: Implementation

Phase description.

### Tickets

1. j-test1

## Technical Details

```sql
CREATE TABLE example (id TEXT);
```

## Open Questions

1. What about this?
2. And that?
"#;
    janus.write_plan("plan-freeform", &content);

    // Create the referenced ticket
    let ticket_content = r#"---
id: j-test1
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Test Ticket

Description.
"#;
    janus.write_ticket("j-test1", ticket_content);

    let output = janus.run_success(&["plan", "show", "plan-freeform"]);

    // Check free-form sections are displayed
    assert!(output.contains("## Overview"));
    assert!(output.contains("This is the overview section"));
    assert!(output.contains("## Technical Details"));
    assert!(output.contains("CREATE TABLE"));
    assert!(output.contains("## Open Questions"));

    // Check phase is displayed with status
    assert!(output.contains("Phase 1: Implementation"));
    assert!(output.contains("j-test1"));
}

// ============================================================================
// Plan Manipulation Command Tests
// ============================================================================

#[test]
fn test_plan_show_acceptance_criteria() {
    let janus = JanusTest::new();

    let plan_content = r#"---
id: plan-ac
uuid: 550e8400-e29b-41d4-a716-446655440005
created: 2024-01-01T00:00:00Z
---
# Plan with Acceptance Criteria

This is the description.

## Acceptance Criteria

- First criterion
- Second criterion
- Third criterion

## Tickets

"#;
    janus.write_plan("plan-ac", &plan_content);

    let output = janus.run_success(&["plan", "show", "plan-ac"]);
    assert!(output.contains("Acceptance Criteria"));
    assert!(output.contains("First criterion"));
    assert!(output.contains("Second criterion"));
    assert!(output.contains("Third criterion"));
}

#[test]
fn test_plan_help_shows_all_subcommands() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["plan", "--help"]);

    // Verify all plan subcommands are documented
    assert!(output.contains("create"), "Should document create command");
    assert!(output.contains("show"), "Should document show command");
    assert!(output.contains("edit"), "Should document edit command");
    assert!(output.contains("ls"), "Should document ls command");
    assert!(
        output.contains("add-ticket"),
        "Should document add-ticket command"
    );
    assert!(
        output.contains("remove-ticket"),
        "Should document remove-ticket command"
    );
    assert!(
        output.contains("move-ticket"),
        "Should document move-ticket command"
    );
    assert!(
        output.contains("add-phase"),
        "Should document add-phase command"
    );
    assert!(
        output.contains("remove-phase"),
        "Should document remove-phase command"
    );
    assert!(
        output.contains("reorder"),
        "Should document reorder command"
    );
    assert!(output.contains("delete"), "Should document delete command");
    assert!(output.contains("rename"), "Should document rename command");
    assert!(output.contains("next"), "Should document next command");
    assert!(output.contains("status"), "Should document status command");
}

#[test]
fn test_plan_show_tickets_only() {
    let janus = JanusTest::new();

    // Create a phased plan with tickets
    let plan_id = janus
        .run_success(&["plan", "create", "Test Plan", "--phase", "Phase One"])
        .trim()
        .to_string();

    let ticket1 = janus
        .run_success(&["create", "Task One"])
        .trim()
        .to_string();
    let ticket2 = janus
        .run_success(&["create", "Task Two"])
        .trim()
        .to_string();

    janus.run_success(&[
        "plan",
        "add-ticket",
        &plan_id,
        &ticket1,
        "--phase",
        "Phase One",
    ]);
    janus.run_success(&[
        "plan",
        "add-ticket",
        &plan_id,
        &ticket2,
        "--phase",
        "Phase One",
    ]);

    // Run with --tickets-only
    let output = janus.run_success(&["plan", "show", &plan_id, "--tickets-only"]);

    // Should show tickets but not the full plan structure
    assert!(output.contains(&ticket1), "Should show ticket 1");
    assert!(output.contains(&ticket2), "Should show ticket 2");
    // Should not show section headers like "## Phase"
    assert!(
        !output.contains("## Phase"),
        "Should not show full plan structure"
    );
}

#[test]
fn test_plan_show_phases_only() {
    let janus = JanusTest::new();

    // Create a phased plan
    let plan_id = janus
        .run_success(&[
            "plan",
            "create",
            "Test Plan",
            "--phase",
            "First Phase",
            "--phase",
            "Second Phase",
        ])
        .trim()
        .to_string();

    // Run with --phases-only
    let output = janus.run_success(&["plan", "show", &plan_id, "--phases-only"]);

    // Should show phases but not the full plan
    assert!(output.contains("First Phase"), "Should show first phase");
    assert!(output.contains("Second Phase"), "Should show second phase");
    // Should have phase numbers
    assert!(
        output.contains("1.") || output.contains("1 "),
        "Should show phase number"
    );
    assert!(
        output.contains("2.") || output.contains("2 "),
        "Should show phase number"
    );
}

#[test]
fn test_plan_show_phases_only_simple_plan() {
    let janus = JanusTest::new();

    // Create a simple plan (no phases)
    let plan_id = janus
        .run_success(&["plan", "create", "Simple Plan"])
        .trim()
        .to_string();

    // Run with --phases-only
    let output = janus.run_success(&["plan", "show", &plan_id, "--phases-only"]);

    // Should indicate it's a simple plan
    assert!(
        output.contains("simple plan") || output.contains("no phases"),
        "Should indicate no phases for simple plan"
    );
}

#[test]
fn test_plan_show_json_format() {
    let janus = JanusTest::new();

    // Create a plan with tickets
    let plan_id = janus
        .run_success(&["plan", "create", "JSON Test Plan"])
        .trim()
        .to_string();
    let ticket_id = janus
        .run_success(&["create", "Test Ticket"])
        .trim()
        .to_string();
    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket_id]);

    // Run with --json
    let output = janus.run_success(&["plan", "show", &plan_id, "--json"]);

    // Should be valid JSON
    assert!(output.starts_with("{"), "Should be JSON object");
    assert!(output.contains("\"id\""), "Should have id field");
    assert!(output.contains("\"title\""), "Should have title field");
    assert!(output.contains("\"status\""), "Should have status field");
    assert!(output.contains("\"tickets\""), "Should have tickets field");
    assert!(
        output.contains("JSON Test Plan"),
        "Should contain plan title"
    );
}

#[test]
fn test_plan_show_verbose_phase_shows_full_summary() {
    let janus = JanusTest::new();

    // Create a ticket with a multi-line completion summary
    let ticket_content = r#"---
id: j-verbose
status: complete
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Task with Long Summary

Description.

## Completion Summary

Line 1 of the completion summary.
Line 2 of the completion summary.
Line 3 of the completion summary.
Line 4 of the completion summary.
Line 5 of the completion summary.
"#;
    janus.write_ticket("j-verbose", ticket_content);

    // Create a phased plan with the ticket
    let plan_content = r#"---
id: plan-verbose
uuid: 550e8400-e29b-41d4-a716-446655440000
created: 2024-01-01T00:00:00Z
---
# Verbose Phase Test

Test plan.

## Phase 1: Test Phase

Description.

### Tickets

1. j-verbose
"#;
    janus.write_plan("plan-verbose", plan_content);

    // Without --verbose-phase, should only show first 2 lines
    let output = janus.run_success(&["plan", "show", "plan-verbose"]);
    assert!(output.contains("Line 1 of the completion summary"));
    assert!(output.contains("Line 2 of the completion summary"));
    assert!(
        !output.contains("Line 3 of the completion summary"),
        "Should not show line 3 without --verbose-phase"
    );

    // With --verbose-phase 1, should show all lines
    let output = janus.run_success(&["plan", "show", "plan-verbose", "--verbose-phase", "1"]);
    assert!(output.contains("Line 1 of the completion summary"));
    assert!(output.contains("Line 2 of the completion summary"));
    assert!(output.contains("Line 3 of the completion summary"));
    assert!(output.contains("Line 4 of the completion summary"));
    assert!(output.contains("Line 5 of the completion summary"));
}

#[test]
fn test_plan_show_verbose_phase_multiple_phases() {
    let janus = JanusTest::new();

    // Create tickets with completion summaries
    let ticket1_content = r#"---
id: j-phase1
status: complete
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Phase 1 Task

## Completion Summary

Phase 1 line 1.
Phase 1 line 2.
Phase 1 line 3.
"#;
    janus.write_ticket("j-phase1", ticket1_content);

    let ticket2_content = r#"---
id: j-phase2
status: complete
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Phase 2 Task

## Completion Summary

Phase 2 line 1.
Phase 2 line 2.
Phase 2 line 3.
"#;
    janus.write_ticket("j-phase2", ticket2_content);

    // Create a phased plan
    let plan_content = r#"---
id: plan-multi
uuid: 550e8400-e29b-41d4-a716-446655440001
created: 2024-01-01T00:00:00Z
---
# Multi Phase Test

## Phase 1: First

### Tickets

1. j-phase1

## Phase 2: Second

### Tickets

1. j-phase2
"#;
    janus.write_plan("plan-multi", plan_content);

    // With --verbose-phase for only phase 1, phase 2 should be truncated
    let output = janus.run_success(&["plan", "show", "plan-multi", "--verbose-phase", "1"]);
    assert!(
        output.contains("Phase 1 line 3"),
        "Phase 1 should show full summary"
    );
    assert!(
        !output.contains("Phase 2 line 3"),
        "Phase 2 should be truncated"
    );

    // With --verbose-phase for both phases
    let output = janus.run_success(&[
        "plan",
        "show",
        "plan-multi",
        "--verbose-phase",
        "1",
        "--verbose-phase",
        "2",
    ]);
    assert!(
        output.contains("Phase 1 line 3"),
        "Phase 1 should show full summary"
    );
    assert!(
        output.contains("Phase 2 line 3"),
        "Phase 2 should show full summary"
    );
}

#[test]
fn test_plan_show_verbose_phase_on_simple_plan_fails() {
    let janus = JanusTest::new();

    // Create a simple plan (no phases)
    let plan_id = janus
        .run_success(&["plan", "create", "Simple Plan"])
        .trim()
        .to_string();

    // --verbose-phase should fail on a simple plan
    let error = janus.run_failure(&["plan", "show", &plan_id, "--verbose-phase", "1"]);
    assert!(
        error.contains("--verbose-phase can only be used with phased plans"),
        "Should error when using --verbose-phase on simple plan: {}",
        error
    );
}

#[test]
fn test_plan_show_verbose_phase_nonexistent_phase() {
    let janus = JanusTest::new();

    // Create a ticket
    let ticket_content = r#"---
id: j-test
status: complete
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Test Task

## Completion Summary

Summary line 1.
Summary line 2.
Summary line 3.
"#;
    janus.write_ticket("j-test", ticket_content);

    // Create a phased plan with only phase 1
    let plan_content = r#"---
id: plan-one
uuid: 550e8400-e29b-41d4-a716-446655440002
created: 2024-01-01T00:00:00Z
---
# One Phase Plan

## Phase 1: Only Phase

### Tickets

1. j-test
"#;
    janus.write_plan("plan-one", plan_content);

    // --verbose-phase 99 should not fail, just not match any phase
    // Phase 1 tickets should still show truncated summary
    let output = janus.run_success(&["plan", "show", "plan-one", "--verbose-phase", "99"]);
    assert!(output.contains("Summary line 1"));
    assert!(output.contains("Summary line 2"));
    assert!(
        !output.contains("Summary line 3"),
        "Should not show line 3 when phase doesn't match"
    );
}

// ============================================================================
// Plan Reorder Tests
// ============================================================================

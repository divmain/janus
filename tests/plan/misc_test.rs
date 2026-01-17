use std::process::Command;

use serial_test::serial;

use crate::common::JanusTest;

#[test]
fn test_plan_reorder_no_tickets_message() {
    let janus = JanusTest::new();

    // Create an empty simple plan
    let plan_id = janus
        .run_success(&["plan", "create", "Empty Plan"])
        .trim()
        .to_string();

    // Reorder should handle empty plan gracefully with a message
    let output = janus.run_success(&["plan", "reorder", &plan_id]);

    // Should indicate there are no tickets to reorder
    assert!(
        output.contains("No tickets to reorder"),
        "Should indicate no tickets to reorder"
    );
}

#[test]
fn test_plan_edit_noninteractive() {
    let janus = JanusTest::new();

    let id = janus
        .run_success(&["plan", "create", "Edit Test Plan"])
        .trim()
        .to_string();

    // In non-interactive mode (CI), edit should print the file path
    let output = janus.run_success(&["plan", "edit", &id]);
    assert!(output.contains("Edit plan file:"));
    assert!(output.contains(&id));
}

#[test]
fn test_plan_not_found() {
    let janus = JanusTest::new();

    let output = janus.run_failure(&["plan", "show", "nonexistent-plan"]);
    assert!(output.contains("not found"));
}

#[test]
fn test_plan_large_many_phases() {
    let janus = JanusTest::new();

    // Create a plan with many phases (10+)
    let mut phases = Vec::new();
    for i in 1..=10 {
        phases.push(format!("--phase"));
        phases.push(format!("Phase {}", i));
    }

    let mut args: Vec<&str> = vec!["plan", "create", "Large Phased Plan"];
    for p in &phases {
        args.push(p);
    }

    let output = janus.run_success(&args);
    let plan_id = output.trim();

    assert!(janus.plan_exists(plan_id), "Plan file should exist");

    let content = janus.read_plan(plan_id);
    // Verify all 10 phases are created
    for i in 1..=10 {
        assert!(
            content.contains(&format!("Phase {}", i)),
            "Should contain Phase {}",
            i
        );
    }
}

#[test]
fn test_plan_large_many_tickets() {
    let janus = JanusTest::new();

    // Create many tickets
    let mut ticket_ids = Vec::new();
    for i in 1..=20 {
        let ticket_content = format!(
            r#"---
id: j-bulk{:02}
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Bulk Task {}

Description for task {}.
"#,
            i, i, i
        );
        janus.write_ticket(&format!("j-bulk{:02}", i), &ticket_content);
        ticket_ids.push(format!("j-bulk{:02}", i));
    }

    // Create a simple plan with all tickets
    let tickets_list: String = ticket_ids
        .iter()
        .enumerate()
        .map(|(i, id)| format!("{}. {}", i + 1, id))
        .collect::<Vec<_>>()
        .join("\n");

    let plan_content = format!(
        r#"---
id: plan-manytickets
uuid: 550e8400-e29b-41d4-a716-446655440003
created: 2024-01-01T00:00:00Z
---
# Plan with Many Tickets

Large plan with 20 tickets.

## Tickets

{}
"#,
        tickets_list
    );
    janus.write_plan("plan-manytickets", &plan_content);

    // Verify plan status works with many tickets
    let output = janus.run_success(&["plan", "status", "plan-manytickets"]);
    assert!(output.contains("0/20"), "Should show 0/20 progress");

    // Verify plan show works
    let output = janus.run_success(&["plan", "show", "plan-manytickets"]);
    assert!(output.contains("Bulk Task 1"));
    assert!(output.contains("Bulk Task 20"));
}

#[test]
fn test_plan_with_multiple_missing_tickets() {
    let janus = JanusTest::new();

    // Create a plan referencing multiple non-existent tickets
    let plan_content = r#"---
id: plan-manymissing
uuid: 550e8400-e29b-41d4-a716-446655440004
created: 2024-01-01T00:00:00Z
---
# Plan with Multiple Missing Tickets

## Tickets

1. j-missing1
2. j-missing2
3. j-missing3
"#;
    janus.write_plan("plan-manymissing", &plan_content);

    let output = janus.run_success(&["plan", "show", "plan-manymissing"]);
    // Should show all missing tickets
    assert!(output.contains("[missing]"));
    assert!(output.contains("j-missing1"));
    assert!(output.contains("j-missing2"));
    assert!(output.contains("j-missing3"));
}

#[test]
fn test_plan_phased_with_empty_phase() {
    let janus = JanusTest::new();

    // Create a ticket
    let ticket_content = r#"---
id: j-inphase
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Task in Phase

Description.
"#;
    janus.write_ticket("j-inphase", ticket_content);

    // Create a phased plan where one phase is empty
    let plan_content = r#"---
id: plan-emptyph
uuid: 550e8400-e29b-41d4-a716-446655440006
created: 2024-01-01T00:00:00Z
---
# Plan with Empty Phase

## Phase 1: Has Tickets

### Tickets

1. j-inphase

## Phase 2: Empty Phase

No tickets yet.

### Tickets

"#;
    janus.write_plan("plan-emptyph", &plan_content);

    let output = janus.run_success(&["plan", "show", "plan-emptyph"]);
    assert!(output.contains("Phase 1: Has Tickets"));
    assert!(output.contains("Phase 2: Empty Phase"));

    // Status should work with empty phase
    let output = janus.run_success(&["plan", "status", "plan-emptyph"]);
    assert!(output.contains("Phase 1") || output.contains("Has Tickets"));
}

#[test]
fn test_plan_with_code_blocks() {
    let janus = JanusTest::new();

    // Create a plan with code blocks that contain ## headers (edge case)
    let plan_content = r#"---
id: plan-code
uuid: 550e8400-e29b-41d4-a716-446655440007
created: 2024-01-01T00:00:00Z
---
# Plan with Code Blocks

Description.

## Overview

This section has code:

```markdown
## This is NOT a header

It's inside a code block.
```

## Tickets

"#;
    janus.write_plan("plan-code", &plan_content);

    let output = janus.run_success(&["plan", "show", "plan-code"]);
    // The code block content should be preserved, not parsed as a section
    // Note: comrak may normalize ``` markdown to ``` markdown (with space)
    assert!(
        output.contains("```") && output.contains("markdown"),
        "Code block fence should be present"
    );
    assert!(output.contains("## This is NOT a header"));
}

#[test]
fn test_plan_reorder_help() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["plan", "reorder", "--help"]);

    // Verify help shows the expected options
    assert!(output.contains("--phase"), "Should document --phase option");
    assert!(
        output.contains("--reorder-phases"),
        "Should document --reorder-phases option"
    );
}

#[test]
fn test_plan_reorder_plan_not_found() {
    let janus = JanusTest::new();

    let error = janus.run_failure(&["plan", "reorder", "nonexistent-plan"]);
    assert!(
        error.contains("not found") || error.contains("No plan"),
        "Should report plan not found"
    );
}

#[test]
fn test_plan_reorder_requires_interactive_terminal() {
    let janus = JanusTest::new();

    // Create a simple plan with tickets
    let plan_id = janus
        .run_success(&["plan", "create", "Test Plan"])
        .trim()
        .to_string();
    let ticket_id = janus
        .run_success(&["create", "Test Ticket"])
        .trim()
        .to_string();
    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket_id]);

    // Attempt to reorder - should fail because we're not in a TTY
    let error = janus.run_failure(&["plan", "reorder", &plan_id]);
    assert!(
        error.contains("interactive") || error.contains("terminal"),
        "Should require interactive terminal"
    );
}

#[test]
fn test_plan_reorder_phased_requires_phase_arg() {
    let janus = JanusTest::new();

    // Create a phased plan
    let plan_id = janus
        .run_success(&[
            "plan",
            "create",
            "Phased Plan",
            "--phase",
            "Phase One",
            "--phase",
            "Phase Two",
        ])
        .trim()
        .to_string();

    // Add tickets to phases
    let ticket1 = janus
        .run_success(&["create", "Ticket 1"])
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

    // Reorder without --phase or --reorder-phases should give guidance
    let output = janus.run(&["plan", "reorder", &plan_id]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should either:
    // 1. Suggest using --phase or --reorder-phases, OR
    // 2. Fail with interactive terminal requirement
    assert!(
        stdout.contains("--phase")
            || stdout.contains("--reorder-phases")
            || stderr.contains("interactive")
            || stderr.contains("terminal"),
        "Should guide user or fail gracefully"
    );
}

#[test]
fn test_plan_reorder_phase_not_found() {
    let janus = JanusTest::new();

    // Create a phased plan
    let plan_id = janus
        .run_success(&["plan", "create", "Phased Plan", "--phase", "Phase One"])
        .trim()
        .to_string();

    // Attempt to reorder non-existent phase
    let error = janus.run_failure(&["plan", "reorder", &plan_id, "--phase", "NonExistent"]);
    assert!(
        error.contains("not found") || error.contains("Phase"),
        "Should report phase not found"
    );
}

#[test]
fn test_plan_reorder_empty_phase() {
    let janus = JanusTest::new();

    // Create a phased plan with empty phase
    let plan_id = janus
        .run_success(&["plan", "create", "Phased Plan", "--phase", "Empty Phase"])
        .trim()
        .to_string();

    // Attempt to reorder empty phase - should handle gracefully
    let output = janus.run(&["plan", "reorder", &plan_id, "--phase", "Empty Phase"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Either succeeds with "No tickets to reorder" or fails with interactive requirement
    assert!(
        stdout.contains("No tickets")
            || stderr.contains("interactive")
            || stderr.contains("terminal"),
        "Should handle empty phase gracefully"
    );
}

#[test]
fn test_plan_reorder_phases_no_phases() {
    let janus = JanusTest::new();

    // Create a simple plan (no phases)
    let plan_id = janus
        .run_success(&["plan", "create", "Simple Plan"])
        .trim()
        .to_string();

    // Attempt to reorder phases in a simple plan
    let output = janus.run(&["plan", "reorder", &plan_id, "--reorder-phases"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Either succeeds with "No phases to reorder" or fails with interactive requirement
    assert!(
        stdout.contains("No phases")
            || stderr.contains("interactive")
            || stderr.contains("terminal"),
        "Should handle plan without phases gracefully"
    );
}

// ============================================================================
// Remote command consolidation (Phase 3)
// ============================================================================

#[test]
fn test_remote_browse_help() {
    let output = Command::new(concat!(env!("CARGO_MANIFEST_DIR"), "/target/debug/janus"))
        .args(["remote", "browse", "--help"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

#[test]
fn test_remote_adopt_help() {
    let output = Command::new(concat!(env!("CARGO_MANIFEST_DIR"), "/target/debug/janus"))
        .args(["remote", "adopt", "--help"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("REMOTE_REF"));
}

#[test]
fn test_remote_push_help() {
    let output = Command::new(concat!(env!("CARGO_MANIFEST_DIR"), "/target/debug/janus"))
        .args(["remote", "push", "--help"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

#[test]
fn test_remote_link_help() {
    let output = Command::new(concat!(env!("CARGO_MANIFEST_DIR"), "/target/debug/janus"))
        .args(["remote", "link", "--help"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("REMOTE_REF"));
}

#[test]
fn test_remote_sync_help() {
    let output = Command::new(concat!(env!("CARGO_MANIFEST_DIR"), "/target/debug/janus"))
        .args(["remote", "sync", "--help"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

#[test]
fn test_removed_commands_fail() {
    // Test that removed commands (adopt, push, remote-link, sync) all fail
    let removed_commands = vec![
        vec!["adopt", "github:foo/bar/1"],
        vec!["push", "j-1234"],
        vec!["remote-link", "j-1234", "github:foo/bar/1"],
        vec!["sync", "j-1234"],
    ];

    for cmd_args in removed_commands {
        let output = Command::new(concat!(env!("CARGO_MANIFEST_DIR"), "/target/debug/janus"))
            .args(&cmd_args)
            .output()
            .expect("Failed to execute command");

        assert!(
            !output.status.success(),
            "Command '{:?}' should fail but succeeded",
            cmd_args
        );
    }
}

#[test]
fn test_remote_no_subcommand_non_pty() {
    let output = Command::new(concat!(env!("CARGO_MANIFEST_DIR"), "/target/debug/janus"))
        .args(["remote"])
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{}{}", stdout, stderr);
    assert!(combined.contains("subcommand") || combined.contains("browse"));
}

#[test]
#[ignore]
fn test_help_has_command_groups() {
    // NOTE: clap's next_help_heading attribute does NOT work with subcommands at the
    // time of this writing. It is a known limitation documented in GitHub issue #5828:
    // https://github.com/clap-rs/clap/issues/5828
    //
    // There is an open PR that would add this functionality:
    // https://github.com/clap-rs/clap/pull/6183
    //
    // The test is ignored because the feature is not supported by clap yet.
    // Once that PR is merged and clap is updated, this test can be enabled and
    // the next_help_heading attributes can be added back to src/main.rs.

    let output = Command::new(concat!(env!("CARGO_MANIFEST_DIR"), "/target/debug/janus"))
        .args(["--help"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("Ticket Commands"));
    assert!(stdout.contains("Status Commands"));
    assert!(stdout.contains("List & Query"));
    assert!(stdout.contains("Relationships"));
}

// ============================================================================
// Plan Import Tests
// ============================================================================

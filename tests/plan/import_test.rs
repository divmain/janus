use std::fs;

use serial_test::serial;

use crate::common::JanusTest;

#[test]
fn test_import_simple_plan() {
    let janus = JanusTest::new();

    // Create an importable plan document using the new format
    let plan_doc = r#"# Simple Import Test Plan

This is the plan description.

## Design

This is the design section with architecture details.

## Implementation

### Phase 1: Setup

Phase description.

#### Task One

First task description.

#### Task Two

Second task description.
"#;

    // Write the plan document to a file
    let plan_path = janus.temp_dir.path().join("simple_plan.md");
    fs::write(&plan_path, plan_doc).expect("Failed to write plan file");

    // Import the plan
    let output = janus.run_success(&["plan", "import", plan_path.to_str().unwrap()]);
    let plan_id = output.trim();

    // Verify plan was created
    assert!(plan_id.starts_with("plan-"), "Should return a plan ID");
    assert!(janus.plan_exists(plan_id), "Plan file should exist");

    // Verify plan content
    let content = janus.read_plan(plan_id);
    assert!(content.contains("# Simple Import Test Plan"));
    assert!(content.contains("This is the plan description."));

    // Verify phase was created with tickets
    assert!(
        content.contains("## Phase 1"),
        "Should have a Phase section"
    );
}

#[test]
fn test_import_phased_plan() {
    let janus = JanusTest::new();

    // Create a phased importable plan document using the new format
    let plan_doc = r#"# Phased Import Test Plan

Overview of the implementation.

## Design

This is the design section.

## Acceptance Criteria

- All tests pass
- Documentation complete

## Implementation

### Phase 1: Infrastructure

Set up the foundational components.

#### Add Dependencies

Add the required dependencies.

#### Create Module Structure

Create the basic module structure.

### Phase 2: Core Logic

Implement the core logic.

#### Implement Core Function

The main implementation task.
"#;

    let plan_path = janus.temp_dir.path().join("phased_plan.md");
    fs::write(&plan_path, plan_doc).expect("Failed to write plan file");

    // Import the plan
    let output = janus.run_success(&["plan", "import", plan_path.to_str().unwrap()]);
    let plan_id = output.trim();

    // Verify plan was created
    assert!(janus.plan_exists(plan_id), "Plan file should exist");

    // Verify plan is phased
    let content = janus.read_plan(plan_id);
    assert!(content.contains("## Phase 1: Infrastructure"));
    assert!(content.contains("## Phase 2: Core Logic"));
    assert!(content.contains("## Acceptance Criteria"));
    assert!(content.contains("- All tests pass"));
    assert!(content.contains("- Documentation complete"));
}

#[test]
fn test_import_completed_tasks() {
    let janus = JanusTest::new();

    // Create a plan with completed tasks marked [x]
    let plan_doc = r#"# Plan with Completed Tasks

## Design

Design info.

## Implementation

### Phase 1: Tasks

#### Completed Task [x]

This task is done.

#### Pending Task

This task is not done.
"#;

    let plan_path = janus.temp_dir.path().join("completed_tasks_plan.md");
    fs::write(&plan_path, plan_doc).expect("Failed to write plan file");

    // Import the plan
    let output = janus.run_success(&["plan", "import", plan_path.to_str().unwrap()]);
    let plan_id = output.trim();

    // Verify plan was created
    assert!(janus.plan_exists(plan_id), "Plan file should exist");

    // The plan should have been created with tickets - check the show output
    let show_output = janus.run_success(&["plan", "show", plan_id]);
    // The completed task should be marked as complete
    assert!(
        show_output.contains("[complete]") || show_output.contains("complete"),
        "Should have a completed ticket"
    );
}

#[test]
fn test_import_with_acceptance_criteria() {
    let janus = JanusTest::new();

    // Create a plan with acceptance criteria
    let plan_doc = r#"# Plan with Acceptance Criteria

## Design

Design details.

## Acceptance Criteria

- Performance improved by 50%
- All tests pass
- Code coverage above 80%

## Implementation

### Phase 1: Optimization

#### Implement optimization

Add the optimization code.
"#;

    let plan_path = janus.temp_dir.path().join("ac_plan.md");
    fs::write(&plan_path, plan_doc).expect("Failed to write plan file");

    // Import the plan
    let output = janus.run_success(&["plan", "import", plan_path.to_str().unwrap()]);
    let plan_id = output.trim();

    // Verify acceptance criteria were imported
    let content = janus.read_plan(plan_id);
    assert!(content.contains("## Acceptance Criteria"));
    assert!(content.contains("Performance improved by 50%"));

    // Verify a verification ticket was created
    let show_output = janus.run_success(&["plan", "show", plan_id]);
    assert!(
        show_output.contains("acceptance criteria")
            || show_output.contains("Acceptance Criteria")
            || show_output.contains("Verify"),
        "Should have acceptance criteria or verification ticket"
    );
}

#[test]
fn test_import_dry_run() {
    let janus = JanusTest::new();

    // Create a plan document using the new format
    let plan_doc = r#"# Dry Run Test Plan

## Design

Design details.

## Implementation

### Phase 1: Work

#### Task One

Description.

#### Task Two

Description.
"#;

    let plan_path = janus.temp_dir.path().join("dry_run_plan.md");
    fs::write(&plan_path, plan_doc).expect("Failed to write plan file");

    // Run import with --dry-run
    let output = janus.run_success(&["plan", "import", plan_path.to_str().unwrap(), "--dry-run"]);

    // Verify dry-run output contains summary info
    assert!(
        output.contains("Dry Run Test Plan") || output.contains("Title:"),
        "Dry run should show plan title"
    );
    assert!(
        output.contains("Tasks:") || output.contains("tickets") || output.contains("Would create"),
        "Dry run should show task/ticket info"
    );

    // Verify no plan was actually created - check no new plans exist
    let plans_output = janus.run_success(&["plan", "ls"]);
    assert!(
        plans_output.trim().is_empty() || !plans_output.contains("Dry Run Test Plan"),
        "No plan should be created in dry-run mode"
    );
}

#[test]
fn test_import_duplicate_title_error() {
    let janus = JanusTest::new();

    // Create a plan with a specific title
    let plan_doc1 = r#"# Duplicate Title Plan

## Design

Design.

## Implementation

### Phase 1: Work

#### Task One

Description.
"#;

    let plan_path1 = janus.temp_dir.path().join("plan1.md");
    fs::write(&plan_path1, plan_doc1).expect("Failed to write plan file");

    // Import the first plan
    janus.run_success(&["plan", "import", plan_path1.to_str().unwrap()]);

    // Try to import a plan with the same title
    let plan_doc2 = r#"# Duplicate Title Plan

## Design

Design.

## Implementation

### Phase 1: Work

#### Task Two

Different description.
"#;

    let plan_path2 = janus.temp_dir.path().join("plan2.md");
    fs::write(&plan_path2, plan_doc2).expect("Failed to write plan file");

    // Second import should fail due to duplicate title
    let output = janus.run_failure(&["plan", "import", plan_path2.to_str().unwrap()]);
    assert!(
        output.contains("already exists") || output.contains("duplicate"),
        "Should fail with duplicate title error"
    );
}

#[test]
fn test_import_title_override() {
    let janus = JanusTest::new();

    // Create the first plan
    let plan_doc1 = r#"# Original Title

## Design

Design.

## Implementation

### Phase 1: Work

#### Task One

Description.
"#;

    let plan_path1 = janus.temp_dir.path().join("plan1.md");
    fs::write(&plan_path1, plan_doc1).expect("Failed to write plan file");

    // Import the first plan
    janus.run_success(&["plan", "import", plan_path1.to_str().unwrap()]);

    // Create second plan with same original title but use --title override
    let plan_doc2 = r#"# Original Title

## Design

Design.

## Implementation

### Phase 1: Work

#### Task Two

Different task.
"#;

    let plan_path2 = janus.temp_dir.path().join("plan2.md");
    fs::write(&plan_path2, plan_doc2).expect("Failed to write plan file");

    // Import with title override should succeed
    let output = janus.run_success(&[
        "plan",
        "import",
        plan_path2.to_str().unwrap(),
        "--title",
        "Different Title",
    ]);
    let plan_id = output.trim();

    // Verify the plan has the overridden title
    let content = janus.read_plan(plan_id);
    assert!(
        content.contains("# Different Title"),
        "Plan should have the overridden title"
    );
}

#[test]
fn test_import_invalid_format_no_title() {
    let janus = JanusTest::new();

    // Create an invalid plan document (no H1 title)
    let plan_doc = r#"Just some content without H1.

## Design

Design.

## Implementation

### Phase 1: Work

#### Task one

Description.
"#;

    let plan_path = janus.temp_dir.path().join("invalid_plan.md");
    fs::write(&plan_path, plan_doc).expect("Failed to write plan file");

    // Import should fail
    let output = janus.run_failure(&["plan", "import", plan_path.to_str().unwrap()]);
    assert!(
        output.contains("title") || output.contains("H1"),
        "Error should mention missing title"
    );
}

#[test]
fn test_import_invalid_format_no_tasks() {
    let janus = JanusTest::new();

    // Create a plan with no tasks (missing Implementation section)
    let plan_doc = r#"# Plan with No Tasks

Just a description with no tasks or phases.

## Design

Design info.
"#;

    let plan_path = janus.temp_dir.path().join("no_tasks_plan.md");
    fs::write(&plan_path, plan_doc).expect("Failed to write plan file");

    // Import should fail
    let output = janus.run_failure(&["plan", "import", plan_path.to_str().unwrap()]);
    assert!(
        output.contains("Implementation"),
        "Error should mention missing Implementation section"
    );
}

#[test]
fn test_import_with_custom_type() {
    let janus = JanusTest::new();

    // Create a plan document using the new format
    let plan_doc = r#"# Feature Plan

## Design

Design details.

## Implementation

### Phase 1: Features

#### Implement feature

Add the new feature.
"#;

    let plan_path = janus.temp_dir.path().join("feature_plan.md");
    fs::write(&plan_path, plan_doc).expect("Failed to write plan file");

    // Import with custom type
    let output = janus.run_success(&[
        "plan",
        "import",
        plan_path.to_str().unwrap(),
        "--type",
        "feature",
    ]);
    let plan_id = output.trim();

    // Verify plan was created
    assert!(janus.plan_exists(plan_id), "Plan file should exist");

    // Find the ticket and verify its type
    // The ticket should be referenced in the plan's Phase section
    let content = janus.read_plan(plan_id);
    // Extract ticket ID from the plan content
    if let Some(pos) = content.find("1. ") {
        let rest = &content[pos + 3..];
        if let Some(end) = rest.find('\n') {
            let ticket_id = rest[..end].trim();
            let ticket_content = janus.read_ticket(ticket_id);
            assert!(
                ticket_content.contains("type: feature"),
                "Ticket should have type: feature"
            );
        }
    }
}

#[test]
fn test_import_with_custom_prefix() {
    let janus = JanusTest::new();

    // Create a plan document using the new format
    let plan_doc = r#"# Prefix Test Plan

## Design

Design.

## Implementation

### Phase 1: Work

#### Task with custom prefix

Description.
"#;

    let plan_path = janus.temp_dir.path().join("prefix_plan.md");
    fs::write(&plan_path, plan_doc).expect("Failed to write plan file");

    // Import with custom prefix
    let output = janus.run_success(&[
        "plan",
        "import",
        plan_path.to_str().unwrap(),
        "--prefix",
        "imp",
    ]);
    let plan_id = output.trim();

    // Verify plan was created
    assert!(janus.plan_exists(plan_id), "Plan file should exist");

    // The created tickets should have the custom prefix
    let content = janus.read_plan(plan_id);
    assert!(
        content.contains("imp-"),
        "Tickets should have custom prefix 'imp-'"
    );
}

#[test]
fn test_import_spec_command() {
    let janus = JanusTest::new();

    // Run the import-spec command
    let output = janus.run_success(&["plan", "import-spec"]);

    // Verify it outputs the format specification
    assert!(
        output.contains("Plan Format Specification") || output.contains("# Plan Title"),
        "Should output the format specification"
    );
    assert!(
        output.contains("## Design") || output.contains("## Implementation"),
        "Should include section formats"
    );
}

#[test]
fn test_import_json_output() {
    let janus = JanusTest::new();

    // Create a plan document using the new format
    let plan_doc = r#"# JSON Output Test Plan

## Design

Design.

## Implementation

### Phase 1: Work

#### Task One

Description.
"#;

    let plan_path = janus.temp_dir.path().join("json_plan.md");
    fs::write(&plan_path, plan_doc).expect("Failed to write plan file");

    // Import with JSON output
    let output = janus.run_success(&["plan", "import", plan_path.to_str().unwrap(), "--json"]);

    // Verify JSON output
    assert!(output.contains("{"), "Output should be JSON");
    assert!(
        output.contains("plan_id") || output.contains("id"),
        "JSON should include plan_id"
    );
}

#[test]
fn test_import_dry_run_json_output() {
    let janus = JanusTest::new();

    // Create a plan document using the new format
    let plan_doc = r#"# Dry Run JSON Test

## Design

Design.

## Implementation

### Phase 1: Work

#### Task One

Description.

#### Task Two

Description.
"#;

    let plan_path = janus.temp_dir.path().join("dry_run_json.md");
    fs::write(&plan_path, plan_doc).expect("Failed to write plan file");

    // Run import with --dry-run and --json
    let output = janus.run_success(&[
        "plan",
        "import",
        plan_path.to_str().unwrap(),
        "--dry-run",
        "--json",
    ]);

    // Verify JSON output
    assert!(output.contains("{"), "Output should be JSON");
    assert!(
        output.contains("title") || output.contains("tasks"),
        "JSON should include title or tasks info"
    );
}

#[test]
fn test_import_plan_with_code_blocks() {
    let janus = JanusTest::new();

    // Create a plan with code blocks in task descriptions
    let plan_doc = r#"# Plan with Code

## Design

Technical design.

## Implementation

### Phase 1: Caching

#### Add Cache Support

Implement caching in the service.

```rust
let cache = HashMap::new();
```

Key changes:
- Add cache data structure
- Modify speak() method
"#;

    let plan_path = janus.temp_dir.path().join("code_plan.md");
    fs::write(&plan_path, plan_doc).expect("Failed to write plan file");

    // Import the plan
    let output = janus.run_success(&["plan", "import", plan_path.to_str().unwrap()]);
    let plan_id = output.trim();

    // Verify plan was created
    assert!(janus.plan_exists(plan_id), "Plan file should exist");

    // The ticket should have been created with the code block in its description
    let content = janus.read_plan(plan_id);
    // Find the ticket ID from the plan
    if let Some(pos) = content.find("1. ") {
        let rest = &content[pos + 3..];
        if let Some(end) = rest.find('\n') {
            let ticket_id = rest[..end].trim();
            if janus.ticket_exists(ticket_id) {
                let ticket_content = janus.read_ticket(ticket_id);
                assert!(
                    ticket_content.contains("HashMap") || ticket_content.contains("cache"),
                    "Ticket should contain code block content"
                );
            }
        }
    }
}

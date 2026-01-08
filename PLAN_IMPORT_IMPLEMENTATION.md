# Plan Import Implementation

This document outlines the step-by-step implementation plan for the `janus plan import` feature described in `PLAN_IMPORT.md`.

## Phase 1: Core Types and Error Handling

Add the foundational types needed for plan import functionality.

### Add import-specific error types to `src/error.rs`

Add two new error variants to `JanusError`:
- `ImportFailed { message: String, issues: Vec<String> }` - for validation/parsing failures with detailed issues (issues formatted as strings for display)
- `DuplicatePlanTitle(String, String)` - when a plan with the same title already exists (title, existing plan ID)

### Add importable plan types to `src/plan/types.rs`

Add the following structs:
- `ImportablePlan` - represents a parsed plan before ticket creation (title, description, acceptance_criteria, phases for phased plans, tasks for simple plans)
- `ImportablePhase` - represents a phase with number, name, description, and tasks
- `ImportableTask` - represents a task with title, body, and is_complete flag
- `ImportValidationError` - structured error with line number, message, and hint (used internally, converted to strings for error display)

### Export new types from `src/plan/mod.rs`

Add the new types to the module's public exports.

## Phase 2: Parser Implementation

Implement the parsing logic for importable plan documents.

### Add section alias constants to `src/plan/parser.rs`

Add constants for:
- `ACCEPTANCE_CRITERIA_ALIASES` - ["acceptance criteria", "goals", "success criteria", "deliverables", "requirements", "objectives"]
- `TASKS_SECTION_ALIASES` - ["tasks", "tickets", "work items", "items", "checklist"]
- `PHASE_PATTERN` - regex pattern for matching phase headers

### Implement `is_section_alias()` helper function

Create a function that checks if a heading text matches any alias in a given list (case-insensitive).

### Implement `is_phase_header()` helper function

Create a function that checks if a heading matches the phase pattern and extracts the phase number and name.

### Implement `is_completed_task()` helper function

Create a function that detects the `[x]` or `[X]` completion marker in task titles (both H3 style and checklist style) and returns (cleaned_title, is_complete).

### Implement `extract_title()` function

Parse the document AST to find the first H1 heading and return its text content.

### Implement `extract_description()` function

Extract paragraph content between the H1 title and the first H2 section.

### Implement `extract_acceptance_criteria()` function

Find an H2 section matching acceptance criteria aliases and extract list items as criteria strings.

### Implement `parse_tasks_from_section()` function

Extract tasks from a section, supporting both:
- H3 headers with content as body
- Checklist items (`- [ ]`, `- [x]`, or plain `- `)

### Implement `parse_phases()` function

Iterate through H2 sections, identify phase headers using `is_phase_header()`, extract phase description (paragraphs before first H3), and call `parse_tasks_from_section()` for each phase. Returns empty vec if no phases found.

### Implement `parse_simple_tasks()` function

Find the Tasks section (using `TASKS_SECTION_ALIASES`) and extract tasks from it. Used for simple plans that don't have phases. Returns empty vec if no Tasks section found.

### Implement `parse_importable_plan()` main entry point

Orchestrate the parsing:
1. Parse markdown to AST using comrak
2. Call `extract_title()` - error if missing
3. Call `extract_description()`
4. Call `extract_acceptance_criteria()`
5. Call `parse_phases()` to extract phased plan structure
6. If no phases found, call `parse_simple_tasks()` to extract simple plan tasks
7. Validate the result (has title, has either phases with tasks OR simple tasks)
8. Return `ImportablePlan` or error with validation issues

### Export parser functions from `src/plan/mod.rs`

Add `parse_importable_plan` and related types to module exports.

## Phase 3: Command Implementation

Implement the CLI command logic.

### Create the Plan Format Specification constant

Add a `PLAN_FORMAT_SPECIFICATION` const string in `src/commands/plan.rs` containing the full format documentation (from PLAN_IMPORT.md section 1).

### Implement `cmd_show_import_spec()` function

Simple function that prints the `PLAN_FORMAT_SPECIFICATION` constant.

### Implement duplicate title check helper

Create a helper function `check_duplicate_plan_title(title: &str) -> Result<(), JanusError>` that iterates through existing plans and returns `DuplicatePlanTitle` error if a match is found.

### Implement dry-run output formatting

Create a helper function to format and print the import summary showing:
- Title and description (truncated if long)
- Acceptance criteria count and items
- For phased plans: phase count, total task count, per-phase task list
- For simple plans: task count and task list
- Summary of what would be created (plan count, ticket count, verification ticket if applicable)

### Implement ticket creation from ImportableTask

Create a helper that converts an `ImportableTask` into ticket creation parameters:
- Title from task.title
- Description from task.body
- Status: complete if is_complete, else new
- Type: from command argument (default: task)
- Prefix: from command argument (optional)

### Implement `cmd_plan_import()` main function

Implement the full import logic:
1. Read content from file path or stdin (if "-")
2. Call `parse_importable_plan()`
3. Apply title override if `--title` provided
4. Call `check_duplicate_plan_title()`
5. If `--dry-run`: print summary and return
6. Build all tickets in memory (Vec of metadata + content):
   - For phased plans: iterate phases and their tasks
   - For simple plans: iterate top-level tasks
7. If acceptance criteria exist, add synthetic "Verify acceptance criteria" ticket as final task
8. Generate plan ID, UUID, timestamp
9. Write all tickets to disk, collecting actual IDs
10. Build PlanMetadata:
    - For phased plans: phases with ticket IDs
    - For simple plans: Tickets section with ticket IDs
11. Serialize and write plan
12. Output plan ID (or JSON if --json)

### Export command functions from `src/commands/mod.rs`

Add `cmd_plan_import` and `cmd_show_import_spec` to the module exports.

## Phase 4: CLI Integration

Wire up the commands to the CLI.

### Add `Import` variant to `PlanAction` enum in `src/main.rs`

Add the Import subcommand with arguments:
- `file: String` - file path or "-" for stdin
- `--dry-run` flag
- `--title <title>` option
- `--type <type>` option with default "task"
- `--prefix <prefix>` option
- `--json` flag

### Add `ShowImportSpec` variant to `PlanAction` enum

Add the import-spec subcommand (no arguments). This will be invoked as `janus plan import-spec`.

### Add match arms in main() for new commands

Handle `PlanAction::Import` by calling `cmd_plan_import()` with the appropriate arguments.
Handle `PlanAction::ShowImportSpec` by calling `cmd_show_import_spec()`.

## Phase 5: Testing

Add comprehensive tests for the import functionality.

### Add unit tests for parser helpers in `src/plan/parser.rs`

Test:
- `is_section_alias()` with various cases
- `is_phase_header()` with different formats
- `is_completed_task()` with `[x]`, `[X]`, and unchecked items

### Add unit tests for `parse_importable_plan()`

Test parsing of:
- Simple plan with Tasks section (H3 tasks)
- Simple plan with Tasks section (checklist tasks)
- Phased plan with multiple phases
- Plan with acceptance criteria
- Plan with completed tasks marked (`[x]`)
- Plan with H3 tasks including multi-line bodies with code blocks
- Mixed plan detection (phases take priority over Tasks section)
- Invalid documents (missing title, no tasks, empty phases)

### Add integration tests in `tests/integration_test.rs`

Test end-to-end:
- `test_import_simple_plan` - import creates plan and tickets
- `test_import_phased_plan` - phases and tickets created correctly
- `test_import_checklist_tasks` - checklist items become tickets
- `test_import_completed_tasks` - `[x]` items have status complete
- `test_import_with_acceptance_criteria` - synthetic verification ticket created
- `test_import_dry_run` - nothing created, summary shown
- `test_import_duplicate_title_error` - error returned for duplicate
- `test_import_title_override` - `--title` allows import despite duplicate
- `test_import_invalid_format` - helpful error for bad documents
- `test_import_from_stdin` - "-" reads from stdin correctly
- `test_import_atomicity` - partial failure creates nothing

## Phase 6: Documentation and Cleanup

Final polish and documentation.

### Update AGENTS.md with plan import documentation

Add a section documenting:
- The `janus plan import` command
- The Plan Format Specification reference
- Example usage

### Run cargo fmt and cargo clippy

Ensure all new code follows project style and has no warnings.

### Run full test suite

Verify all existing tests still pass and new tests pass.

### Manual testing with example documents

Test with real-world AI-generated plan documents to verify robustness.

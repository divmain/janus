use std::fs;
use std::process::Command;

mod common;
use common::JanusTest;

// ============================================================================
// Create command tests
// Note: Basic create tests are in tests/commands/create_test.rs
// ============================================================================

#[test]
fn test_create_default_triaged_false() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["create", "Test ticket"]);
    let id = output.trim();

    let output = janus.run_success(&["show", id]);
    assert!(
        output.contains("triaged: false"),
        "New tickets should have triaged: false"
    );
}

#[test]
fn test_old_ticket_without_triaged_field_parsed() {
    let janus = JanusTest::new();

    let yaml = r#"---
id: j-test123
uuid: abc123-def456
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Old Ticket
"#;
    janus.write_ticket("j-test123", yaml);

    let output = janus.run_success(&["show", "j-test123"]);
    assert!(
        output.contains("Old Ticket"),
        "Old ticket should be readable"
    );
}

// ============================================================================
// Spawning metadata tests
// ============================================================================

#[test]
fn test_create_with_spawned_from() {
    let janus = JanusTest::new();

    // Create a parent ticket
    let parent_id = janus
        .run_success(&["create", "Parent ticket"])
        .trim()
        .to_string();

    // Create a child ticket spawned from the parent
    let child_id = janus
        .run_success(&[
            "create",
            "Child ticket",
            "--spawned-from",
            &parent_id,
            "--spawn-context",
            "Needs OAuth setup first",
        ])
        .trim()
        .to_string();

    let child_output = janus.run_success(&["show", &child_id]);
    assert!(
        child_output.contains(&format!("spawned-from: {parent_id}")),
        "Child should have spawned-from field"
    );
    assert!(
        child_output.contains("spawn-context: Needs OAuth setup first"),
        "Child should have spawn-context field"
    );
    assert!(
        child_output.contains("depth: 1"),
        "Child should have depth: 1 (parent has implicit depth 0)"
    );
}

#[test]
fn test_create_spawned_chain_depth() {
    let janus = JanusTest::new();

    // Create a root ticket (no spawning fields)
    let root_id = janus
        .run_success(&["create", "Root ticket"])
        .trim()
        .to_string();

    // Create depth-1 ticket
    let depth1_id = janus
        .run_success(&["create", "Depth 1 ticket", "--spawned-from", &root_id])
        .trim()
        .to_string();

    let depth1_output = janus.run_success(&["show", &depth1_id]);
    assert!(
        depth1_output.contains("depth: 1"),
        "First spawn should have depth 1"
    );

    // Create depth-2 ticket
    let depth2_id = janus
        .run_success(&["create", "Depth 2 ticket", "--spawned-from", &depth1_id])
        .trim()
        .to_string();

    let depth2_output = janus.run_success(&["show", &depth2_id]);
    assert!(
        depth2_output.contains("depth: 2"),
        "Second spawn should have depth 2"
    );
}

#[test]
fn test_create_without_spawning_fields() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["create", "Regular ticket"]);
    let id = output.trim();

    let output = janus.run_success(&["show", id]);

    // Spawning fields should not be present
    assert!(
        !output.contains("spawned-from"),
        "Regular ticket should not have spawned-from"
    );
    assert!(
        !output.contains("spawn-context"),
        "Regular ticket should not have spawn-context"
    );
    assert!(
        !output.contains("depth"),
        "Regular ticket should not have depth"
    );
}

#[test]
fn test_create_spawned_from_nonexistent_parent() {
    let janus = JanusTest::new();

    // Create a ticket spawned from a non-existent parent should fail
    let stderr = janus.run_failure(&["create", "Orphan ticket", "--spawned-from", "j-nonexistent"]);
    assert!(
        stderr.contains("not found"),
        "Should fail with 'not found' when spawned-from references a nonexistent ticket, got: {stderr}"
    );
}

#[test]
fn test_create_spawned_from_partial_id_resolves() {
    let janus = JanusTest::new();

    // Create a parent ticket
    let parent_id = janus
        .run_success(&["create", "Parent ticket"])
        .trim()
        .to_string();

    // Use a partial ID (first few chars) to create a child
    // The full ID is e.g. "j-a1b2", use enough to be unambiguous
    let partial_id = &parent_id[..parent_id.len().min(4)];
    let child_id = janus
        .run_success(&["create", "Child via partial", "--spawned-from", partial_id])
        .trim()
        .to_string();

    let child_output = janus.run_success(&["show", &child_id]);
    // The resolved spawned-from should be the full canonical parent ID, not the partial
    assert!(
        child_output.contains(&format!("spawned-from: {parent_id}")),
        "Should resolve partial ID to canonical full ID in spawned-from, got: {child_output}"
    );
    assert!(
        child_output.contains("depth: 1"),
        "Child should have depth: 1"
    );
}

#[test]
fn test_create_spawned_with_other_options() {
    let janus = JanusTest::new();

    // Create a parent
    let parent_id = janus.run_success(&["create", "Parent"]).trim().to_string();

    // Create spawned ticket with other options
    let child_id = janus
        .run_success(&[
            "create",
            "Spawned bug",
            "--spawned-from",
            &parent_id,
            "--type",
            "bug",
            "--priority",
            "0",
            "--description",
            "Fix critical issue",
        ])
        .trim()
        .to_string();

    let child_output = janus.run_success(&["show", &child_id]);
    assert!(child_output.contains(&format!("spawned-from: {parent_id}")));
    assert!(child_output.contains("type: bug"));
    assert!(child_output.contains("priority: 0"));
    assert!(child_output.contains("Fix critical issue"));
    assert!(child_output.contains("depth: 1"));
}

#[test]
fn test_set_parent_self_reference() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    let stderr = janus.run_failure(&["set", &id, "parent", &id]);
    assert!(stderr.contains("cannot be its own parent"));
}

#[test]
fn test_set_immutable_id_field_fails() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    let stderr = janus.run_failure(&["set", &id, "id", "new-id"]);
    assert!(stderr.contains("invalid field"));
}

#[test]
fn test_set_immutable_uuid_field_fails() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    let stderr = janus.run_failure(&["set", &id, "uuid", "new-uuid"]);
    assert!(stderr.contains("invalid field"));
}

#[test]
fn test_ls_next_in_plan_phased() {
    let janus = JanusTest::new();

    // Create tickets
    let t1_id = janus
        .run_success(&["create", "Phase 1 Task"])
        .trim()
        .to_string();
    let t2_id = janus
        .run_success(&["create", "Phase 2 Task"])
        .trim()
        .to_string();

    // Create a phased plan and capture the ID from JSON output
    let plan_output = janus.run_success(&[
        "plan",
        "create",
        "Phased Plan",
        "--phase",
        "Phase 1",
        "--phase",
        "Phase 2",
        "--json",
    ]);
    let plan_json: serde_json::Value =
        serde_json::from_str(&plan_output).expect("Plan create should output JSON");
    let plan_id = plan_json["id"].as_str().unwrap().to_string();

    // Add tickets to phases
    janus.run_success(&["plan", "add-ticket", &plan_id, &t1_id, "--phase", "1"]);
    janus.run_success(&["plan", "add-ticket", &plan_id, &t2_id, "--phase", "2"]);

    // --next-in-plan should show tickets from incomplete phases
    let output = janus.run_success(&["ls", "--next-in-plan", &plan_id]);
    assert!(output.contains(&t1_id), "Phase 1 task should appear");
    assert!(output.contains(&t2_id), "Phase 2 task should appear");

    // Complete phase 1 ticket
    janus.run_success(&["close", &t1_id, "--no-summary"]);

    let output = janus.run_success(&["ls", "--next-in-plan", &plan_id]);
    assert!(!output.contains(&t1_id), "Completed task should not appear");
    assert!(output.contains(&t2_id), "Phase 2 task should still appear");
}

#[test]
fn test_ls_spawned_from_filter() {
    let janus = JanusTest::new();

    // Create a parent ticket
    let parent_id = janus
        .run_success(&["create", "Parent ticket"])
        .trim()
        .to_string();

    // Create child tickets spawned from the parent
    let child1_id = janus
        .run_success(&["create", "Child 1", "--spawned-from", &parent_id])
        .trim()
        .to_string();
    let child2_id = janus
        .run_success(&["create", "Child 2", "--spawned-from", &parent_id])
        .trim()
        .to_string();

    // Create an unrelated ticket
    let _unrelated_id = janus
        .run_success(&["create", "Unrelated ticket"])
        .trim()
        .to_string();

    // Filter by spawned-from should show only direct children
    let output = janus.run_success(&["ls", "--spawned-from", &parent_id]);
    assert!(output.contains(&child1_id), "Child 1 should appear");
    assert!(output.contains(&child2_id), "Child 2 should appear");
    assert!(
        !output.contains(&parent_id) || output.matches(&parent_id).count() == 0,
        "Parent should not appear in list"
    );
    assert!(
        !output.contains("Unrelated"),
        "Unrelated ticket should not appear"
    );
}

#[test]
fn test_ls_spawned_from_partial_id() {
    let janus = JanusTest::new();

    // Create a parent and child
    let parent_id = janus.run_success(&["create", "Parent"]).trim().to_string();
    let child_id = janus
        .run_success(&["create", "Child", "--spawned-from", &parent_id])
        .trim()
        .to_string();

    // Use partial ID for the filter
    let partial = parent_id.split('-').next_back().unwrap();
    let output = janus.run_success(&["ls", "--spawned-from", partial]);
    assert!(output.contains(&child_id));
}

#[test]
fn test_ls_spawned_from_no_children() {
    let janus = JanusTest::new();

    // Create a parent with no children
    let parent_id = janus
        .run_success(&["create", "Lonely parent"])
        .trim()
        .to_string();

    let output = janus.run_success(&["ls", "--spawned-from", &parent_id]);
    assert!(
        output.trim().is_empty(),
        "Should return empty for parent with no children"
    );
}

#[test]
fn test_ls_spawned_from_nonexistent_fails() {
    let janus = JanusTest::new();

    let output = janus.run(&["ls", "--spawned-from", "nonexistent-id"]);
    assert!(
        !output.status.success(),
        "Should fail for nonexistent parent ticket"
    );
}

#[test]
fn test_ls_depth_zero_shows_root_tickets() {
    let janus = JanusTest::new();

    // Create root tickets (no spawned_from)
    let root1_id = janus.run_success(&["create", "Root 1"]).trim().to_string();
    let root2_id = janus.run_success(&["create", "Root 2"]).trim().to_string();

    // Create child ticket
    let child_id = janus
        .run_success(&["create", "Child", "--spawned-from", &root1_id])
        .trim()
        .to_string();

    // depth 0 should show only root tickets
    let output = janus.run_success(&["ls", "--depth", "0"]);
    assert!(output.contains(&root1_id), "Root 1 should appear");
    assert!(output.contains(&root2_id), "Root 2 should appear");
    assert!(!output.contains(&child_id), "Child should not appear");
}

#[test]
fn test_ls_depth_one_shows_first_level_children() {
    let janus = JanusTest::new();

    // Create hierarchy
    let root_id = janus.run_success(&["create", "Root"]).trim().to_string();
    let child_id = janus
        .run_success(&["create", "Child", "--spawned-from", &root_id])
        .trim()
        .to_string();
    let grandchild_id = janus
        .run_success(&["create", "Grandchild", "--spawned-from", &child_id])
        .trim()
        .to_string();

    // depth 1 should show only first-level children
    let output = janus.run_success(&["ls", "--depth", "1"]);
    assert!(!output.contains(&root_id), "Root should not appear");
    assert!(output.contains(&child_id), "Child should appear");
    assert!(
        !output.contains(&grandchild_id),
        "Grandchild should not appear"
    );
}

#[test]
fn test_ls_max_depth_shows_tickets_up_to_depth() {
    let janus = JanusTest::new();

    // Create hierarchy
    let root_id = janus.run_success(&["create", "Root"]).trim().to_string();
    let child_id = janus
        .run_success(&["create", "Child", "--spawned-from", &root_id])
        .trim()
        .to_string();
    let grandchild_id = janus
        .run_success(&["create", "Grandchild", "--spawned-from", &child_id])
        .trim()
        .to_string();

    // max-depth 1 should show root and children, but not grandchildren
    let output = janus.run_success(&["ls", "--max-depth", "1"]);
    assert!(output.contains(&root_id), "Root should appear");
    assert!(output.contains(&child_id), "Child should appear");
    assert!(
        !output.contains(&grandchild_id),
        "Grandchild should not appear"
    );

    // max-depth 2 should show all
    let output = janus.run_success(&["ls", "--max-depth", "2"]);
    assert!(output.contains(&root_id), "Root should appear");
    assert!(output.contains(&child_id), "Child should appear");
    assert!(output.contains(&grandchild_id), "Grandchild should appear");
}

#[test]
fn test_ls_max_depth_zero_shows_only_roots() {
    let janus = JanusTest::new();

    // Create hierarchy
    let root_id = janus.run_success(&["create", "Root"]).trim().to_string();
    let _child_id = janus
        .run_success(&["create", "Child", "--spawned-from", &root_id])
        .trim()
        .to_string();

    let output = janus.run_success(&["ls", "--max-depth", "0"]);
    assert!(output.contains(&root_id), "Root should appear");
    assert!(!output.contains("Child"), "Child should not appear");
}

#[test]
fn test_ls_next_in_plan_simple() {
    let janus = JanusTest::new();

    // Create tickets
    let t1_id = janus.run_success(&["create", "Task 1"]).trim().to_string();
    let t2_id = janus.run_success(&["create", "Task 2"]).trim().to_string();

    // Create a simple plan and capture the ID from JSON output
    let plan_output = janus.run_success(&["plan", "create", "Test Plan", "--json"]);
    let plan_json: serde_json::Value =
        serde_json::from_str(&plan_output).expect("Plan create should output JSON");
    let plan_id = plan_json["id"].as_str().unwrap().to_string();

    // Add tickets to the plan
    janus.run_success(&["plan", "add-ticket", &plan_id, &t1_id]);
    janus.run_success(&["plan", "add-ticket", &plan_id, &t2_id]);

    // Mark first ticket as complete
    janus.run_success(&["close", &t1_id, "--no-summary"]);

    // --next-in-plan should show only incomplete tickets
    let output = janus.run_success(&["ls", "--next-in-plan", &plan_id]);
    assert!(!output.contains(&t1_id), "Completed task should not appear");
    assert!(output.contains(&t2_id), "Incomplete task should appear");
}

#[test]
fn test_ls_next_in_plan_with_json() {
    let janus = JanusTest::new();

    // Create and set up a plan
    let t1_id = janus.run_success(&["create", "Task"]).trim().to_string();
    let plan_output = janus.run_success(&["plan", "create", "Test Plan", "--json"]);
    let plan_json: serde_json::Value =
        serde_json::from_str(&plan_output).expect("Plan create should output JSON");
    let plan_id = plan_json["id"].as_str().unwrap().to_string();

    janus.run_success(&["plan", "add-ticket", &plan_id, &t1_id]);

    // Test JSON output
    let output = janus.run_success(&["ls", "--next-in-plan", &plan_id, "--json"]);
    let json: serde_json::Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert!(json.is_array());
    assert!(!json.as_array().unwrap().is_empty());
}

#[test]
fn test_ls_next_in_plan_not_found() {
    let janus = JanusTest::new();

    let output = janus.run(&["ls", "--next-in-plan", "nonexistent-plan"]);
    assert!(!output.status.success(), "Should fail for nonexistent plan");
}

#[test]
fn test_ls_spawned_from_with_ready_filter() {
    let janus = JanusTest::new();

    // Create parent and children
    let parent_id = janus.run_success(&["create", "Parent"]).trim().to_string();
    let ready_child_id = janus
        .run_success(&["create", "Ready Child", "--spawned-from", &parent_id])
        .trim()
        .to_string();
    let blocked_child_id = janus
        .run_success(&["create", "Blocked Child", "--spawned-from", &parent_id])
        .trim()
        .to_string();

    // Add dependency to make one child blocked
    let blocker_id = janus.run_success(&["create", "Blocker"]).trim().to_string();
    janus.run_success(&["dep", "add", &blocked_child_id, &blocker_id]);

    // Combine filters: spawned-from AND ready
    let output = janus.run_success(&["ls", "--spawned-from", &parent_id, "--ready"]);
    assert!(
        output.contains(&ready_child_id),
        "Ready child should appear"
    );
    assert!(
        !output.contains("Blocked Child"),
        "Blocked child should not appear"
    );
}

#[test]
fn test_ls_depth_with_status_filter() {
    let janus = JanusTest::new();

    // Create hierarchy with different statuses
    let root_id = janus.run_success(&["create", "Root"]).trim().to_string();
    let child_id = janus
        .run_success(&["create", "Child", "--spawned-from", &root_id])
        .trim()
        .to_string();

    // Close the child
    janus.run_success(&["close", &child_id, "--no-summary"]);

    // depth 1 with status filter should work together
    let output = janus.run_success(&["ls", "--depth", "1", "--status", "complete"]);
    assert!(output.contains(&child_id), "Completed child should appear");

    let output = janus.run_success(&["ls", "--depth", "1", "--status", "new"]);
    assert!(
        !output.contains(&child_id),
        "Completed child should not appear with new filter"
    );
}

#[test]
fn test_ls_next_in_plan_with_limit() {
    let janus = JanusTest::new();

    // Create multiple tickets
    let t1_id = janus.run_success(&["create", "Task 1"]).trim().to_string();
    let t2_id = janus.run_success(&["create", "Task 2"]).trim().to_string();
    let t3_id = janus.run_success(&["create", "Task 3"]).trim().to_string();

    // Create a plan and capture the ID from JSON output
    let plan_output = janus.run_success(&["plan", "create", "Test Plan", "--json"]);
    let plan_json: serde_json::Value =
        serde_json::from_str(&plan_output).expect("Plan create should output JSON");
    let plan_id = plan_json["id"].as_str().unwrap().to_string();

    // Add all tickets
    janus.run_success(&["plan", "add-ticket", &plan_id, &t1_id]);
    janus.run_success(&["plan", "add-ticket", &plan_id, &t2_id]);
    janus.run_success(&["plan", "add-ticket", &plan_id, &t3_id]);

    // With limit 2, should show only 2 tickets
    let output = janus.run_success(&["ls", "--next-in-plan", &plan_id, "--limit", "2"]);
    let line_count = output.lines().count();
    assert_eq!(
        line_count, 2,
        "Should show exactly 2 tickets with --limit 2"
    );
}

#[test]
fn test_ls_sort_by_priority() {
    let janus = JanusTest::new();

    let t1_id = janus
        .run_success(&["create", "Task 1", "--priority", "3"])
        .trim()
        .to_string();
    let t2_id = janus
        .run_success(&["create", "Task 2", "--priority", "1"])
        .trim()
        .to_string();
    let t3_id = janus
        .run_success(&["create", "Task 3", "--priority", "0"])
        .trim()
        .to_string();

    let output = janus.run_success(&["ls", "--sort-by", "priority"]);
    let lines: Vec<&str> = output.lines().collect();

    assert_eq!(lines.len(), 3, "Should show 3 tickets");
    assert!(lines[0].contains(&t3_id), "P0 ticket should be first");
    assert!(lines[1].contains(&t2_id), "P1 ticket should be second");
    assert!(lines[2].contains(&t1_id), "P3 ticket should be third");
}

#[test]
fn test_ls_sort_by_id() {
    let janus = JanusTest::new();

    janus.run_success(&["create", "Zebra"]);
    janus.run_success(&["create", "Alpha"]);
    janus.run_success(&["create", "Middle"]);

    let output = janus.run_success(&["ls", "--sort-by", "id"]);
    let lines: Vec<&str> = output.lines().collect();

    assert_eq!(lines.len(), 3, "Should show 3 tickets");
    let ids: Vec<&str> = lines
        .iter()
        .filter_map(|l| l.split_whitespace().next())
        .collect();

    assert!(ids[0] < ids[1], "IDs should be sorted alphabetically");
    assert!(ids[1] < ids[2], "IDs should be sorted alphabetically");
}

#[test]
fn test_ls_sort_by_created() {
    let janus = JanusTest::new();

    let t1_id = janus.run_success(&["create", "First"]).trim().to_string();

    // Delay to ensure different timestamps (timestamps have second precision)
    std::thread::sleep(std::time::Duration::from_secs(1));

    let t2_id = janus.run_success(&["create", "Second"]).trim().to_string();

    std::thread::sleep(std::time::Duration::from_secs(1));

    let t3_id = janus.run_success(&["create", "Third"]).trim().to_string();

    let output = janus.run_success(&["ls", "--sort-by", "created"]);
    let lines: Vec<&str> = output.lines().collect();

    assert_eq!(lines.len(), 3, "Should show 3 tickets");
    assert!(
        lines[0].contains(&t3_id),
        "Most recent ticket should be first"
    );
    assert!(lines[1].contains(&t2_id), "Middle ticket should be second");
    assert!(lines[2].contains(&t1_id), "Oldest ticket should be last");
}

#[test]
fn test_ls_sort_by_invalid_uses_priority() {
    let janus = JanusTest::new();

    let t1_id = janus
        .run_success(&["create", "Task 1", "--priority", "3"])
        .trim()
        .to_string();
    let t2_id = janus
        .run_success(&["create", "Task 2", "--priority", "0"])
        .trim()
        .to_string();

    let output = janus.run_success(&["ls", "--sort-by", "invalid"]);
    let lines: Vec<&str> = output.lines().collect();

    assert_eq!(lines.len(), 2, "Should show 2 tickets");
    assert!(
        lines[0].contains(&t2_id),
        "P0 ticket should be first (fallback to priority)"
    );
    assert!(
        lines[1].contains(&t1_id),
        "P3 ticket should be second (fallback to priority)"
    );
}

// ============================================================================
// Add-note command tests
// ============================================================================

#[test]
fn test_add_note_empty_string() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    let stderr = janus.run_failure(&["add-note", &id, ""]);

    assert!(stderr.contains("empty"));
}

#[test]
fn test_add_note_whitespace_only() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    let stderr = janus.run_failure(&["add-note", &id, "   \n\t  "]);

    assert!(stderr.contains("empty"));
}

#[test]
fn test_add_note_no_args_rejects_empty() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    // No note text argument and piped empty stdin should be rejected
    let stderr = janus.run_failure(&["add-note", &id]);

    assert!(stderr.contains("empty"));
}

// ============================================================================
// Edit command tests
// ============================================================================

#[test]
fn test_config_show_empty() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["config", "show"]);
    assert!(output.contains("Configuration"));
    assert!(output.contains("not configured"));
}

#[test]
fn test_config_set_default_remote() {
    let janus = JanusTest::new();

    janus.run_success(&["config", "set", "default.remote", "github:myorg/myrepo"]);
    let output = janus.run_success(&["config", "show"]);
    assert!(output.contains("github"));
    assert!(output.contains("myorg"));
}

#[test]
fn test_config_set_linear_default_remote() {
    let janus = JanusTest::new();

    janus.run_success(&["config", "set", "default.remote", "linear:myorg"]);
    let output = janus.run_success(&["config", "show"]);
    assert!(output.contains("linear"));
    assert!(output.contains("myorg"));
}

#[test]
fn test_config_get_not_set() {
    let janus = JanusTest::new();

    let stderr = janus.run_failure(&["config", "get", "github.token"]);
    assert!(stderr.contains("not set"));
}

#[test]
fn test_config_set_invalid_key() {
    let janus = JanusTest::new();

    let stderr = janus.run_failure(&["config", "set", "invalid.key", "value"]);
    assert!(stderr.contains("unknown config key"));
}

#[test]
fn test_config_set_invalid_default_remote_format() {
    let janus = JanusTest::new();

    let stderr = janus.run_failure(&["config", "set", "default.remote", "invalid"]);
    assert!(stderr.contains("invalid") || stderr.contains("format"));
}

#[test]
fn test_config_file_created() {
    let janus = JanusTest::new();

    janus.run_success(&["config", "set", "default.remote", "github:owner/repo"]);

    let config_path = janus.temp_dir.path().join(".janus").join("config.yaml");
    assert!(config_path.exists(), "Config file should be created");

    let content = fs::read_to_string(config_path).unwrap();
    assert!(content.contains("github"));
    assert!(content.contains("owner"));
}

// ============================================================================
// Remote sync command tests (without actual API calls)
// ============================================================================

#[test]
fn test_adopt_invalid_ref() {
    let janus = JanusTest::new();

    let stderr = janus.run_failure(&["remote", "adopt", "invalid"]);
    assert!(stderr.contains("invalid") || stderr.contains("expected"));
}

#[test]
fn test_adopt_with_reserved_prefix_fails() {
    let janus = JanusTest::new();

    let stderr = janus.run_failure(&[
        "remote",
        "adopt",
        "github:test/test/123",
        "--prefix",
        "plan",
    ]);
    assert!(
        stderr.contains("reserved"),
        "Error should mention the prefix is reserved, got: {stderr}"
    );
}

#[test]
fn test_adopt_with_invalid_prefix_characters_fails() {
    let janus = JanusTest::new();

    let stderr = janus.run_failure(&[
        "remote",
        "adopt",
        "github:test/test/123",
        "--prefix",
        "invalid/prefix",
    ]);
    assert!(
        stderr.contains("invalid characters"),
        "Error should mention invalid characters, got: {stderr}"
    );
}

#[test]
fn test_push_not_configured() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    let stderr = janus.run_failure(&["remote", "push", &id]);
    // Should fail due to no default.remote config
    assert!(
        stderr.contains("not configured") || stderr.contains("default.remote"),
        "Should fail due to missing config: {stderr}"
    );
}

#[test]
fn test_remote_link_invalid_ref() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    let stderr = janus.run_failure(&["remote", "link", &id, "invalid"]);
    assert!(stderr.contains("invalid") || stderr.contains("expected"));
}

#[test]
fn test_sync_not_linked() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    let stderr = janus.run_failure(&["remote", "sync", &id]);
    assert!(stderr.contains("not linked"));
}

#[test]
fn test_help_shows_new_commands() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["--help"]);
    assert!(output.contains("remote"), "Should show remote command");
    assert!(output.contains("config"), "Should show config command");
}

// ============================================================================
// Cache command and error handling tests (Phase 6)
// ============================================================================

// ============================================================================
// Completions command tests (Phase 1)
// ============================================================================

#[test]
fn test_completions_bash() {
    let output = Command::new(concat!(env!("CARGO_MANIFEST_DIR"), "/target/debug/janus"))
        .args(["completions", "bash"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("_janus"));
}

#[test]
fn test_completions_zsh() {
    let output = Command::new(concat!(env!("CARGO_MANIFEST_DIR"), "/target/debug/janus"))
        .args(["completions", "zsh"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("#compdef janus"));
}

#[test]
fn test_completions_fish() {
    let output = Command::new(concat!(env!("CARGO_MANIFEST_DIR"), "/target/debug/janus"))
        .args(["completions", "fish"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("complete -c janus"));
}

#[test]
fn test_completions_invalid_shell() {
    let output = Command::new(concat!(env!("CARGO_MANIFEST_DIR"), "/target/debug/janus"))
        .args(["completions", "invalid"])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success());
}

// ============================================================================
// Cache command and error handling tests (Phase 6)
// ============================================================================

#[test]
fn test_cache_basic_workflow() {
    let janus = JanusTest::new();

    let janus_dir = janus.temp_dir.path().join(".janus");
    fs::create_dir_all(janus_dir.join("items")).unwrap();

    let ticket_path = janus_dir.join("items").join("j-a1b2.md");
    let content = r#"---
id: j-a1b2
uuid: 550e8400-e29b-41d4-a716-446655440000
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Test ticket
"#;
    fs::write(&ticket_path, content).unwrap();

    let ticket_path2 = janus_dir.join("items").join("j-c3d4.md");
    let content2 = r#"---
id: j-c3d4
uuid: 550e8400-e29b-41d4-a716-446655440001
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Another ticket
"#;
    fs::write(&ticket_path2, content2).unwrap();

    let output = janus.run_success(&["ls"]);
    assert!(output.contains("j-a1b2"));
    assert!(output.contains("j-c3d4"));

    let output2 = janus.run_success(&["ls"]);
    assert!(output2.contains("j-a1b2"));
    assert!(output2.contains("j-c3d4"));

    let modified_content = content.replace("Test ticket", "Modified ticket");
    std::thread::sleep(std::time::Duration::from_millis(1100));
    fs::write(&ticket_path, modified_content).unwrap();

    let output3 = janus.run_success(&["show", "j-a1b2"]);
    assert!(output3.contains("Modified ticket"));

    let ticket_path3 = janus_dir.join("items").join("j-e5f6.md");
    let content3 = r#"---
id: j-e5f6
uuid: 550e8400-e29b-41d4-a716-446655440002
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# New ticket
"#;
    fs::write(&ticket_path3, content3).unwrap();

    let output4 = janus.run_success(&["ls"]);
    assert!(output4.contains("j-e5f6"));

    fs::remove_file(&ticket_path2).unwrap();
    let output5 = janus.run_success(&["ls"]);
    assert!(!output5.contains("j-c3d4"));
}

#[test]
fn test_cache_status_command() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["cache", "status"]);
    assert!(output.contains("Cache status") || output.contains("not available"));

    let janus_dir = janus.temp_dir.path().join(".janus");
    fs::create_dir_all(janus_dir.join("items")).unwrap();

    let ticket_path = janus_dir.join("items").join("j-test.md");
    let content = r#"---
id: j-test
uuid: 550e8400-e29b-41d4-a716-446655440003
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Test ticket
"#;
    fs::write(&ticket_path, content).unwrap();

    let _ = janus.run(&["ls"]);
    let status_output = janus.run_success(&["cache", "status"]);
    assert!(status_output.contains("Cache status"));
    assert!(status_output.contains("Tickets loaded"));
}

#[test]
fn test_cache_prune_command() {
    let janus = JanusTest::new();

    let janus_dir = janus.temp_dir.path().join(".janus");
    fs::create_dir_all(janus_dir.join("items")).unwrap();

    let ticket_path = janus_dir.join("items").join("j-test.md");
    let content = r#"---
id: j-test
uuid: 550e8400-e29b-41d4-a716-446655440003
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Test ticket
"#;
    fs::write(&ticket_path, content).unwrap();

    let _ = janus.run(&["ls"]);

    // Create an orphaned embedding file
    let emb_dir = janus_dir.join("embeddings");
    fs::create_dir_all(&emb_dir).unwrap();
    fs::write(emb_dir.join("orphaned-key.bin"), b"fake embedding data").unwrap();

    let output = janus.run_success(&["cache", "prune"]);
    assert!(
        output.contains("Pruned") || output.contains("orphaned"),
        "Prune output should mention pruning or orphaned files. Got: {output}"
    );

    let output2 = janus.run_success(&["ls"]);
    assert!(output2.contains("j-test"));
}

#[test]
fn test_cache_rebuild_command() {
    let janus = JanusTest::new();

    let janus_dir = janus.temp_dir.path().join(".janus");
    fs::create_dir_all(janus_dir.join("items")).unwrap();

    let ticket_path = janus_dir.join("items").join("j-test.md");
    let content = r#"---
id: j-test
uuid: 550e8400-e29b-41d4-a716-446655440003
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Test ticket
"#;
    fs::write(&ticket_path, content).unwrap();

    let output = janus.run_success(&["cache", "rebuild"]);
    assert!(output.contains("rebuilt") || output.contains("Cached tickets"));

    let output2 = janus.run_success(&["ls"]);
    assert!(output2.contains("j-test"));
}

#[test]
fn test_cache_corrupted_database() {
    let janus = JanusTest::new();

    let janus_dir = janus.temp_dir.path().join(".janus");
    fs::create_dir_all(janus_dir.join("items")).unwrap();

    let ticket_path = janus_dir.join("items").join("j-test.md");
    let content = r#"---
id: j-test
uuid: 550e8400-e29b-41d4-a716-446655440003
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Test ticket
"#;
    fs::write(&ticket_path, content).unwrap();

    // Write a corrupted file in the embeddings directory to simulate bad state
    let emb_dir = janus_dir.join("embeddings");
    fs::create_dir_all(&emb_dir).unwrap();
    fs::write(emb_dir.join("corrupted.bin"), b"bad data").unwrap();

    // ls should still work since tickets are read from disk, not embeddings
    let stdout = janus.run(&["ls"]).stdout;
    let stdout_str = String::from_utf8_lossy(&stdout);

    assert!(
        stdout_str.contains("j-test"),
        "ls should still show tickets even with corrupted embeddings. stdout: {stdout_str}"
    );
}

#[test]
fn test_cache_rebuild_after_corruption() {
    let janus = JanusTest::new();

    let janus_dir = janus.temp_dir.path().join(".janus");
    fs::create_dir_all(janus_dir.join("items")).unwrap();

    let ticket_path = janus_dir.join("items").join("j-test.md");
    let content = r#"---
id: j-test
uuid: 550e8400-e29b-41d4-a716-446655440003
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Test ticket
"#;
    fs::write(&ticket_path, content).unwrap();

    let _ = janus.run(&["ls"]);

    // Write corrupted data in the embeddings directory
    let emb_dir = janus_dir.join("embeddings");
    fs::create_dir_all(&emb_dir).unwrap();
    fs::write(emb_dir.join("corrupted.bin"), b"corrupted data").unwrap();

    let output = janus.run_success(&["cache", "rebuild"]);
    assert!(output.contains("rebuilt"));

    let stdout = janus.run_success(&["ls"]);
    assert!(stdout.contains("j-test"));
}

#[test]
fn test_cache_no_directory_works_without_cache() {
    let janus = JanusTest::new();

    let janus_dir = janus.temp_dir.path().join(".janus");
    fs::create_dir_all(janus_dir.join("items")).unwrap();

    let ticket_path = janus_dir.join("items").join("j-test.md");
    let content = r#"---
id: j-test
uuid: 550e8400-e29b-41d4-a716-446655440003
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Test ticket
"#;
    fs::write(&ticket_path, content).unwrap();

    // Delete the embeddings directory if it exists
    let emb_dir = janus_dir.join("embeddings");
    if emb_dir.exists() {
        fs::remove_dir_all(&emb_dir).ok();
    }

    // ls should still work without any embeddings directory
    let stdout1 = janus.run(&["ls"]).stdout;
    let stdout1_str = String::from_utf8_lossy(&stdout1);
    assert!(stdout1_str.contains("j-test"));
}

#[test]
fn test_cache_unavailable_degrades_gracefully() {
    let janus = JanusTest::new();

    let janus_dir = janus.temp_dir.path().join(".janus");
    fs::create_dir_all(janus_dir.join("items")).unwrap();

    let ticket_path = janus_dir.join("items").join("j-test.md");
    let content = r#"---
id: j-test
uuid: 550e8400-e29b-41d4-a716-446655440003
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Test ticket
"#;
    fs::write(&ticket_path, content).unwrap();

    // Write corrupt data to embeddings directory
    let emb_dir = janus_dir.join("embeddings");
    fs::create_dir_all(&emb_dir).unwrap();
    let corrupt_content = vec![0xFF, 0xFE, 0xFD, 0xFC, 0xFB, 0xFA];
    fs::write(emb_dir.join("corrupt.bin"), &corrupt_content).unwrap();

    let output = janus.run_success(&["show", "j-test"]);
    assert!(output.contains("Test ticket"));
}

// ============================================================================
// Completion Summary tests
// ============================================================================

#[test]
fn test_show_displays_completion_summary() {
    let janus = JanusTest::new();

    // Create a ticket with a completion summary section
    let content = r#"---
id: j-done
uuid: 550e8400-e29b-41d4-a716-446655440004
status: complete
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Completed Task

This task has been completed.

## Completion Summary

Successfully implemented the feature with the following changes:
- Added new parser logic
- Updated cache schema
- All tests passing
"#;
    janus.write_ticket("j-done", content);

    let output = janus.run_success(&["show", "j-done"]);

    // The show command displays raw content, so completion summary should be visible
    assert!(output.contains("## Completion Summary"));
    assert!(output.contains("Successfully implemented the feature"));
    assert!(output.contains("Added new parser logic"));
}

#[test]
fn test_completion_summary_in_cache() {
    let janus = JanusTest::new();

    // Create a ticket with a completion summary
    let content = r#"---
id: j-cached
uuid: 550e8400-e29b-41d4-a716-446655440005
status: complete
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Cached Task

Description.

## Completion Summary

Task completed successfully.
"#;
    janus.write_ticket("j-cached", content);

    // Run ls to populate the cache
    janus.run_success(&["ls"]);

    // Show should still work and display the completion summary
    let output = janus.run_success(&["show", "j-cached"]);
    assert!(output.contains("## Completion Summary"));
    assert!(output.contains("Task completed successfully"));
}

// ============================================================================
// Plan command tests
// ============================================================================

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
    assert!(content.contains(&format!("id: {id}")));
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
    assert!(output.contains(&format!("id: {id}")));
    assert!(output.contains("# Raw Test Plan"));
}

#[test]
fn test_plan_show_with_tickets() {
    let janus = JanusTest::new();

    // Create tickets with known IDs
    let ticket1_content = r#"---
id: j-task1
uuid: 550e8400-e29b-41d4-a716-446655440006
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
uuid: 550e8400-e29b-41d4-a716-446655440007
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
    janus.write_plan("plan-test", content);

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
uuid: 550e8400-e29b-41d4-a716-446655440008
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
uuid: 550e8400-e29b-41d4-a716-446655440009
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
uuid: 550e8400-e29b-41d4-a716-44665544000a
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
    janus.write_plan("plan-phased", plan_content);

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
    janus.write_plan("plan-missing", content);

    let output = janus.run_success(&["plan", "show", "plan-missing"]);
    assert!(output.contains("[missing]"));
    assert!(output.contains("j-nonexistent"));
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
fn test_plan_ls_status_filter() {
    let janus = JanusTest::new();

    // Create a plan with completed tickets
    let ticket_content = r#"---
id: j-done2
uuid: 550e8400-e29b-41d4-a716-44665544000b
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
    janus.write_plan("plan-complete", complete_plan);

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
fn test_plan_not_found() {
    let janus = JanusTest::new();

    let output = janus.run_failure(&["plan", "show", "nonexistent-plan"]);
    assert!(output.contains("not found"));
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
    janus.write_plan("plan-freeform", content);

    // Create the referenced ticket
    let ticket_content = r#"---
id: j-test1
uuid: 550e8400-e29b-41d4-a716-44665544000c
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
fn test_plan_add_ticket_simple() {
    let janus = JanusTest::new();

    // Create a simple plan
    let plan_id = janus
        .run_success(&["plan", "create", "Simple Plan"])
        .trim()
        .to_string();

    // Create a ticket
    let ticket_id = janus
        .run_success(&["create", "Test Ticket"])
        .trim()
        .to_string();

    // Add ticket to plan
    let output = janus.run_success(&["plan", "add-ticket", &plan_id, &ticket_id]);
    assert!(output.contains("Added"));
    assert!(output.contains(&ticket_id));

    // Verify ticket is in plan
    let content = janus.read_plan(&plan_id);
    assert!(content.contains(&ticket_id));
}

#[test]
fn test_plan_add_ticket_phased() {
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

    // Create a ticket
    let ticket_id = janus
        .run_success(&["create", "Test Ticket"])
        .trim()
        .to_string();

    // Add ticket to phase
    let output = janus.run_success(&[
        "plan",
        "add-ticket",
        &plan_id,
        &ticket_id,
        "--phase",
        "Phase One",
    ]);
    assert!(output.contains("Added"));

    // Verify ticket is in plan
    let content = janus.read_plan(&plan_id);
    assert!(content.contains(&ticket_id));
}

#[test]
fn test_plan_add_ticket_requires_phase_for_phased_plan() {
    let janus = JanusTest::new();

    // Create a phased plan
    let plan_id = janus
        .run_success(&["plan", "create", "Phased Plan", "--phase", "Phase One"])
        .trim()
        .to_string();

    // Create a ticket
    let ticket_id = janus
        .run_success(&["create", "Test Ticket"])
        .trim()
        .to_string();

    // Try to add ticket without --phase (should fail)
    let output = janus.run_failure(&["plan", "add-ticket", &plan_id, &ticket_id]);
    assert!(output.contains("--phase"));
}

#[test]
fn test_plan_add_ticket_duplicate() {
    let janus = JanusTest::new();

    // Create a simple plan
    let plan_id = janus
        .run_success(&["plan", "create", "Simple Plan"])
        .trim()
        .to_string();

    // Create a ticket
    let ticket_id = janus
        .run_success(&["create", "Test Ticket"])
        .trim()
        .to_string();

    // Add ticket to plan
    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket_id]);

    // Try to add same ticket again (should fail)
    let output = janus.run_failure(&["plan", "add-ticket", &plan_id, &ticket_id]);
    assert!(output.contains("already"));
}

#[test]
fn test_plan_add_ticket_with_position() {
    let janus = JanusTest::new();

    // Create a simple plan
    let plan_id = janus
        .run_success(&["plan", "create", "Simple Plan"])
        .trim()
        .to_string();

    // Create three tickets
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

    // Add ticket1 and ticket3
    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket1]);
    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket3]);

    // Add ticket2 at position 2 (between ticket1 and ticket3)
    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket2, "--position", "2"]);

    // Verify order in plan
    let content = janus.read_plan(&plan_id);
    let pos1 = content.find(&ticket1).unwrap();
    let pos2 = content.find(&ticket2).unwrap();
    let pos3 = content.find(&ticket3).unwrap();
    assert!(pos1 < pos2);
    assert!(pos2 < pos3);
}

#[test]
fn test_plan_remove_ticket() {
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

    // Verify ticket is in plan
    let content = janus.read_plan(&plan_id);
    assert!(content.contains(&ticket_id));

    // Remove ticket
    let output = janus.run_success(&["plan", "remove-ticket", &plan_id, &ticket_id]);
    assert!(output.contains("Removed"));

    // Verify ticket is no longer in plan
    let content = janus.read_plan(&plan_id);
    assert!(!content.contains(&ticket_id));
}

#[test]
fn test_plan_remove_ticket_not_in_plan() {
    let janus = JanusTest::new();

    // Create a simple plan
    let plan_id = janus
        .run_success(&["plan", "create", "Simple Plan"])
        .trim()
        .to_string();

    // Create a ticket but don't add it to the plan
    let ticket_id = janus
        .run_success(&["create", "Test Ticket"])
        .trim()
        .to_string();

    // Try to remove ticket (should fail)
    let output = janus.run_failure(&["plan", "remove-ticket", &plan_id, &ticket_id]);
    assert!(output.contains("not found in plan"));
}

#[test]
fn test_plan_move_ticket() {
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

    // Create and add a ticket to Phase One
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

    // Move ticket to Phase Two
    let output = janus.run_success(&[
        "plan",
        "move-ticket",
        &plan_id,
        &ticket_id,
        "--to-phase",
        "Phase Two",
    ]);
    assert!(output.contains("Moved"));
    assert!(output.contains("Phase Two"));

    // Verify the move - ticket should be after Phase 2 header
    let content = janus.read_plan(&plan_id);
    let phase2_pos = content.find("Phase 2").unwrap();
    let ticket_pos = content.rfind(&ticket_id).unwrap(); // Use rfind to find the last occurrence
    assert!(
        ticket_pos > phase2_pos,
        "Ticket should be after Phase 2 header"
    );
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
fn test_plan_add_phase() {
    let janus = JanusTest::new();

    // Create a simple plan (no phases)
    let plan_id = janus
        .run_success(&["plan", "create", "Simple Plan"])
        .trim()
        .to_string();

    // Add a phase
    let output = janus.run_success(&["plan", "add-phase", &plan_id, "New Phase"]);
    assert!(output.contains("Added phase"));
    assert!(output.contains("New Phase"));

    // Verify phase is in plan
    let content = janus.read_plan(&plan_id);
    assert!(content.contains("New Phase"));
    assert!(content.contains("## Phase"));
}

#[test]
fn test_plan_add_phase_to_phased_plan() {
    let janus = JanusTest::new();

    // Create a phased plan
    let plan_id = janus
        .run_success(&["plan", "create", "Phased Plan", "--phase", "Phase One"])
        .trim()
        .to_string();

    // Add another phase
    janus.run_success(&["plan", "add-phase", &plan_id, "Phase Two"]);

    // Verify both phases are in plan
    let content = janus.read_plan(&plan_id);
    assert!(content.contains("Phase One"));
    assert!(content.contains("Phase Two"));
}

#[test]
fn test_plan_remove_phase_empty() {
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

    // Remove Phase One (empty, should succeed)
    let output = janus.run_success(&["plan", "remove-phase", &plan_id, "Phase One"]);
    assert!(output.contains("Removed"));

    // Verify phase is no longer in plan
    let content = janus.read_plan(&plan_id);
    assert!(!content.contains("Phase One"));
    assert!(content.contains("Phase Two"));
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
fn test_plan_remove_phase_with_force() {
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

    // Remove phase with --force
    let output = janus.run_success(&["plan", "remove-phase", &plan_id, "Phase One", "--force"]);
    assert!(output.contains("Removed"));

    // Verify phase is gone
    let content = janus.read_plan(&plan_id);
    assert!(!content.contains("Phase One"));
}

#[test]
fn test_plan_remove_phase_with_migrate() {
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

    // Create and add a ticket to Phase One
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

    // Remove Phase One with --migrate to Phase Two
    let output = janus.run_success(&[
        "plan",
        "remove-phase",
        &plan_id,
        "Phase One",
        "--migrate",
        "Phase Two",
    ]);
    assert!(output.contains("Migrated"));
    assert!(output.contains("Removed"));

    // Verify ticket is now in Phase Two
    let content = janus.read_plan(&plan_id);
    assert!(!content.contains("Phase One"));
    assert!(content.contains("Phase Two"));
    assert!(content.contains(&ticket_id));
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

#[test]
fn test_plan_add_ticket_with_after() {
    let janus = JanusTest::new();

    // Create a simple plan
    let plan_id = janus
        .run_success(&["plan", "create", "Simple Plan"])
        .trim()
        .to_string();

    // Create three tickets
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

    // Add ticket1 and ticket3
    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket1]);
    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket3]);

    // Add ticket2 after ticket1
    janus.run_success(&[
        "plan",
        "add-ticket",
        &plan_id,
        &ticket2,
        "--after",
        &ticket1,
    ]);

    // Verify order in plan
    let content = janus.read_plan(&plan_id);
    let pos1 = content.find(&ticket1).unwrap();
    let pos2 = content.find(&ticket2).unwrap();
    let pos3 = content.find(&ticket3).unwrap();
    assert!(pos1 < pos2);
    assert!(pos2 < pos3);
}

#[test]
fn test_plan_not_found_for_manipulation() {
    let janus = JanusTest::new();

    // Create a ticket for add-ticket test
    let ticket_id = janus
        .run_success(&["create", "Test Ticket"])
        .trim()
        .to_string();

    // Try to add ticket to non-existent plan
    let output = janus.run_failure(&["plan", "add-ticket", "nonexistent", &ticket_id]);
    assert!(output.contains("not found"));
}

#[test]
fn test_plan_ticket_not_found() {
    let janus = JanusTest::new();

    // Create a simple plan
    let plan_id = janus
        .run_success(&["plan", "create", "Simple Plan"])
        .trim()
        .to_string();

    // Try to add non-existent ticket
    let output = janus.run_failure(&["plan", "add-ticket", &plan_id, "nonexistent-ticket"]);
    assert!(output.contains("not found"));
}

// ============================================================================
// Plan Next command tests
// ============================================================================

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
fn test_plan_status_all_cancelled() {
    let janus = JanusTest::new();

    // Create a plan with cancelled tickets
    let ticket1_content = r#"---
id: j-canc1
uuid: 550e8400-e29b-41d4-a716-44665544000d
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
uuid: 550e8400-e29b-41d4-a716-44665544000e
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
    janus.write_plan("plan-allcanc", plan_content);

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
uuid: 550e8400-e29b-41d4-a716-44665544000f
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
uuid: 550e8400-e29b-41d4-a716-446655440010
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
    janus.write_plan("plan-mixfinish", plan_content);

    let output = janus.run_success(&["plan", "status", "plan-mixfinish"]);
    // Mixed complete/cancelled should show as complete
    assert!(
        output.contains("complete") || output.contains("[complete]"),
        "Mixed complete/cancelled should show as complete"
    );
}

#[test]
fn test_plan_status_all_next() {
    let janus = JanusTest::new();

    // Create tickets with 'next' status
    let ticket1_content = r#"---
id: j-next1
uuid: 550e8400-e29b-41d4-a716-446655440011
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
uuid: 550e8400-e29b-41d4-a716-446655440012
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
    janus.write_plan("plan-allnext", plan_content);

    let output = janus.run_success(&["plan", "status", "plan-allnext"]);
    // All new/next should show as new
    assert!(
        output.contains("new") || output.contains("[new]"),
        "All next tickets should show plan as new"
    );
}

#[test]
fn test_plan_large_many_phases() {
    let janus = JanusTest::new();

    // Create a plan with many phases (10+)
    let mut phases = Vec::new();
    for i in 1..=10 {
        phases.push("--phase".to_string());
        phases.push(format!("Phase {i}"));
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
            content.contains(&format!("Phase {i}")),
            "Should contain Phase {i}"
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
id: j-bulk{i:02}
uuid: 550e8400-e29b-41d4-a716-44665544{i:04x}
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Bulk Task {i}

Description for task {i}.
"#
        );
        janus.write_ticket(&format!("j-bulk{i:02}"), &ticket_content);
        ticket_ids.push(format!("j-bulk{i:02}"));
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

{tickets_list}
"#
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
    janus.write_plan("plan-manymissing", plan_content);

    let output = janus.run_success(&["plan", "show", "plan-manymissing"]);
    // Should show all missing tickets
    assert!(output.contains("[missing]"));
    assert!(output.contains("j-missing1"));
    assert!(output.contains("j-missing2"));
    assert!(output.contains("j-missing3"));
}

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
    janus.write_plan("plan-ac", plan_content);

    let output = janus.run_success(&["plan", "show", "plan-ac"]);
    assert!(output.contains("Acceptance Criteria"));
    assert!(output.contains("First criterion"));
    assert!(output.contains("Second criterion"));
    assert!(output.contains("Third criterion"));
}

#[test]
fn test_plan_phased_with_empty_phase() {
    let janus = JanusTest::new();

    // Create a ticket
    let ticket_content = r#"---
id: j-inphase
uuid: 550e8400-e29b-41d4-a716-446655440014
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
    janus.write_plan("plan-emptyph", plan_content);

    let output = janus.run_success(&["plan", "show", "plan-emptyph"]);
    assert!(output.contains("Phase 1: Has Tickets"));
    assert!(output.contains("Phase 2: Empty Phase"));

    // Status should work with empty phase
    let output = janus.run_success(&["plan", "status", "plan-emptyph"]);
    assert!(output.contains("Phase 1") || output.contains("Has Tickets"));
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
    janus.write_plan("plan-code", plan_content);

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
fn test_plan_add_phase_with_position() {
    let janus = JanusTest::new();

    // Create a phased plan
    let plan_id = janus
        .run_success(&[
            "plan",
            "create",
            "Phased Plan",
            "--phase",
            "First",
            "--phase",
            "Third",
        ])
        .trim()
        .to_string();

    // Add a phase at position 2 (between First and Third)
    janus.run_success(&["plan", "add-phase", &plan_id, "Second", "--position", "2"]);

    let content = janus.read_plan(&plan_id);
    let first_pos = content.find("First").unwrap();
    let second_pos = content.find("Second").unwrap();
    let third_pos = content.find("Third").unwrap();

    assert!(first_pos < second_pos, "First should come before Second");
    assert!(second_pos < third_pos, "Second should come before Third");
}

#[test]
fn test_plan_add_phase_with_after() {
    let janus = JanusTest::new();

    // Create a phased plan
    let plan_id = janus
        .run_success(&[
            "plan",
            "create",
            "Phased Plan",
            "--phase",
            "First",
            "--phase",
            "Third",
        ])
        .trim()
        .to_string();

    // Add a phase after First
    janus.run_success(&["plan", "add-phase", &plan_id, "Second", "--after", "First"]);

    let content = janus.read_plan(&plan_id);
    let first_pos = content.find("First").unwrap();
    let second_pos = content.find("Second").unwrap();
    let third_pos = content.find("Third").unwrap();

    assert!(first_pos < second_pos, "First should come before Second");
    assert!(second_pos < third_pos, "Second should come before Third");
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
fn test_plan_move_ticket_with_position() {
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

    // Add tickets to Phase One
    janus.run_success(&[
        "plan",
        "add-ticket",
        &plan_id,
        &ticket1,
        "--phase",
        "Phase One",
    ]);

    // Add tickets to Phase Two
    janus.run_success(&[
        "plan",
        "add-ticket",
        &plan_id,
        &ticket2,
        "--phase",
        "Phase Two",
    ]);
    janus.run_success(&[
        "plan",
        "add-ticket",
        &plan_id,
        &ticket3,
        "--phase",
        "Phase Two",
    ]);

    // Move ticket1 to Phase Two at position 1
    let output = janus.run_success(&[
        "plan",
        "move-ticket",
        &plan_id,
        &ticket1,
        "--to-phase",
        "Phase Two",
        "--position",
        "1",
    ]);
    assert!(output.contains("Moved"));

    // Verify ticket1 is now first in Phase Two
    let content = janus.read_plan(&plan_id);
    let phase2_pos = content.find("Phase 2").unwrap();
    let t1_after_p2 = content[phase2_pos..].find(&ticket1);
    let t2_after_p2 = content[phase2_pos..].find(&ticket2);

    assert!(
        t1_after_p2.is_some() && t2_after_p2.is_some(),
        "Both tickets should be in Phase 2"
    );
    assert!(
        t1_after_p2.unwrap() < t2_after_p2.unwrap(),
        "Ticket1 should be before Ticket2 in Phase 2"
    );
}

#[test]
fn test_plan_status_with_in_progress_tickets() {
    let janus = JanusTest::new();

    // Create tickets with in_progress status
    let ticket1_content = r#"---
id: j-inprog1
uuid: 550e8400-e29b-41d4-a716-446655440015
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
uuid: 550e8400-e29b-41d4-a716-446655440016
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
    janus.write_plan("plan-inprog", plan_content);

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
uuid: 550e8400-e29b-41d4-a716-446655440017
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
uuid: 550e8400-e29b-41d4-a716-446655440018
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
    janus.write_plan("plan-ph12", plan_content);

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
fn test_plan_show_verbose_phase_shows_full_summary() {
    let janus = JanusTest::new();

    // Create a ticket with a multi-line completion summary
    let ticket_content = r#"---
id: j-verbose
uuid: 550e8400-e29b-41d4-a716-446655440019
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
uuid: 550e8400-e29b-41d4-a716-44665544001a
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
uuid: 550e8400-e29b-41d4-a716-44665544001b
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
        "Should error when using --verbose-phase on simple plan: {error}"
    );
}

#[test]
fn test_plan_show_verbose_phase_nonexistent_phase() {
    let janus = JanusTest::new();

    // Create a ticket
    let ticket_content = r#"---
id: j-test
uuid: 550e8400-e29b-41d4-a716-44665544001c
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
fn test_remote_no_subcommand_non_pty() {
    let output = Command::new(concat!(env!("CARGO_MANIFEST_DIR"), "/target/debug/janus"))
        .args(["remote"])
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stdout}{stderr}");
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

// ============================================================================
// Alias tests (Phase 5)
// ============================================================================

#[test]
fn test_edit_alias() {
    let janus = JanusTest::new();
    let id = janus
        .run_success(&["create", "Test ticket"])
        .trim()
        .to_string();

    let output = janus.run_success(&["e", &id, "--json"]);
    assert!(output.contains(&id));
    assert!(output.contains(".janus"));
}

#[test]
fn test_ls_alias() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["l"]);
    assert!(output.trim().is_empty() || output.contains("No tickets"));
}

// ============================================================
// Plan cache integration tests
// ============================================================

#[test]
fn test_plan_cache_finding_by_partial_id() {
    let janus = JanusTest::new();

    // Create multiple plans
    janus.write_plan(
        "plan-alpha",
        r#"---
id: plan-alpha
uuid: 10000000-0000-0000-0000-000000000001
created: 2024-01-01T00:00:00Z
---
# Alpha Plan

Description for alpha.
"#,
    );

    janus.write_plan(
        "plan-beta",
        r#"---
id: plan-beta
uuid: 20000000-0000-0000-0000-000000000002
created: 2024-01-01T00:00:00Z
---
# Beta Plan

Description for beta.
"#,
    );

    // Sync the cache to ensure plans are cached
    let _ = janus.run_success(&["cache", "rebuild"]);

    // Find by partial ID should work via cache
    let output = janus.run_success(&["plan", "show", "plan-al"]);
    assert!(output.contains("Alpha Plan"));

    let output = janus.run_success(&["plan", "show", "plan-be"]);
    assert!(output.contains("Beta Plan"));
}

#[test]
fn test_plan_cache_get_all_plans() {
    let janus = JanusTest::new();

    // Create multiple plans
    janus.write_plan(
        "plan-first",
        r#"---
id: plan-first
uuid: 30000000-0000-0000-0000-000000000003
created: 2024-01-01T00:00:00Z
---
# First Plan
"#,
    );

    janus.write_plan(
        "plan-second",
        r#"---
id: plan-second
uuid: 40000000-0000-0000-0000-000000000004
created: 2024-01-01T00:00:00Z
---
# Second Plan
"#,
    );

    janus.write_plan(
        "plan-third",
        r#"---
id: plan-third
uuid: 50000000-0000-0000-0000-000000000005
created: 2024-01-01T00:00:00Z
---
# Third Plan
"#,
    );

    // Sync cache
    let _ = janus.run_success(&["cache", "rebuild"]);

    // List plans should use cache
    let output = janus.run_success(&["plan", "ls"]);
    assert!(output.contains("plan-first"));
    assert!(output.contains("plan-second"));
    assert!(output.contains("plan-third"));
}

#[test]
fn test_plan_cache_fallback_when_missing() {
    let janus = JanusTest::new();

    // Create a plan WITHOUT syncing cache first
    janus.write_plan(
        "plan-fallback",
        r#"---
id: plan-fallback
uuid: 60000000-0000-0000-0000-000000000006
created: 2024-01-01T00:00:00Z
---
# Fallback Plan

This plan should be found even without cache.
"#,
    );

    // Should still work by falling back to filesystem reads
    let output = janus.run_success(&["plan", "show", "plan-fallback"]);
    assert!(output.contains("Fallback Plan"));

    // Partial match should also work via fallback
    let output = janus.run_success(&["plan", "show", "plan-fal"]);
    assert!(output.contains("Fallback Plan"));
}

#[test]
fn test_plan_cache_consistency_with_ticket_cache() {
    let janus = JanusTest::new();

    // Create a ticket
    let ticket_id = janus
        .run_success(&["create", "Test ticket for plan"])
        .trim()
        .to_string();

    // Create a plan that references the ticket
    janus.write_plan(
        "plan-consistency",
        &format!(
            r#"---
id: plan-consistency
uuid: 70000000-0000-0000-0000-000000000007
created: 2024-01-01T00:00:00Z
---
# Consistency Plan

## Tickets

- {ticket_id}
"#
        ),
    );

    // Sync cache
    let _ = janus.run_success(&["cache", "rebuild"]);

    // Find ticket by partial ID (uses cache)
    let partial_id = ticket_id.chars().take(8).collect::<String>();
    let output = janus.run_success(&["show", &partial_id]);
    assert!(output.contains("Test ticket for plan"));

    // Find plan by partial ID (should also use cache)
    let output = janus.run_success(&["plan", "show", "plan-cons"]);
    assert!(output.contains("Consistency Plan"));
    assert!(output.contains(&ticket_id));
}

#[test]
fn test_plan_cache_status_uses_cached_plans() {
    let janus = JanusTest::new();

    // Create tickets
    let ticket1 = janus
        .run_success(&["create", "First ticket"])
        .trim()
        .to_string();
    let ticket2 = janus
        .run_success(&["create", "Second ticket"])
        .trim()
        .to_string();

    // Complete one ticket
    janus.run_success(&["status", &ticket1, "complete"]);

    // Create a plan
    janus.write_plan(
        "plan-status",
        &format!(
            r#"---
id: plan-status
uuid: 80000000-0000-0000-0000-000000000008
created: 2024-01-01T00:00:00Z
---
# Status Test Plan

## Tickets

- {ticket1}
- {ticket2}
"#
        ),
    );

    // Sync cache
    let _ = janus.run_success(&["cache", "rebuild"]);

    // Status command should find plan via cache
    let output = janus.run_success(&["plan", "status", "plan-status"]);
    assert!(output.contains("in_progress") || output.contains("1/2"));
}

#[test]
fn test_plan_cache_ambiguous_partial_id() {
    let janus = JanusTest::new();

    // Create plans with similar prefixes
    janus.write_plan(
        "plan-ambig1",
        r#"---
id: plan-ambig1
uuid: 90000000-0000-0000-0000-000000000009
created: 2024-01-01T00:00:00Z
---
# Ambiguous 1
"#,
    );

    janus.write_plan(
        "plan-ambig2",
        r#"---
id: plan-ambig2
uuid: a0000000-0000-0000-0000-00000000000a
created: 2024-01-01T00:00:00Z
---
# Ambiguous 2
"#,
    );

    // Sync cache
    let _ = janus.run_success(&["cache", "rebuild"]);

    // Ambiguous partial ID should error
    let output = janus.run_failure(&["plan", "show", "plan-amb"]);
    assert!(output.contains("ambiguous") || output.contains("more than one"));
}

#[test]
fn test_plan_cache_not_found() {
    let janus = JanusTest::new();

    // Create one plan
    janus.write_plan(
        "plan-existing",
        r#"---
id: plan-existing
uuid: b0000000-0000-0000-0000-00000000000b
created: 2024-01-01T00:00:00Z
---
# Existing Plan
"#,
    );

    // Sync cache
    let _ = janus.run_success(&["cache", "rebuild"]);

    // Non-existent plan should error
    let output = janus.run_failure(&["plan", "show", "plan-nonexistent"]);
    assert!(output.contains("not found"));
}

// ============================================================================
// Next command tests
// ============================================================================

#[test]
fn test_next_empty_repo() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["next"]);
    assert!(output.contains("No tickets found"));
}

#[test]
fn test_next_single_ticket() {
    let janus = JanusTest::new();

    let id = janus
        .run_success(&["create", "Test ticket"])
        .trim()
        .to_string();

    let output = janus.run_success(&["next"]);
    assert!(output.contains(&id));
    assert!(output.contains("Test ticket"));
    assert!(output.contains("ready"));
}

#[test]
fn test_next_with_dependency() {
    let janus = JanusTest::new();

    // Create dependency chain: dep -> main (P0)
    let dep_id = janus
        .run_success(&["create", "Dependency ticket"])
        .trim()
        .to_string();
    let main_id = janus
        .run_success(&["create", "Main ticket", "-p", "0"])
        .trim()
        .to_string();

    // Add dependency
    janus.run_success(&["dep", "add", &main_id, &dep_id]);

    let output = janus.run_success(&["next"]);

    // Dependency should appear first (it blocks the P0 ticket)
    assert!(output.contains(&dep_id));
    assert!(output.contains(&main_id));
    assert!(output.contains("blocks"));
    assert!(output.contains("blocked"));
}

#[test]
fn test_next_all_complete() {
    let janus = JanusTest::new();

    let id = janus
        .run_success(&["create", "Test ticket"])
        .trim()
        .to_string();
    janus.run_success(&["close", &id, "--no-summary"]);

    let output = janus.run_success(&["next"]);
    assert!(output.contains("All tickets are complete"));
}

#[test]
fn test_next_limit() {
    let janus = JanusTest::new();

    // Create 5 tickets
    for i in 0..5 {
        janus.run_success(&["create", &format!("Ticket {i}")]);
    }

    let output = janus.run_success(&["next", "--limit", "2"]);
    let lines: Vec<&str> = output.lines().collect();
    // Header + separator + 2 data lines = 4 lines
    assert_eq!(lines.len(), 4);
}

#[test]
fn test_next_json_output() {
    let janus = JanusTest::new();

    let id = janus
        .run_success(&["create", "Test ticket"])
        .trim()
        .to_string();

    let output = janus.run_success(&["next", "--json"]);
    let json: serde_json::Value = serde_json::from_str(&output).expect("Should be valid JSON");

    assert!(json.is_array());
    assert_eq!(json.as_array().unwrap().len(), 1);
    assert_eq!(json[0]["id"], id);
    assert_eq!(json[0]["title"], "Test ticket");
    assert_eq!(json[0]["status"], "ready");
    assert_eq!(json[0]["reason"], "ready");
}

#[test]
fn test_next_json_with_dependency() {
    let janus = JanusTest::new();

    let dep_id = janus
        .run_success(&["create", "Dependency"])
        .trim()
        .to_string();
    let main_id = janus
        .run_success(&["create", "Main ticket", "-p", "0"])
        .trim()
        .to_string();

    janus.run_success(&["dep", "add", &main_id, &dep_id]);

    let output = janus.run_success(&["next", "--json"]);
    let json: serde_json::Value = serde_json::from_str(&output).expect("Should be valid JSON");

    assert!(json.is_array());
    assert_eq!(json.as_array().unwrap().len(), 2);

    // First item should be the dependency with "blocks" field
    assert_eq!(json[0]["id"], dep_id);
    assert_eq!(json[0]["reason"], "blocking");
    assert_eq!(json[0]["blocks"], main_id);

    // Second item should be the blocked ticket with "blocked_by" field
    assert_eq!(json[1]["id"], main_id);
    assert_eq!(json[1]["reason"], "target");
    assert!(json[1]["blocked_by"].is_array());
    assert_eq!(json[1]["blocked_by"][0], dep_id);
}

#[test]
fn test_next_priority_ordering() {
    let janus = JanusTest::new();

    // Create P2 ticket first, then P0 ticket
    let p2_id = janus
        .run_success(&["create", "P2 ticket", "-p", "2"])
        .trim()
        .to_string();
    let p0_id = janus
        .run_success(&["create", "P0 ticket", "-p", "0"])
        .trim()
        .to_string();

    let output = janus.run_success(&["next"]);
    let lines: Vec<&str> = output.lines().collect();

    // Skip header lines to find first data line
    let first_data_line = lines
        .iter()
        .find(|l| l.contains(&p0_id) || l.contains(&p2_id));
    assert!(first_data_line.is_some());
    // P0 should appear before P2
    assert!(first_data_line.unwrap().contains(&p0_id));
}

// ============================================================================
// Size field tests
// ============================================================================

#[test]
fn test_create_with_size() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["create", "Test ticket", "--size", "medium"]);
    let id = output.trim();

    assert!(janus.ticket_exists(id), "Ticket file should exist");

    let output = janus.run_success(&["show", id]);
    assert!(
        output.contains("size: medium"),
        "Ticket should have size: medium"
    );
}

#[test]
fn test_create_with_size_alias() {
    let janus = JanusTest::new();

    // Test 'm' alias for medium
    let output = janus.run_success(&["create", "Test ticket with alias", "--size", "m"]);
    let id = output.trim();

    let output = janus.run_success(&["show", id]);
    assert!(
        output.contains("size: medium"),
        "Ticket should have size: medium when using alias 'm'"
    );
}

#[test]
fn test_create_with_all_size_aliases() {
    let janus = JanusTest::new();

    // Test all size aliases
    let test_cases = vec![
        ("xs", "xsmall"),
        ("s", "small"),
        ("m", "medium"),
        ("l", "large"),
        ("xl", "xlarge"),
    ];

    for (alias, expected) in test_cases {
        let output = janus.run_success(&["create", "Test", "--size", alias]);
        let id = output.trim();

        let output = janus.run_success(&["show", id]);
        assert!(
            output.contains(&format!("size: {expected}")),
            "Alias '{alias}' should result in size: {expected}"
        );
    }
}

#[test]
fn test_ls_filter_size_single() {
    let janus = JanusTest::new();

    // Create tickets with different sizes
    let small_id = janus
        .run_success(&["create", "Small ticket", "--size", "small"])
        .trim()
        .to_string();
    let medium_id = janus
        .run_success(&["create", "Medium ticket", "--size", "medium"])
        .trim()
        .to_string();
    let _large_id = janus
        .run_success(&["create", "Large ticket", "--size", "large"])
        .trim()
        .to_string();

    // Filter by small size only
    let output = janus.run_success(&["ls", "--size", "small"]);
    assert!(output.contains(&small_id), "Should show small ticket");
    assert!(
        !output.contains(&medium_id),
        "Should not show medium ticket"
    );
}

#[test]
fn test_ls_filter_size_multiple() {
    let janus = JanusTest::new();

    // Create tickets with different sizes
    let small_id = janus
        .run_success(&["create", "Small ticket", "--size", "small"])
        .trim()
        .to_string();
    let medium_id = janus
        .run_success(&["create", "Medium ticket", "--size", "medium"])
        .trim()
        .to_string();
    let large_id = janus
        .run_success(&["create", "Large ticket", "--size", "large"])
        .trim()
        .to_string();

    // Filter by multiple sizes (small and medium)
    let output = janus.run_success(&["ls", "--size", "small,medium"]);
    assert!(output.contains(&small_id), "Should show small ticket");
    assert!(output.contains(&medium_id), "Should show medium ticket");
    assert!(!output.contains(&large_id), "Should not show large ticket");
}

#[test]
fn test_set_size() {
    let janus = JanusTest::new();

    // Create a ticket without size
    let id = janus
        .run_success(&["create", "Test ticket"])
        .trim()
        .to_string();

    // Verify no size initially
    let output = janus.run_success(&["show", &id]);
    assert!(
        !output.contains("size:"),
        "Ticket should not have size initially"
    );

    // Set size to large
    janus.run_success(&["set", &id, "size", "large"]);

    let output = janus.run_success(&["show", &id]);
    assert!(
        output.contains("size: large"),
        "Ticket should have size: large"
    );
}

#[test]
fn test_set_size_clear() {
    let janus = JanusTest::new();

    // Create a ticket with size
    let id = janus
        .run_success(&["create", "Test ticket", "--size", "medium"])
        .trim()
        .to_string();

    // Verify size is set
    let output = janus.run_success(&["show", &id]);
    assert!(
        output.contains("size: medium"),
        "Ticket should have size: medium"
    );

    // Clear size by not providing a value
    janus.run_success(&["set", &id, "size"]);

    let output = janus.run_success(&["show", &id]);
    assert!(
        !output.contains("size:"),
        "Ticket should not have size after clearing"
    );
}

#[test]
fn test_show_displays_size() {
    let janus = JanusTest::new();

    // Create a ticket with size
    let id = janus
        .run_success(&["create", "Test ticket", "--size", "xlarge"])
        .trim()
        .to_string();

    // Check text output
    let output = janus.run_success(&["show", &id]);
    assert!(
        output.contains("size: xlarge"),
        "Show output should contain size"
    );

    // Check JSON output
    let json_output = janus.run_success(&["show", &id, "--json"]);
    assert!(
        json_output.contains("\"size\":") || json_output.contains("'size':"),
        "JSON output should contain size field"
    );
}

#[test]
fn test_create_invalid_size() {
    let janus = JanusTest::new();

    let stderr = janus.run_failure(&["create", "Test", "--size", "invalid"]);
    assert!(
        stderr.contains("Invalid") && stderr.contains("size"),
        "Error should indicate invalid size input"
    );
}

#[test]
fn test_set_invalid_size() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();

    let stderr = janus.run_failure(&["set", &id, "size", "huge"]);
    assert!(
        stderr.contains("Invalid") || stderr.contains("invalid"),
        "Error should indicate invalid size value"
    );
}

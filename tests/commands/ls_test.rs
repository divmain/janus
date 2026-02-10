#[path = "../common/mod.rs"]
mod common;
use common::JanusTest;

// ============================================================================
// List command tests
// ============================================================================

#[test]
fn test_ls_empty() {
    let janus = JanusTest::new();
    let output = janus.run_success(&["ls"]);
    assert!(output.is_empty() || output.trim().is_empty());
}

#[test]
fn test_ls_basic() {
    let janus = JanusTest::new();

    let id1 = janus
        .run_success(&["create", "Ticket 1"])
        .trim()
        .to_string();
    let id2 = janus
        .run_success(&["create", "Ticket 2"])
        .trim()
        .to_string();

    let output = janus.run_success(&["ls"]);
    assert!(output.contains(&id1));
    assert!(output.contains(&id2));
    assert!(output.contains("Ticket 1"));
    assert!(output.contains("Ticket 2"));
}

#[test]
fn test_ls_status_filter() {
    let janus = JanusTest::new();

    let id1 = janus
        .run_success(&["create", "Open ticket"])
        .trim()
        .to_string();
    let id2 = janus
        .run_success(&["create", "Closed ticket"])
        .trim()
        .to_string();
    janus.run_success(&["close", &id2, "--no-summary"]);

    let output = janus.run_success(&["ls", "--status", "new"]);
    assert!(output.contains(&id1));
    assert!(!output.contains(&id2));

    let output = janus.run_success(&["ls", "--status", "complete"]);
    assert!(!output.contains(&id1));
    assert!(output.contains(&id2));
}

#[test]
fn test_ready_after_dep_closed() {
    let janus = JanusTest::new();

    let dep_id = janus
        .run_success(&["create", "Dependency"])
        .trim()
        .to_string();
    let blocked_id = janus.run_success(&["create", "Blocked"]).trim().to_string();

    janus.run_success(&["dep", "add", &blocked_id, &dep_id]);

    // Initially blocked
    let output = janus.run_success(&["ls", "--ready"]);
    assert!(!output.contains(&blocked_id));

    // Close dependency
    janus.run_success(&["close", &dep_id, "--no-summary"]);

    // Now ready
    let output = janus.run_success(&["ls", "--ready"]);
    assert!(output.contains(&blocked_id));
}

// ============================================================================
// Phase 2: Consolidated ls command tests
// ============================================================================

#[test]
fn test_ls_ready_flag() {
    let janus = JanusTest::new();

    // Create a ticket with no deps (should appear in --ready)
    let ready_id = janus
        .run_success(&["create", "Ready ticket"])
        .trim()
        .to_string();

    // Create a ticket with incomplete dep (should NOT appear in --ready)
    let blocking_id = janus
        .run_success(&["create", "Blocking"])
        .trim()
        .to_string();
    let blocked_id = janus.run_success(&["create", "Blocked"]).trim().to_string();
    janus.run_success(&["dep", "add", &blocked_id, &blocking_id]);

    let output = janus.run_success(&["ls", "--ready"]);

    assert!(output.contains(&ready_id));
    assert!(output.contains("Ready ticket"));
    assert!(!output.contains(&blocked_id));
    assert!(!output.contains("Blocked ticket"));
}

#[test]
fn test_ls_blocked_flag() {
    let janus = JanusTest::new();

    let dep_id = janus
        .run_success(&["create", "Dependency"])
        .trim()
        .to_string();
    let blocked_id = janus
        .run_success(&["create", "Blocked ticket"])
        .trim()
        .to_string();
    let ready_id = janus.run_success(&["create", "Ready"]).trim().to_string();

    janus.run_success(&["dep", "add", &blocked_id, &dep_id]);

    let output = janus.run_success(&["ls", "--blocked"]);

    // The blocked ticket should appear
    assert!(output.contains(&blocked_id));
    assert!(output.contains("Blocked ticket"));

    // The dependency ticket should not appear as blocked
    assert!(!output.contains("Dependency"));

    // Ready ticket should not appear
    assert!(!output.contains(&ready_id));
    assert!(!output.contains("Ready"));
}

#[test]
fn test_ls_closed_flag() {
    let janus = JanusTest::new();

    let open_id = janus.run_success(&["create", "Open"]).trim().to_string();
    let closed_id = janus.run_success(&["create", "Closed"]).trim().to_string();
    janus.run_success(&["close", &closed_id, "--no-summary"]);

    let output = janus.run_success(&["ls", "--closed"]);

    assert!(!output.contains(&open_id));
    assert!(output.contains(&closed_id));
}

#[test]
fn test_ls_closed_with_limit() {
    let janus = JanusTest::new();

    // Create and close 5 tickets
    for i in 0..5 {
        let id = janus
            .run_success(&["create", &format!("Ticket {i}")])
            .trim()
            .to_string();
        janus.run_success(&["close", &id, "--no-summary"]);
    }

    let output = janus.run_success(&["ls", "--closed", "--limit", "2"]);
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines.len(), 2);

    // Without --limit, should default to 20 (or all if less than 20)
    let output = janus.run_success(&["ls", "--closed"]);
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines.len(), 5, "All 5 closed tickets should be shown");
}

#[test]
fn test_ls_ready_and_blocked_flags() {
    let janus = JanusTest::new();

    let ready_id = janus.run_success(&["create", "Ready"]).trim().to_string();
    let dep_id = janus.run_success(&["create", "Dep"]).trim().to_string();
    let blocked_id = janus.run_success(&["create", "Blocked"]).trim().to_string();

    janus.run_success(&["dep", "add", &blocked_id, &dep_id]);

    let output = janus.run_success(&["ls", "--ready", "--blocked"]);

    // Both ready AND blocked tickets should appear (union behavior)
    assert!(output.contains(&ready_id));
    assert!(output.contains("Ready"));
    assert!(output.contains(&blocked_id));
    assert!(output.contains("Blocked"));
}

#[test]
fn test_ls_default_excludes_closed() {
    let janus = JanusTest::new();

    let open_id = janus.run_success(&["create", "Open"]).trim().to_string();
    let closed_id = janus.run_success(&["create", "Closed"]).trim().to_string();
    janus.run_success(&["close", &closed_id, "--no-summary"]);

    // By default, closed tickets should not appear
    let output = janus.run_success(&["ls"]);
    assert!(output.contains(&open_id));
    assert!(!output.contains(&closed_id));

    // Using --closed shows closed tickets
    let output_closed = janus.run_success(&["ls", "--closed"]);
    assert!(!output_closed.contains(&open_id));
    assert!(output_closed.contains(&closed_id));
}

#[test]
fn test_ls_status_conflicts_with_filters() {
    let janus = JanusTest::new();

    // Test --status conflicts with --ready
    let output = janus.run_failure(&["ls", "--status", "new", "--ready"]);
    assert!(output.contains("cannot be used with") || output.contains("conflicts"));

    // Test --status conflicts with --blocked
    let output = janus.run_failure(&["ls", "--status", "new", "--blocked"]);
    assert!(output.contains("cannot be used with") || output.contains("conflicts"));

    // Test --status conflicts with --closed
    let output = janus.run_failure(&["ls", "--status", "new", "--closed"]);
    assert!(output.contains("cannot be used with") || output.contains("conflicts"));
}

#[test]
fn test_removed_ls_commands_fail() {
    let janus = JanusTest::new();

    // Test that removed standalone commands (ready, blocked, closed) all fail
    let removed_commands = vec!["ready", "blocked", "closed"];

    for cmd in removed_commands {
        let output = janus.run(&[cmd]);
        assert!(
            !output.status.success(),
            "{cmd} command should fail but succeeded"
        );
    }
}

#[test]
fn test_ls_limit_without_closed() {
    let janus = JanusTest::new();

    // Create more tickets than the limit
    for i in 0..10 {
        janus.run_success(&["create", &format!("Ticket {i}")]);
    }

    // Test that --limit now works universally
    let output = janus.run_success(&["ls", "--limit", "3"]);
    let line_count = output.lines().count();
    assert_eq!(
        line_count, 3,
        "Should show exactly 3 tickets when --limit 3 is used"
    );
}

#[test]
fn test_ls_unlimited_without_limit_flag() {
    let janus = JanusTest::new();

    // Create 5 tickets
    for i in 0..5 {
        janus.run_success(&["create", &format!("Ticket {i}")]);
    }

    // Without --limit, should show all tickets
    let output = janus.run_success(&["ls"]);
    let line_count = output.lines().count();
    assert_eq!(
        line_count, 5,
        "Should show all 5 tickets when --limit is not specified"
    );
}

#[test]
fn test_ls_limit_with_ready_flag() {
    let janus = JanusTest::new();

    // Create multiple ready tickets
    for i in 0..10 {
        janus.run_success(&["create", &format!("Ticket {i}")]);
    }

    let output = janus.run_success(&["ls", "--ready", "--limit", "3"]);
    let line_count = output.lines().count();
    assert_eq!(
        line_count, 3,
        "Should show exactly 3 ready tickets when --limit 3 is used with --ready"
    );
}

#[test]
fn test_ls_limit_with_blocked_flag() {
    let janus = JanusTest::new();

    // Create multiple blocked tickets
    let dep = janus.run_success(&["create", "Dep"]).trim().to_string();
    for i in 0..5 {
        let blocked = janus
            .run_success(&["create", &format!("Blocked {i}")])
            .trim()
            .to_string();
        janus.run_success(&["dep", "add", &blocked, &dep]);
    }

    let output = janus.run_success(&["ls", "--blocked", "--limit", "2"]);
    let line_count = output.lines().count();
    assert_eq!(
        line_count, 2,
        "Should show exactly 2 blocked tickets when --limit 2 is used with --blocked"
    );
}

#[test]
fn test_ls_closed_default_limit() {
    let janus = JanusTest::new();

    // Create and close 30 tickets
    for i in 0..30 {
        let id = janus
            .run_success(&["create", &format!("Ticket {i}")])
            .trim()
            .to_string();
        janus.run_success(&["close", &id, "--no-summary"]);
    }

    // --closed without explicit --limit should show all tickets (no implicit limit)
    let output = janus.run_success(&["ls", "--closed"]);
    let line_count = output.lines().count();
    assert_eq!(
        line_count, 30,
        "--closed should show all tickets when no limit is specified"
    );
}

#[test]
fn test_ls_closed_custom_limit() {
    let janus = JanusTest::new();

    // Create and close 30 tickets
    for i in 0..30 {
        let id = janus
            .run_success(&["create", &format!("Ticket {i}")])
            .trim()
            .to_string();
        janus.run_success(&["close", &id, "--no-summary"]);
    }

    // --closed --limit 5 should show 5 tickets
    let output = janus.run_success(&["ls", "--closed", "--limit", "5"]);
    let line_count = output.lines().count();
    assert_eq!(
        line_count, 5,
        "--closed --limit 5 should show exactly 5 tickets"
    );
}

#[test]
fn test_ls_closed_with_status_filter() {
    let janus = JanusTest::new();

    let open_id = janus
        .run_success(&["create", "Open ticket"])
        .trim()
        .to_string();
    let closed_id = janus
        .run_success(&["create", "Closed ticket"])
        .trim()
        .to_string();
    janus.run_success(&["close", &closed_id, "--no-summary"]);

    // --closed shows only closed tickets
    let output = janus.run_success(&["ls", "--closed"]);
    assert!(!output.contains(&open_id));
    assert!(output.contains(&closed_id));

    // --status complete shows only complete tickets
    let output = janus.run_success(&["ls", "--status", "complete"]);
    assert!(!output.contains(&open_id));
    assert!(output.contains(&closed_id));
}

#[test]
fn test_ls_all_three_filters_union() {
    let janus = JanusTest::new();

    // Create a ready ticket (new, no deps)
    let ready_id = janus.run_success(&["create", "Ready"]).trim().to_string();

    // Create a blocked ticket (new, has incomplete dep)
    let dep_id = janus.run_success(&["create", "Dep"]).trim().to_string();
    let blocked_id = janus.run_success(&["create", "Blocked"]).trim().to_string();
    janus.run_success(&["dep", "add", &blocked_id, &dep_id]);

    // Create a closed ticket
    let closed_id = janus.run_success(&["create", "Closed"]).trim().to_string();
    janus.run_success(&["close", &closed_id, "--no-summary"]);

    // Combine all three filters - should show union of all
    let output = janus.run_success(&["ls", "--ready", "--blocked", "--closed"]);
    assert!(output.contains(&ready_id));
    assert!(output.contains(&blocked_id));
    assert!(output.contains(&closed_id));
    assert!(output.contains("Ready"));
    assert!(output.contains("Blocked"));
    assert!(output.contains("Closed"));
}

#[test]
fn test_ls_json_output_works() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();

    // Test --json flag with regular ls
    let output = janus.run_success(&["ls", "--json"]);
    assert!(output.contains(&id));
    // JSON output should have status field in format "status": "new"
    assert!(output.contains("\"status\":"));
    assert!(output.contains("\"new\""));

    // Test --json flag with --ready
    let output = janus.run_success(&["ls", "--ready", "--json"]);
    assert!(output.contains(&id));
    assert!(output.contains("\"status\":"));
    assert!(output.contains("\"new\""));

    // Test --json flag with --blocked
    let _output = janus.run_success(&["ls", "--blocked", "--json"]);
    // No blocked tickets, so should be empty or just contain the ready one
}

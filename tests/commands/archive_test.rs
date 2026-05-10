#[path = "../common/mod.rs"]
mod common;
use common::JanusTest;

// ============================================================================
// Archive command tests
// ============================================================================

#[test]
fn test_archive_no_tickets() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["archive", "--days", "7"]);
    assert!(
        output.contains("No tickets older than 7 day(s) to archive."),
        "Expected 'no tickets' message, got: {output}"
    );
}

#[test]
fn test_archive_dry_run() {
    let janus = JanusTest::new();

    // Create a ticket and close it so it's in Complete status
    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    janus.run_success(&["close", &id, "--no-summary"]);

    // --dry-run --days 0 should report disabled (days=0 disables archiving
    // before the dry-run logic is reached)
    let output = janus.run_success(&["archive", "--dry-run", "--days", "0"]);
    assert!(
        output.contains("Auto-archive is disabled"),
        "Expected disabled message with --days 0, got: {output}"
    );

    // --dry-run --days 1: the ticket was just created, so it's < 1 day old
    // and should not appear as a candidate
    let output = janus.run_success(&["archive", "--dry-run", "--days", "1"]);
    assert!(
        output.contains("No tickets are older than 1 day(s)."),
        "Expected no-candidates message, got: {output}"
    );
}

#[test]
fn test_archive_json_output() {
    let janus = JanusTest::new();

    // JSON output for the disabled path (--days 0)
    let output = janus.run_success(&["archive", "--json", "--days", "0"]);
    let json: serde_json::Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert_eq!(json["disabled"], true, "JSON should contain disabled: true");
    assert_eq!(json["days"], 0, "JSON should contain days: 0");
    assert!(
        json["archived"].as_array().unwrap().is_empty(),
        "archived should be empty array"
    );
    assert_eq!(
        json["dry_run"], false,
        "dry_run should be false when --dry-run is not passed"
    );
}

#[test]
fn test_archive_with_completed_ticket() {
    let janus = JanusTest::new();

    // Create and complete a ticket
    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    janus.run_success(&["close", &id, "--no-summary"]);

    // With --days 0, archiving is disabled regardless of ticket state
    let output = janus.run_success(&["archive", "--days", "0"]);
    assert!(
        output.contains("Auto-archive is disabled"),
        "Expected disabled message, got: {output}"
    );

    // Verify the ticket is still in complete status (not archived)
    let show_output = janus.run_success(&["show", &id]);
    assert!(
        show_output.contains("status: complete"),
        "Ticket should remain complete when archive is disabled"
    );
}

#[test]
fn test_archive_status_shows_archived() {
    let janus = JanusTest::new();

    // Create a ticket and set it to archived status directly
    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    janus.run_success(&["status", &id, "complete"]);
    janus.run_success(&["status", &id, "archived"]);

    // Verify show displays archived status
    let output = janus.run_success(&["show", &id]);
    assert!(
        output.contains("status: archived"),
        "Expected archived status in show output, got: {output}"
    );
}

#[test]
fn test_archive_json_dry_run_no_candidates() {
    let janus = JanusTest::new();

    // Create and complete a ticket
    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    janus.run_success(&["close", &id, "--no-summary"]);

    // JSON dry-run with --days 1: brand-new ticket is not old enough
    let output = janus.run_success(&["archive", "--json", "--dry-run", "--days", "1"]);
    let json: serde_json::Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert_eq!(json["dry_run"], true, "dry_run should be true");
    assert_eq!(json["days"], 1);
    assert!(
        json["candidates"].as_array().unwrap().is_empty(),
        "No candidates expected for a brand-new ticket"
    );
    assert!(
        json["archived"].as_array().unwrap().is_empty(),
        "archived should be empty in dry-run"
    );
}

#[test]
fn test_archive_json_dry_run_with_old_ticket() {
    let janus = JanusTest::new();

    // Write a ticket file directly with a completed-at timestamp far in the past.
    // The uuid field is required by the strict frontmatter parser.
    janus.write_ticket(
        "t-old1",
        "\
---
id: t-old1
uuid: 00000000-0000-0000-0000-000000000001
status: complete
type: task
priority: 2
created: 2020-01-01T00:00:00Z
completed-at: 2020-01-01T00:00:00Z
---
# Old completed ticket
",
    );

    // Dry-run with --days 1: the ticket is years old, so it should be a candidate
    let output = janus.run_success(&["archive", "--json", "--dry-run", "--days", "1"]);
    let json: serde_json::Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert_eq!(json["dry_run"], true);
    let candidates = json["candidates"].as_array().unwrap();
    assert!(
        candidates.iter().any(|c| c.as_str() == Some("t-old1")),
        "Expected t-old1 in candidates, got: {candidates:?}"
    );
    // Dry-run should not actually archive
    assert!(
        json["archived"].as_array().unwrap().is_empty(),
        "archived should be empty in dry-run"
    );

    // Verify the ticket is still complete (not archived)
    let show_output = janus.run_success(&["show", "t-old1"]);
    assert!(
        show_output.contains("status: complete"),
        "Ticket should still be complete after dry-run"
    );
}

#[test]
fn test_archive_sweeps_old_ticket() {
    let janus = JanusTest::new();

    // Write a ticket file directly with a completed-at timestamp far in the past
    janus.write_ticket(
        "t-old2",
        "\
---
id: t-old2
uuid: 00000000-0000-0000-0000-000000000002
status: complete
type: task
priority: 2
created: 2020-01-01T00:00:00Z
completed-at: 2020-01-01T00:00:00Z
---
# Old completed ticket
",
    );

    // Actually archive with --days 1: the ticket is years old
    let output = janus.run_success(&["archive", "--days", "1"]);
    assert!(
        output.contains("Archived 1 ticket(s)"),
        "Expected archived message, got: {output}"
    );
    assert!(
        output.contains("t-old2"),
        "Expected ticket ID in output, got: {output}"
    );

    // Verify the ticket is now archived
    let show_output = janus.run_success(&["show", "t-old2"]);
    assert!(
        show_output.contains("status: archived"),
        "Ticket should be archived after sweep"
    );
}

#[test]
fn test_archive_skips_non_complete_tickets() {
    let janus = JanusTest::new();

    // Write tickets in various non-complete statuses with old timestamps.
    // Even though these are old, archive only sweeps Complete tickets.
    janus.write_ticket(
        "t-new1",
        "\
---
id: t-new1
uuid: 00000000-0000-0000-0000-000000000010
status: new
type: task
priority: 2
created: 2020-01-01T00:00:00Z
---
# New ticket
",
    );

    janus.write_ticket(
        "t-prog",
        "\
---
id: t-prog
uuid: 00000000-0000-0000-0000-000000000011
status: in_progress
type: task
priority: 2
created: 2020-01-01T00:00:00Z
---
# In-progress ticket
",
    );

    janus.write_ticket(
        "t-canc",
        "\
---
id: t-canc
uuid: 00000000-0000-0000-0000-000000000012
status: cancelled
type: task
priority: 2
created: 2020-01-01T00:00:00Z
---
# Cancelled ticket
",
    );

    // Run archive with --days 1
    let output = janus.run_success(&["archive", "--days", "1"]);
    assert!(
        output.contains("No tickets older than 1 day(s) to archive."),
        "Should not archive non-complete tickets, got: {output}"
    );
}

#[test]
fn test_archive_only_archives_old_enough_tickets() {
    let janus = JanusTest::new();

    // Write an old completed ticket
    janus.write_ticket(
        "t-old3",
        "\
---
id: t-old3
uuid: 00000000-0000-0000-0000-000000000003
status: complete
type: task
priority: 2
created: 2020-01-01T00:00:00Z
completed-at: 2020-01-01T00:00:00Z
---
# Old completed ticket
",
    );

    // Create a fresh completed ticket via the CLI (will be < 1 day old)
    let fresh_id = janus.run_success(&["create", "Fresh"]).trim().to_string();
    janus.run_success(&["close", &fresh_id, "--no-summary"]);

    // Archive with --days 1: only the old ticket should be archived
    let output = janus.run_success(&["archive", "--days", "1"]);
    assert!(
        output.contains("Archived 1 ticket(s)"),
        "Expected exactly 1 archived ticket, got: {output}"
    );
    assert!(
        output.contains("t-old3"),
        "Expected old ticket in output, got: {output}"
    );

    // Old ticket should be archived
    let show_old = janus.run_success(&["show", "t-old3"]);
    assert!(show_old.contains("status: archived"));

    // Fresh ticket should still be complete
    let show_fresh = janus.run_success(&["show", &fresh_id]);
    assert!(
        show_fresh.contains("status: complete"),
        "Fresh ticket should remain complete"
    );
}

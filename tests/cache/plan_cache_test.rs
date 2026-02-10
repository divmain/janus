#[path = "../common/mod.rs"]
mod common;

use common::JanusTest;

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

- {}
"#,
            ticket_id
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

- {}
- {}
"#,
            ticket1, ticket2
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

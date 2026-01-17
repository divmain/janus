#[path = "../common/mod.rs"]
mod common;
use common::JanusTest;
use serial_test::serial;

// ============================================================================
// Alias tests
// ============================================================================

#[test]
#[serial]
fn test_create_alias() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["c", "Test ticket"]);
    let id = output.trim();
    assert!(janus.ticket_exists(id));
}

#[test]
#[serial]
fn test_show_alias() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    let output = janus.run_success(&["s", &id]);
    assert!(output.contains("# Test"));
}

// ============================================================================
// Priority sorting tests
// ============================================================================

#[test]
#[serial]
fn test_ready_sorted_by_priority() {
    let janus = JanusTest::new();

    // Create tickets with different priorities
    let id_p4 = janus
        .run_success(&["create", "P4 ticket", "-p", "4"])
        .trim()
        .to_string();
    let id_p0 = janus
        .run_success(&["create", "P0 ticket", "-p", "0"])
        .trim()
        .to_string();
    let id_p2 = janus
        .run_success(&["create", "P2 ticket", "-p", "2"])
        .trim()
        .to_string();

    let output = janus.run_success(&["ls", "--ready"]);
    let lines: Vec<&str> = output.lines().collect();

    // Find positions
    let pos_p0 = lines.iter().position(|l| l.contains(&id_p0));
    let pos_p2 = lines.iter().position(|l| l.contains(&id_p2));
    let pos_p4 = lines.iter().position(|l| l.contains(&id_p4));

    // P0 should come before P2 which should come before P4
    assert!(pos_p0 < pos_p2, "P0 should come before P2");
    assert!(pos_p2 < pos_p4, "P2 should come before P4");
}

// ============================================================================
// Help tests
// ============================================================================

#[test]
#[serial]
fn test_help() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["--help"]);
    assert!(output.contains("Plain-text issue tracking"));
    assert!(output.contains("create"));
    assert!(output.contains("show"));
    assert!(output.contains("dep"));
    assert!(output.contains("link"));
}

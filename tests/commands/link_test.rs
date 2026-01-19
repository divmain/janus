#[path = "../common/mod.rs"]
mod common;
use common::JanusTest;
use serial_test::serial;

// ============================================================================
// Link command tests
// ============================================================================

#[test]
#[serial]
fn test_link_add() {
    let janus = JanusTest::new();

    let id1 = janus
        .run_success(&["create", "Ticket 1"])
        .trim()
        .to_string();
    let id2 = janus
        .run_success(&["create", "Ticket 2"])
        .trim()
        .to_string();

    let output = janus.run_success(&["link", "add", &id1, &id2]);
    assert!(output.contains("Added"));

    // Both tickets should have links
    let content1 = janus.read_ticket(&id1);
    let content2 = janus.read_ticket(&id2);
    assert!(content1.contains(&id2));
    assert!(content2.contains(&id1));
}

#[test]
#[serial]
fn test_link_add_multiple() {
    let janus = JanusTest::new();

    let id1 = janus
        .run_success(&["create", "Ticket 1"])
        .trim()
        .to_string();
    let id2 = janus
        .run_success(&["create", "Ticket 2"])
        .trim()
        .to_string();
    let id3 = janus
        .run_success(&["create", "Ticket 3"])
        .trim()
        .to_string();

    let output = janus.run_success(&["link", "add", &id1, &id2, &id3]);
    assert!(output.contains("Added"));
    assert!(output.contains("3 tickets"));
}

#[test]
#[serial]
fn test_link_add_duplicate() {
    let janus = JanusTest::new();

    let id1 = janus
        .run_success(&["create", "Ticket 1"])
        .trim()
        .to_string();
    let id2 = janus
        .run_success(&["create", "Ticket 2"])
        .trim()
        .to_string();

    janus.run_success(&["link", "add", &id1, &id2]);
    let output = janus.run_success(&["link", "add", &id1, &id2]);
    assert!(output.contains("already exist"));
}

#[test]
#[serial]
fn test_link_remove() {
    let janus = JanusTest::new();

    let id1 = janus
        .run_success(&["create", "Ticket 1"])
        .trim()
        .to_string();
    let id2 = janus
        .run_success(&["create", "Ticket 2"])
        .trim()
        .to_string();

    janus.run_success(&["link", "add", &id1, &id2]);
    let output = janus.run_success(&["link", "remove", &id1, &id2]);
    assert!(output.contains("Removed link"));

    let content1 = janus.read_ticket(&id1);
    let content2 = janus.read_ticket(&id2);
    assert!(content1.contains("links: []"));
    assert!(content2.contains("links: []"));
}

#[test]
#[serial]
fn test_link_remove_not_found() {
    let janus = JanusTest::new();

    let id1 = janus
        .run_success(&["create", "Ticket 1"])
        .trim()
        .to_string();
    let id2 = janus
        .run_success(&["create", "Ticket 2"])
        .trim()
        .to_string();

    let stderr = janus.run_failure(&["link", "remove", &id1, &id2]);
    assert!(stderr.contains("not found"));
}

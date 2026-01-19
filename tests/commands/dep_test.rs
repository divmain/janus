#[path = "../common/mod.rs"]
mod common;
use common::JanusTest;
use serial_test::serial;

// ============================================================================
// Dependency command tests
// ============================================================================

#[test]
#[serial]
fn test_dep_add() {
    let janus = JanusTest::new();

    let id1 = janus
        .run_success(&["create", "Ticket 1"])
        .trim()
        .to_string();
    let id2 = janus
        .run_success(&["create", "Ticket 2"])
        .trim()
        .to_string();

    let output = janus.run_success(&["dep", "add", &id1, &id2]);
    assert!(output.contains("Added dependency"));

    let content = janus.read_ticket(&id1);
    assert!(content.contains(&format!("[\"{}\"]", id2)));
}

#[test]
#[serial]
fn test_dep_add_duplicate() {
    let janus = JanusTest::new();

    let id1 = janus
        .run_success(&["create", "Ticket 1"])
        .trim()
        .to_string();
    let id2 = janus
        .run_success(&["create", "Ticket 2"])
        .trim()
        .to_string();

    janus.run_success(&["dep", "add", &id1, &id2]);
    let output = janus.run_success(&["dep", "add", &id1, &id2]);
    assert!(output.contains("already exists"));
}

#[test]
#[serial]
fn test_dep_remove() {
    let janus = JanusTest::new();

    let id1 = janus
        .run_success(&["create", "Ticket 1"])
        .trim()
        .to_string();
    let id2 = janus
        .run_success(&["create", "Ticket 2"])
        .trim()
        .to_string();

    janus.run_success(&["dep", "add", &id1, &id2]);
    let output = janus.run_success(&["dep", "remove", &id1, &id2]);
    assert!(output.contains("Removed dependency"));

    let content = janus.read_ticket(&id1);
    assert!(content.contains("deps: []"));
}

#[test]
#[serial]
fn test_dep_remove_not_found() {
    let janus = JanusTest::new();

    let id1 = janus
        .run_success(&["create", "Ticket 1"])
        .trim()
        .to_string();
    let id2 = janus
        .run_success(&["create", "Ticket 2"])
        .trim()
        .to_string();

    let stderr = janus.run_failure(&["dep", "remove", &id1, &id2]);
    assert!(stderr.contains("not found"));
}

#[test]
#[serial]
fn test_dep_tree() {
    let janus = JanusTest::new();

    let id1 = janus.run_success(&["create", "Root"]).trim().to_string();
    let id2 = janus.run_success(&["create", "Child 1"]).trim().to_string();
    let id3 = janus.run_success(&["create", "Child 2"]).trim().to_string();

    janus.run_success(&["dep", "add", &id1, &id2]);
    janus.run_success(&["dep", "add", &id1, &id3]);

    let output = janus.run_success(&["dep", "tree", &id1]);
    assert!(output.contains(&id1));
    assert!(output.contains(&id2));
    assert!(output.contains(&id3));
    assert!(output.contains("Root"));
}

#[test]
#[serial]
fn test_dep_tree_full() {
    let janus = JanusTest::new();

    let id1 = janus.run_success(&["create", "Root"]).trim().to_string();
    let id2 = janus.run_success(&["create", "Child"]).trim().to_string();
    let id3 = janus
        .run_success(&["create", "Grandchild"])
        .trim()
        .to_string();

    janus.run_success(&["dep", "add", &id1, &id2]);
    janus.run_success(&["dep", "add", &id2, &id3]);

    let output = janus.run_success(&["dep", "tree", &id1, "--full"]);
    assert!(output.contains(&id1));
    assert!(output.contains(&id2));
    assert!(output.contains(&id3));
}

// ============================================================================
// Circular dependency detection tests
// ============================================================================

#[test]
#[serial]
fn test_dep_add_direct_circular() {
    let janus = JanusTest::new();

    let id1 = janus
        .run_success(&["create", "Ticket A"])
        .trim()
        .to_string();
    let id2 = janus
        .run_success(&["create", "Ticket B"])
        .trim()
        .to_string();

    // Add A -> B (should succeed)
    janus.run_success(&["dep", "add", &id1, &id2]);

    // Try to add B -> A (should fail with circular dependency error)
    let stderr = janus.run_failure(&["dep", "add", &id2, &id1]);
    assert!(stderr.contains("circular dependency"));
    assert!(stderr.contains("direct"));

    // Verify B still has no dependencies
    let content = janus.read_ticket(&id2);
    assert!(content.contains("deps: []"));
}

#[test]
#[serial]
fn test_dep_add_transitive_circular_3_level() {
    let janus = JanusTest::new();

    let id1 = janus
        .run_success(&["create", "Ticket A"])
        .trim()
        .to_string();
    let id2 = janus
        .run_success(&["create", "Ticket B"])
        .trim()
        .to_string();
    let id3 = janus
        .run_success(&["create", "Ticket C"])
        .trim()
        .to_string();

    // Add A -> B (should succeed)
    janus.run_success(&["dep", "add", &id1, &id2]);

    // Add B -> C (should succeed)
    janus.run_success(&["dep", "add", &id2, &id3]);

    // Try to add C -> A (should fail with circular dependency error)
    let stderr = janus.run_failure(&["dep", "add", &id3, &id1]);
    assert!(stderr.contains("circular dependency"));
    assert!(stderr.contains("cycle"));

    // Verify C still has no dependencies
    let content = janus.read_ticket(&id3);
    assert!(content.contains("deps: []"));
}

#[test]
#[serial]
fn test_dep_add_transitive_circular_4_level() {
    let janus = JanusTest::new();

    let id1 = janus
        .run_success(&["create", "Ticket A"])
        .trim()
        .to_string();
    let id2 = janus
        .run_success(&["create", "Ticket B"])
        .trim()
        .to_string();
    let id3 = janus
        .run_success(&["create", "Ticket C"])
        .trim()
        .to_string();
    let id4 = janus
        .run_success(&["create", "Ticket D"])
        .trim()
        .to_string();

    // Create chain: A -> B -> C -> D
    janus.run_success(&["dep", "add", &id1, &id2]);
    janus.run_success(&["dep", "add", &id2, &id3]);
    janus.run_success(&["dep", "add", &id3, &id4]);

    // Try to add D -> A (should fail - creates 4-level cycle)
    let stderr = janus.run_failure(&["dep", "add", &id4, &id1]);
    assert!(stderr.contains("circular dependency"));
    assert!(stderr.contains("cycle"));

    // Verify D still only depends on nothing (we didn't add any deps to D)
    let content = janus.read_ticket(&id4);
    assert!(content.contains("deps: []"));
}

#[test]
#[serial]
fn test_dep_add_valid_non_circular_chain() {
    let janus = JanusTest::new();

    let id1 = janus
        .run_success(&["create", "Ticket A"])
        .trim()
        .to_string();
    let id2 = janus
        .run_success(&["create", "Ticket B"])
        .trim()
        .to_string();
    let id3 = janus
        .run_success(&["create", "Ticket C"])
        .trim()
        .to_string();
    let id4 = janus
        .run_success(&["create", "Ticket D"])
        .trim()
        .to_string();

    // Create valid chain: A -> B -> C and A -> D (no cycles)
    janus.run_success(&["dep", "add", &id1, &id2]);
    janus.run_success(&["dep", "add", &id2, &id3]);
    janus.run_success(&["dep", "add", &id1, &id4]);

    // All should succeed
    let content1 = janus.read_ticket(&id1);
    assert!(content1.contains(&id2));
    assert!(content1.contains(&id4));

    let content2 = janus.read_ticket(&id2);
    assert!(content2.contains(&id3));
}

#[test]
#[serial]
fn test_dep_add_valid_diamond_dependency() {
    let janus = JanusTest::new();

    let id1 = janus
        .run_success(&["create", "Ticket A"])
        .trim()
        .to_string();
    let id2 = janus
        .run_success(&["create", "Ticket B"])
        .trim()
        .to_string();
    let id3 = janus
        .run_success(&["create", "Ticket C"])
        .trim()
        .to_string();
    let id4 = janus
        .run_success(&["create", "Ticket D"])
        .trim()
        .to_string();

    // Create diamond: A -> B -> D and A -> C -> D (no cycles, just converging paths)
    janus.run_success(&["dep", "add", &id1, &id2]);
    janus.run_success(&["dep", "add", &id1, &id3]);
    janus.run_success(&["dep", "add", &id2, &id4]);
    janus.run_success(&["dep", "add", &id3, &id4]);

    // All should succeed - diamond patterns are valid
    let content1 = janus.read_ticket(&id1);
    assert!(content1.contains(&id2));
    assert!(content1.contains(&id3));
}

#[test]
#[serial]
fn test_dep_add_circular_in_middle_of_chain() {
    let janus = JanusTest::new();

    let id1 = janus
        .run_success(&["create", "Ticket A"])
        .trim()
        .to_string();
    let id2 = janus
        .run_success(&["create", "Ticket B"])
        .trim()
        .to_string();
    let id3 = janus
        .run_success(&["create", "Ticket C"])
        .trim()
        .to_string();
    let id4 = janus
        .run_success(&["create", "Ticket D"])
        .trim()
        .to_string();

    // Create chain: A -> B -> C -> D
    janus.run_success(&["dep", "add", &id1, &id2]);
    janus.run_success(&["dep", "add", &id2, &id3]);
    janus.run_success(&["dep", "add", &id3, &id4]);

    // Try to add C -> B (should fail - creates cycle in middle of chain)
    let stderr = janus.run_failure(&["dep", "add", &id3, &id2]);
    assert!(stderr.contains("circular dependency"));

    // Verify the chain structure is unchanged
    let content1 = janus.read_ticket(&id1);
    assert!(content1.contains(&id2));

    let content2 = janus.read_ticket(&id2);
    assert!(content2.contains(&id3));
    assert!(!content2.contains(&id1)); // B should not depend on A

    let content3 = janus.read_ticket(&id3);
    assert!(content3.contains(&id4));
    assert!(!content3.contains(&id2)); // C should not depend on B
}

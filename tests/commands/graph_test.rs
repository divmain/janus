#[path = "../common/mod.rs"]
mod common;
use common::JanusTest;
use serial_test::serial;

// ============================================================================
// Graph command tests
// ============================================================================

#[test]
#[serial]
fn test_graph_empty_dot() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["graph"]);
    assert!(output.contains("digraph janus"));
    assert!(output.contains("rankdir=TB"));
}

#[test]
#[serial]
fn test_graph_empty_mermaid() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["graph", "--format", "mermaid"]);
    assert!(output.contains("graph TD"));
}

#[test]
#[serial]
fn test_graph_with_tickets_dot() {
    let janus = JanusTest::new();

    let id1 = janus
        .run_success(&["create", "Ticket One"])
        .trim()
        .to_string();
    let id2 = janus
        .run_success(&["create", "Ticket Two"])
        .trim()
        .to_string();

    let output = janus.run_success(&["graph"]);
    assert!(output.contains("digraph janus"));
    assert!(output.contains(&id1));
    assert!(output.contains(&id2));
    assert!(output.contains("Ticket One"));
    assert!(output.contains("Ticket Two"));
}

#[test]
#[serial]
fn test_graph_with_tickets_mermaid() {
    let janus = JanusTest::new();

    let id1 = janus
        .run_success(&["create", "Ticket One"])
        .trim()
        .to_string();
    let id2 = janus
        .run_success(&["create", "Ticket Two"])
        .trim()
        .to_string();

    let output = janus.run_success(&["graph", "--format", "mermaid"]);
    assert!(output.contains("graph TD"));
    // IDs have hyphens replaced with underscores in mermaid
    let safe_id1 = id1.replace('-', "_");
    let safe_id2 = id2.replace('-', "_");
    assert!(output.contains(&safe_id1));
    assert!(output.contains(&safe_id2));
}

#[test]
#[serial]
fn test_graph_with_dependency_dot() {
    let janus = JanusTest::new();

    let id1 = janus
        .run_success(&["create", "Ticket One"])
        .trim()
        .to_string();
    let id2 = janus
        .run_success(&["create", "Ticket Two"])
        .trim()
        .to_string();

    // Add dependency: id1 depends on id2
    janus.run_success(&["dep", "add", &id1, &id2]);

    let output = janus.run_success(&["graph"]);
    assert!(output.contains(&format!("\"{}\" -> \"{}\"", id1, id2)));
    assert!(output.contains("blocks"));
}

#[test]
#[serial]
fn test_graph_with_dependency_mermaid() {
    let janus = JanusTest::new();

    let id1 = janus
        .run_success(&["create", "Ticket One"])
        .trim()
        .to_string();
    let id2 = janus
        .run_success(&["create", "Ticket Two"])
        .trim()
        .to_string();

    // Add dependency: id1 depends on id2
    janus.run_success(&["dep", "add", &id1, &id2]);

    let output = janus.run_success(&["graph", "--format", "mermaid"]);
    let safe_id1 = id1.replace('-', "_");
    let safe_id2 = id2.replace('-', "_");
    assert!(output.contains(&format!("{} -->|blocks| {}", safe_id1, safe_id2)));
}

#[test]
#[serial]
fn test_graph_with_spawning_dot() {
    let janus = JanusTest::new();

    let id1 = janus
        .run_success(&["create", "Parent Ticket"])
        .trim()
        .to_string();
    let id2 = janus
        .run_success(&["create", "Child Ticket", "--spawned-from", &id1])
        .trim()
        .to_string();

    let output = janus.run_success(&["graph"]);
    assert!(output.contains(&format!("\"{}\" -> \"{}\"", id1, id2)));
    assert!(output.contains("style=dashed"));
    assert!(output.contains("spawned"));
}

#[test]
#[serial]
fn test_graph_with_spawning_mermaid() {
    let janus = JanusTest::new();

    let id1 = janus
        .run_success(&["create", "Parent Ticket"])
        .trim()
        .to_string();
    let id2 = janus
        .run_success(&["create", "Child Ticket", "--spawned-from", &id1])
        .trim()
        .to_string();

    let output = janus.run_success(&["graph", "--format", "mermaid"]);
    let safe_id1 = id1.replace('-', "_");
    let safe_id2 = id2.replace('-', "_");
    assert!(output.contains(&format!("{} -.->|spawned| {}", safe_id1, safe_id2)));
}

#[test]
#[serial]
fn test_graph_deps_only() {
    let janus = JanusTest::new();

    let id1 = janus
        .run_success(&["create", "Parent Ticket"])
        .trim()
        .to_string();
    // Child ticket for spawning relationship
    let _id2 = janus
        .run_success(&["create", "Child Ticket", "--spawned-from", &id1])
        .trim()
        .to_string();
    let id3 = janus
        .run_success(&["create", "Dep Ticket"])
        .trim()
        .to_string();

    // Add dependency
    janus.run_success(&["dep", "add", &id1, &id3]);

    // Graph with --deps should only show dependencies
    let output = janus.run_success(&["graph", "--deps"]);
    assert!(output.contains("blocks"));
    assert!(!output.contains("spawned"));
}

#[test]
#[serial]
fn test_graph_spawn_only() {
    let janus = JanusTest::new();

    let id1 = janus
        .run_success(&["create", "Parent Ticket"])
        .trim()
        .to_string();
    // Child ticket for spawning relationship
    let _id2 = janus
        .run_success(&["create", "Child Ticket", "--spawned-from", &id1])
        .trim()
        .to_string();
    let id3 = janus
        .run_success(&["create", "Dep Ticket"])
        .trim()
        .to_string();

    // Add dependency
    janus.run_success(&["dep", "add", &id1, &id3]);

    // Graph with --spawn should only show spawning relationships
    let output = janus.run_success(&["graph", "--spawn"]);
    assert!(output.contains("spawned"));
    assert!(!output.contains("blocks"));
}

#[test]
#[serial]
fn test_graph_root_option() {
    let janus = JanusTest::new();

    let id1 = janus
        .run_success(&["create", "Root Ticket"])
        .trim()
        .to_string();
    let id2 = janus
        .run_success(&["create", "Connected Ticket"])
        .trim()
        .to_string();
    let id3 = janus
        .run_success(&["create", "Unconnected Ticket"])
        .trim()
        .to_string();

    // Only connect id1 and id2
    janus.run_success(&["dep", "add", &id1, &id2]);

    // Graph from id1 should only show id1 and id2
    let output = janus.run_success(&["graph", "--root", &id1]);
    assert!(output.contains(&id1));
    assert!(output.contains(&id2));
    assert!(!output.contains(&id3));
}

#[test]
#[serial]
fn test_graph_json_output() {
    let janus = JanusTest::new();

    let id1 = janus
        .run_success(&["create", "Ticket One"])
        .trim()
        .to_string();
    let id2 = janus
        .run_success(&["create", "Ticket Two"])
        .trim()
        .to_string();

    janus.run_success(&["dep", "add", &id1, &id2]);

    let output = janus.run_success(&["graph", "--json"]);
    let json: serde_json::Value = serde_json::from_str(&output).expect("Valid JSON");

    assert!(json["format"].as_str().is_some());
    assert!(json["nodes"].is_array());
    assert!(json["edges"].is_array());
    assert!(json["graph"].is_string());

    // Check nodes contain our tickets
    let nodes = json["nodes"].as_array().unwrap();
    let node_ids: Vec<&str> = nodes.iter().map(|n| n["id"].as_str().unwrap()).collect();
    assert!(node_ids.contains(&id1.as_str()));
    assert!(node_ids.contains(&id2.as_str()));

    // Check edges
    let edges = json["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0]["from"].as_str().unwrap(), &id1);
    assert_eq!(edges[0]["to"].as_str().unwrap(), &id2);
    assert_eq!(edges[0]["type"].as_str().unwrap(), "blocks");
}

#[test]
#[serial]
fn test_graph_plan_option() {
    let janus = JanusTest::new();

    // Create tickets
    let id1 = janus
        .run_success(&["create", "Plan Ticket 1"])
        .trim()
        .to_string();
    let id2 = janus
        .run_success(&["create", "Plan Ticket 2"])
        .trim()
        .to_string();
    let id3 = janus
        .run_success(&["create", "Non-Plan Ticket"])
        .trim()
        .to_string();

    // Create a plan and add tickets to it
    let plan_output = janus.run_success(&["plan", "create", "Test Plan"]);
    let plan_id = plan_output.trim().to_string();

    janus.run_success(&["plan", "add-ticket", &plan_id, &id1]);
    janus.run_success(&["plan", "add-ticket", &plan_id, &id2]);

    // Graph with --plan should only show plan tickets
    let output = janus.run_success(&["graph", "--plan", &plan_id]);
    assert!(output.contains(&id1));
    assert!(output.contains(&id2));
    assert!(!output.contains(&id3));
}

#[test]
#[serial]
fn test_graph_invalid_format() {
    let janus = JanusTest::new();

    let stderr = janus.run_failure(&["graph", "--format", "invalid"]);
    assert!(stderr.contains("Invalid graph format"));
}

#[test]
#[serial]
fn test_graph_root_not_found() {
    let janus = JanusTest::new();

    let stderr = janus.run_failure(&["graph", "--root", "j-nonexistent"]);
    assert!(stderr.contains("not found"));
}

#[test]
#[serial]
fn test_graph_plan_not_found() {
    let janus = JanusTest::new();

    let stderr = janus.run_failure(&["graph", "--plan", "plan-nonexistent"]);
    assert!(stderr.contains("not found"));
}

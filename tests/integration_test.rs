use std::fs;
use std::process::{Command, Output};
use tempfile::TempDir;

/// Helper struct to run janus commands in an isolated temp directory
struct JanusTest {
    temp_dir: TempDir,
    binary_path: String,
}

impl JanusTest {
    fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        // Find the binary - check both debug and release
        let binary_path = if cfg!(debug_assertions) {
            concat!(env!("CARGO_MANIFEST_DIR"), "/target/debug/janus")
        } else {
            concat!(env!("CARGO_MANIFEST_DIR"), "/target/release/janus")
        };

        // If the above doesn't exist, try the alternative
        let binary_path = if std::path::Path::new(binary_path).exists() {
            binary_path.to_string()
        } else {
            // Fallback to debug
            concat!(env!("CARGO_MANIFEST_DIR"), "/target/debug/janus").to_string()
        };

        JanusTest {
            temp_dir,
            binary_path,
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(&self.binary_path)
            .args(args)
            .current_dir(self.temp_dir.path())
            .output()
            .expect("Failed to execute janus command")
    }

    fn run_success(&self, args: &[&str]) -> String {
        let output = self.run(args);
        if !output.status.success() {
            panic!(
                "Command {:?} failed with status {:?}\nstdout: {}\nstderr: {}",
                args,
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        String::from_utf8_lossy(&output.stdout).to_string()
    }

    fn run_failure(&self, args: &[&str]) -> String {
        let output = self.run(args);
        assert!(
            !output.status.success(),
            "Expected command {:?} to fail, but it succeeded",
            args
        );
        String::from_utf8_lossy(&output.stderr).to_string()
    }

    fn read_ticket(&self, id: &str) -> String {
        let path = self
            .temp_dir
            .path()
            .join(".janus")
            .join("items")
            .join(format!("{}.md", id));
        fs::read_to_string(path).expect("Failed to read ticket file")
    }

    fn ticket_exists(&self, id: &str) -> bool {
        let path = self
            .temp_dir
            .path()
            .join(".janus")
            .join("items")
            .join(format!("{}.md", id));
        path.exists()
    }

    fn write_ticket(&self, id: &str, content: &str) {
        let dir = self.temp_dir.path().join(".janus").join("items");
        fs::create_dir_all(&dir).expect("Failed to create .janus/items directory");
        let path = dir.join(format!("{}.md", id));
        fs::write(path, content).expect("Failed to write ticket file");
    }

    fn read_plan(&self, id: &str) -> String {
        let path = self
            .temp_dir
            .path()
            .join(".janus")
            .join("plans")
            .join(format!("{}.md", id));
        fs::read_to_string(path).expect("Failed to read plan file")
    }

    fn plan_exists(&self, id: &str) -> bool {
        let path = self
            .temp_dir
            .path()
            .join(".janus")
            .join("plans")
            .join(format!("{}.md", id));
        path.exists()
    }

    fn write_plan(&self, id: &str, content: &str) {
        let dir = self.temp_dir.path().join(".janus").join("plans");
        fs::create_dir_all(&dir).expect("Failed to create .janus/plans directory");
        let path = dir.join(format!("{}.md", id));
        fs::write(path, content).expect("Failed to write plan file");
    }
}

// ============================================================================
// Create command tests
// ============================================================================

#[test]
fn test_create_basic() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["create", "Test ticket"]);
    let id = output.trim();

    assert!(!id.is_empty(), "Should output a ticket ID");
    assert!(id.contains('-'), "ID should contain a dash");
    assert!(janus.ticket_exists(id), "Ticket file should exist");

    let content = janus.read_ticket(id);
    assert!(content.contains("# Test ticket"));
    assert!(content.contains("status: new"));
    assert!(content.contains("deps: []"));
    assert!(content.contains("links: []"));
    assert!(content.contains("type: task"));
    assert!(content.contains("priority: 2"));
}

#[test]
fn test_create_with_options() {
    let janus = JanusTest::new();

    let output = janus.run_success(&[
        "create",
        "Bug ticket",
        "-d",
        "This is a description",
        "-p",
        "0",
        "-t",
        "bug",
        "--external-ref",
        "gh-123",
    ]);
    let id = output.trim();

    let content = janus.read_ticket(id);
    assert!(content.contains("# Bug ticket"));
    assert!(content.contains("This is a description"));
    assert!(content.contains("priority: 0"));
    assert!(content.contains("type: bug"));
    assert!(content.contains("external-ref: gh-123"));
}

#[test]
fn test_create_with_parent() {
    let janus = JanusTest::new();

    let parent_id = janus
        .run_success(&["create", "Parent ticket"])
        .trim()
        .to_string();
    let child_id = janus
        .run_success(&["create", "Child ticket", "--parent", &parent_id])
        .trim()
        .to_string();

    let child_content = janus.read_ticket(&child_id);
    assert!(child_content.contains(&format!("parent: {}", parent_id)));
}

#[test]
fn test_create_all_types() {
    let janus = JanusTest::new();

    for ticket_type in &["bug", "feature", "task", "epic", "chore"] {
        let output = janus.run_success(&["create", "Test", "-t", ticket_type]);
        let id = output.trim();
        let content = janus.read_ticket(id);
        assert!(
            content.contains(&format!("type: {}", ticket_type)),
            "Type should be {}",
            ticket_type
        );
    }
}

#[test]
fn test_create_all_priorities() {
    let janus = JanusTest::new();

    for priority in &["0", "1", "2", "3", "4"] {
        let output = janus.run_success(&["create", "Test", "-p", priority]);
        let id = output.trim();
        let content = janus.read_ticket(id);
        assert!(
            content.contains(&format!("priority: {}", priority)),
            "Priority should be {}",
            priority
        );
    }
}

#[test]
fn test_create_invalid_priority() {
    let janus = JanusTest::new();
    let stderr = janus.run_failure(&["create", "Test", "-p", "5"]);
    assert!(stderr.contains("Invalid priority"));
}

#[test]
fn test_create_invalid_type() {
    let janus = JanusTest::new();
    let stderr = janus.run_failure(&["create", "Test", "-t", "invalid"]);
    assert!(stderr.contains("Invalid type"));
}

#[test]
fn test_create_with_custom_prefix() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["create", "Test ticket", "--prefix", "perf"]);
    let id = output.trim();

    assert!(id.starts_with("perf-"), "ID should start with 'perf-'");
    assert!(janus.ticket_exists(id), "Ticket file should exist");

    let content = janus.read_ticket(id);
    assert!(content.contains("# Test ticket"));
    assert!(content.contains("uuid:"), "Ticket should have a UUID");
}

#[test]
fn test_create_with_empty_uses_default() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["create", "Test ticket", "--prefix", ""]);
    let id = output.trim();

    assert!(!id.is_empty(), "Should output a ticket ID");
    assert!(id.contains('-'), "ID should contain a dash");
    assert!(janus.ticket_exists(id), "Ticket file should exist");
}

#[test]
fn test_create_with_hyphen_prefix() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["create", "Test ticket", "--prefix", "my-prefix"]);
    let id = output.trim();

    assert!(
        id.starts_with("my-prefix-"),
        "ID should start with 'my-prefix-'"
    );
    assert!(janus.ticket_exists(id), "Ticket file should exist");
}

#[test]
fn test_create_with_underscore_prefix() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["create", "Test ticket", "--prefix", "my_prefix"]);
    let id = output.trim();

    assert!(
        id.starts_with("my_prefix-"),
        "ID should start with 'my_prefix-'"
    );
    assert!(janus.ticket_exists(id), "Ticket file should exist");
}

#[test]
fn test_create_multiple_tickets_same_prefix() {
    let janus = JanusTest::new();

    let output1 = janus.run_success(&["create", "Ticket 1", "--prefix", "perf"]);
    let output2 = janus.run_success(&["create", "Ticket 2", "--prefix", "perf"]);
    let id1 = output1.trim();
    let id2 = output2.trim();

    assert!(id1.starts_with("perf-"), "ID1 should start with 'perf-'");
    assert!(id2.starts_with("perf-"), "ID2 should start with 'perf-'");
    assert_ne!(id1, id2, "IDs should be unique even with same prefix");
    assert!(janus.ticket_exists(id1), "Ticket1 should exist");
    assert!(janus.ticket_exists(id2), "Ticket2 should exist");
}

#[test]
fn test_create_tickets_different_prefixes() {
    let janus = JanusTest::new();

    let output1 = janus.run_success(&["create", "Bug fix", "--prefix", "bug"]);
    let output2 = janus.run_success(&["create", "Feature", "--prefix", "feat"]);
    let output3 = janus.run_success(&["create", "Task"]);
    let id1 = output1.trim();
    let id2 = output2.trim();
    let id3 = output3.trim();

    assert!(id1.starts_with("bug-"), "ID1 should start with 'bug-'");
    assert!(id2.starts_with("feat-"), "ID2 should start with 'feat-'");
    assert!(!id3.starts_with("bug-"), "ID3 should not start with 'bug-'");
    assert!(
        !id3.starts_with("feat-"),
        "ID3 should not start with 'feat-'"
    );
}

#[test]
fn test_create_with_reserved_prefix_fails() {
    let janus = JanusTest::new();

    let stderr = janus.run_failure(&["create", "Test ticket", "--prefix", "plan"]);
    assert!(
        stderr.contains("reserved"),
        "Error should mention the prefix is reserved"
    );
    assert!(
        stderr.contains("plan"),
        "Error should mention the prefix 'plan'"
    );
}

#[test]
fn test_create_with_invalid_prefix_characters_fails() {
    let janus = JanusTest::new();

    let invalid_prefixes = vec![
        ("invalid/prefix", "invalid characters"),
        ("invalid@prefix", "invalid characters"),
        ("invalid prefix", "invalid characters"),
        ("invalid.prefix", "invalid characters"),
    ];

    for (prefix, expected_error) in invalid_prefixes {
        let stderr = janus.run_failure(&["create", "Test ticket", "--prefix", prefix]);
        assert!(
            stderr.contains(expected_error),
            "Error for prefix '{}' should contain '{}'",
            prefix,
            expected_error
        );
        assert!(
            stderr.contains(prefix),
            "Error should mention the invalid prefix '{}'",
            prefix
        );
    }
}

// ============================================================================
// Status command tests
// ============================================================================

#[test]
fn test_status_start() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    janus.run_success(&["status", &id, "complete"]);

    let content = janus.read_ticket(&id);
    assert!(content.contains("status: complete"));

    janus.run_success(&["start", &id]);
    let content = janus.read_ticket(&id);
    assert!(content.contains("status: in_progress"));
}

#[test]
fn test_status_close() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    janus.run_success(&["close", &id]);

    let content = janus.read_ticket(&id);
    assert!(content.contains("status: complete"));
}

#[test]
fn test_status_reopen() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    janus.run_success(&["close", &id]);
    janus.run_success(&["reopen", &id]);

    let content = janus.read_ticket(&id);
    assert!(content.contains("status: new"));
}

#[test]
fn test_status_cancelled() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    janus.run_success(&["status", &id, "cancelled"]);

    let content = janus.read_ticket(&id);
    assert!(content.contains("status: cancelled"));
}

#[test]
fn test_status_next() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    janus.run_success(&["status", &id, "next"]);

    let content = janus.read_ticket(&id);
    assert!(content.contains("status: next"));
}

#[test]
fn test_status_in_progress() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    janus.run_success(&["status", &id, "in_progress"]);

    let content = janus.read_ticket(&id);
    assert!(content.contains("status: in_progress"));
}

#[test]
fn test_start_sets_in_progress() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    janus.run_success(&["start", &id]);

    let content = janus.read_ticket(&id);
    assert!(content.contains("status: in_progress"));
}

#[test]
fn test_status_invalid() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    let stderr = janus.run_failure(&["status", &id, "invalid"]);
    assert!(stderr.contains("Invalid status"));
}

// ============================================================================
// Set command tests
// ============================================================================

#[test]
fn test_set_priority() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();

    // Default priority is 2, change to 0
    let output = janus.run_success(&["set", &id, "priority", "0"]);
    assert!(output.contains("Updated"));
    assert!(output.contains("priority"));

    let content = janus.read_ticket(&id);
    assert!(content.contains("priority: 0"));
}

#[test]
fn test_set_priority_all_values() {
    let janus = JanusTest::new();

    for priority in &["0", "1", "2", "3", "4"] {
        let id = janus.run_success(&["create", "Test"]).trim().to_string();
        janus.run_success(&["set", &id, "priority", priority]);

        let content = janus.read_ticket(&id);
        assert!(
            content.contains(&format!("priority: {}", priority)),
            "Priority should be set to {}",
            priority
        );
    }
}

#[test]
fn test_set_priority_invalid() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    let stderr = janus.run_failure(&["set", &id, "priority", "5"]);
    assert!(stderr.contains("invalid value"));
    assert!(stderr.contains("priority"));
}

#[test]
fn test_set_type() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();

    // Default type is task, change to bug
    let output = janus.run_success(&["set", &id, "type", "bug"]);
    assert!(output.contains("Updated"));
    assert!(output.contains("type"));

    let content = janus.read_ticket(&id);
    assert!(content.contains("type: bug"));
}

#[test]
fn test_set_type_all_values() {
    let janus = JanusTest::new();

    for ticket_type in &["bug", "feature", "task", "epic", "chore"] {
        let id = janus.run_success(&["create", "Test"]).trim().to_string();
        janus.run_success(&["set", &id, "type", ticket_type]);

        let content = janus.read_ticket(&id);
        assert!(
            content.contains(&format!("type: {}", ticket_type)),
            "Type should be set to {}",
            ticket_type
        );
    }
}

#[test]
fn test_set_type_invalid() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    let stderr = janus.run_failure(&["set", &id, "type", "invalid"]);
    assert!(stderr.contains("invalid value"));
    assert!(stderr.contains("type"));
}

#[test]
fn test_set_parent() {
    let janus = JanusTest::new();

    let parent_id = janus
        .run_success(&["create", "Parent ticket"])
        .trim()
        .to_string();
    let child_id = janus
        .run_success(&["create", "Child ticket"])
        .trim()
        .to_string();

    // Set parent
    let output = janus.run_success(&["set", &child_id, "parent", &parent_id]);
    assert!(output.contains("Updated"));
    assert!(output.contains("parent"));

    let content = janus.read_ticket(&child_id);
    assert!(content.contains(&format!("parent: {}", parent_id)));
}

#[test]
fn test_set_parent_clear() {
    let janus = JanusTest::new();

    let parent_id = janus
        .run_success(&["create", "Parent ticket"])
        .trim()
        .to_string();
    let child_id = janus
        .run_success(&["create", "Child ticket", "--parent", &parent_id])
        .trim()
        .to_string();

    // Verify parent is set
    let content = janus.read_ticket(&child_id);
    assert!(content.contains(&format!("parent: {}", parent_id)));

    // Clear parent with empty string
    let output = janus.run_success(&["set", &child_id, "parent", ""]);
    assert!(output.contains("Updated"));

    let content = janus.read_ticket(&child_id);
    assert!(!content.contains("parent:"));
}

#[test]
fn test_set_parent_nonexistent() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    let stderr = janus.run_failure(&["set", &id, "parent", "nonexistent"]);
    assert!(stderr.contains("not found"));
}

#[test]
fn test_set_invalid_field() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    let stderr = janus.run_failure(&["set", &id, "invalid_field", "value"]);
    assert!(stderr.contains("invalid field"));
    assert!(stderr.contains("must be one of"));
}

#[test]
fn test_set_json_output() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    let output = janus.run_success(&["set", &id, "priority", "1", "--json"]);

    // Verify JSON output
    let json: serde_json::Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert_eq!(json["action"], "field_updated");
    assert_eq!(json["field"], "priority");
    assert_eq!(json["new_value"], "1");
    assert_eq!(json["id"], id);
}

#[test]
fn test_set_ticket_not_found() {
    let janus = JanusTest::new();

    let stderr = janus.run_failure(&["set", "nonexistent", "priority", "1"]);
    assert!(stderr.contains("not found"));
}

// ============================================================================
// Show command tests
// ============================================================================

#[test]
fn test_show_basic() {
    let janus = JanusTest::new();

    let id = janus
        .run_success(&["create", "Test ticket", "-d", "Description"])
        .trim()
        .to_string();
    let output = janus.run_success(&["show", &id]);

    assert!(output.contains("# Test ticket"));
    assert!(output.contains("Description"));
    assert!(output.contains(&format!("id: {}", id)));
}

#[test]
fn test_show_partial_id() {
    let janus = JanusTest::new();

    let id = janus
        .run_success(&["create", "Test ticket"])
        .trim()
        .to_string();
    // Use just the hash part (after the dash)
    let partial = id.split('-').last().unwrap();
    let output = janus.run_success(&["show", partial]);

    assert!(output.contains("# Test ticket"));
}

#[test]
fn test_show_with_blockers() {
    let janus = JanusTest::new();

    let dep_id = janus
        .run_success(&["create", "Dependency"])
        .trim()
        .to_string();
    let id = janus
        .run_success(&["create", "Main ticket"])
        .trim()
        .to_string();
    janus.run_success(&["dep", "add", &id, &dep_id]);

    let output = janus.run_success(&["show", &id]);
    assert!(output.contains("## Blockers"));
    assert!(output.contains(&dep_id));
}

#[test]
fn test_show_with_blocking() {
    let janus = JanusTest::new();

    let id = janus
        .run_success(&["create", "Main ticket"])
        .trim()
        .to_string();
    let blocked_id = janus
        .run_success(&["create", "Blocked ticket"])
        .trim()
        .to_string();
    janus.run_success(&["dep", "add", &blocked_id, &id]);

    let output = janus.run_success(&["show", &id]);
    assert!(output.contains("## Blocking"));
    assert!(output.contains(&blocked_id));
}

#[test]
fn test_show_with_children() {
    let janus = JanusTest::new();

    let parent_id = janus.run_success(&["create", "Parent"]).trim().to_string();
    let child_id = janus
        .run_success(&["create", "Child", "--parent", &parent_id])
        .trim()
        .to_string();

    let output = janus.run_success(&["show", &parent_id]);
    assert!(output.contains("## Children"));
    assert!(output.contains(&child_id));
}

#[test]
fn test_show_with_links() {
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

    let output = janus.run_success(&["show", &id1]);
    assert!(output.contains("## Linked"));
    assert!(output.contains(&id2));
}

#[test]
fn test_show_not_found() {
    let janus = JanusTest::new();
    let stderr = janus.run_failure(&["show", "nonexistent"]);
    assert!(stderr.contains("not found"));
}

// ============================================================================
// Dependency command tests
// ============================================================================

#[test]
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

#[test]
fn test_undep_legacy() {
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
    let output = janus.run_success(&["undep", &id1, &id2]);
    assert!(output.contains("Removed dependency"));
}

// ============================================================================
// Link command tests
// ============================================================================

#[test]
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

#[test]
fn test_unlink_legacy() {
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
    let output = janus.run_success(&["unlink", &id1, &id2]);
    assert!(output.contains("Removed link"));
}

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
    janus.run_success(&["close", &id2]);

    let output = janus.run_success(&["ls", "--status", "new"]);
    assert!(output.contains(&id1));
    assert!(!output.contains(&id2));

    let output = janus.run_success(&["ls", "--status", "complete"]);
    assert!(!output.contains(&id1));
    assert!(output.contains(&id2));
}

#[test]
fn test_ready() {
    let janus = JanusTest::new();

    let dep_id = janus
        .run_success(&["create", "Dependency"])
        .trim()
        .to_string();
    let blocked_id = janus.run_success(&["create", "Blocked"]).trim().to_string();
    let ready_id = janus.run_success(&["create", "Ready"]).trim().to_string();

    janus.run_success(&["dep", "add", &blocked_id, &dep_id]);

    let output = janus.run_success(&["ls", "--ready"]);
    assert!(output.contains(&dep_id));
    assert!(output.contains(&ready_id));
    assert!(!output.contains(&blocked_id));
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
    janus.run_success(&["close", &dep_id]);

    // Now ready
    let output = janus.run_success(&["ls", "--ready"]);
    assert!(output.contains(&blocked_id));
}

#[test]
fn test_blocked() {
    let janus = JanusTest::new();

    let dep_id = janus
        .run_success(&["create", "Dependency"])
        .trim()
        .to_string();
    let blocked_id = janus.run_success(&["create", "Blocked"]).trim().to_string();
    let ready_id = janus.run_success(&["create", "Ready"]).trim().to_string();

    janus.run_success(&["dep", "add", &blocked_id, &dep_id]);

    let output = janus.run_success(&["ls", "--blocked"]);

    // The blocked ticket should appear with its title
    assert!(output.contains(&blocked_id), "Blocked ticket should appear");
    assert!(
        output.contains("Blocked"),
        "Blocked ticket title should appear"
    );

    // The dep_id appears in the suffix as a blocker, which is expected
    // But the dependency ticket's title should NOT appear (it's not blocked itself)
    assert!(
        !output.contains("Dependency"),
        "Dependency ticket should not be listed as blocked"
    );

    // Ready ticket should not appear at all
    assert!(
        !output.contains(&ready_id),
        "Ready ticket should not appear"
    );
    assert!(
        !output.contains("Ready"),
        "Ready ticket title should not appear"
    );
}

#[test]
fn test_closed() {
    let janus = JanusTest::new();

    let id1 = janus.run_success(&["create", "Open"]).trim().to_string();
    let id2 = janus.run_success(&["create", "Closed"]).trim().to_string();
    janus.run_success(&["close", &id2]);

    let output = janus.run_success(&["ls", "--closed"]);
    assert!(!output.contains(&id1));
    assert!(output.contains(&id2));
}

#[test]
fn test_closed_limit() {
    let janus = JanusTest::new();

    // Create and close 5 tickets
    for i in 0..5 {
        let id = janus
            .run_success(&["create", &format!("Ticket {}", i)])
            .trim()
            .to_string();
        janus.run_success(&["close", &id]);
    }

    let output = janus.run_success(&["ls", "--closed", "--limit", "2"]);
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines.len(), 2);
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
    janus.run_success(&["close", &closed_id]);

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
            .run_success(&["create", &format!("Ticket {}", i)])
            .trim()
            .to_string();
        janus.run_success(&["close", &id]);
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
fn test_ls_all_flag() {
    let janus = JanusTest::new();

    let open_id = janus.run_success(&["create", "Open"]).trim().to_string();
    let closed_id = janus.run_success(&["create", "Closed"]).trim().to_string();
    janus.run_success(&["close", &closed_id]);

    // Without --all, closed tickets should not appear
    let output_without_all = janus.run_success(&["ls"]);
    assert!(output_without_all.contains(&open_id));
    assert!(!output_without_all.contains(&closed_id));

    // With --all, closed tickets should appear
    let output_with_all = janus.run_success(&["ls", "--all"]);
    assert!(output_with_all.contains(&open_id));
    assert!(output_with_all.contains(&closed_id));
}

#[test]
fn test_ls_status_conflicts_with_filters() {
    let janus = JanusTest::new();

    let output = janus.run_failure(&["ls", "--status", "new", "--ready"]);
    assert!(output.contains("cannot be used with") || output.contains("conflicts"));
}

#[test]
fn test_ready_command_removed() {
    let janus = JanusTest::new();

    let output = janus.run(&["ready"]);
    // run() doesn't check success, so we need to verify it failed
    assert!(
        !output.status.success(),
        "ready command should fail but succeeded"
    );
}

#[test]
fn test_blocked_command_removed() {
    let janus = JanusTest::new();

    let output = janus.run(&["blocked"]);
    assert!(
        !output.status.success(),
        "blocked command should fail but succeeded"
    );
}

#[test]
fn test_closed_command_removed() {
    let janus = JanusTest::new();

    let output = janus.run(&["closed"]);
    assert!(
        !output.status.success(),
        "closed command should fail but succeeded"
    );
}

#[test]
fn test_ls_limit_without_closed() {
    let janus = JanusTest::new();

    // Create more tickets than the limit
    for i in 0..10 {
        janus.run_success(&["create", &format!("Ticket {}", i)]);
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
        janus.run_success(&["create", &format!("Ticket {}", i)]);
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
        janus.run_success(&["create", &format!("Ticket {}", i)]);
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
            .run_success(&["create", &format!("Blocked {}", i)])
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
            .run_success(&["create", &format!("Ticket {}", i)])
            .trim()
            .to_string();
        janus.run_success(&["close", &id]);
    }

    // --closed without explicit --limit should default to 20
    let output = janus.run_success(&["ls", "--closed"]);
    let line_count = output.lines().count();
    assert_eq!(line_count, 20, "--closed should show 20 tickets by default");
}

#[test]
fn test_ls_closed_custom_limit() {
    let janus = JanusTest::new();

    // Create and close 30 tickets
    for i in 0..30 {
        let id = janus
            .run_success(&["create", &format!("Ticket {}", i)])
            .trim()
            .to_string();
        janus.run_success(&["close", &id]);
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
fn test_ls_all_with_other_filters() {
    let janus = JanusTest::new();

    let open_id = janus
        .run_success(&["create", "Open ticket"])
        .trim()
        .to_string();
    let closed_id = janus
        .run_success(&["create", "Closed ticket"])
        .trim()
        .to_string();
    janus.run_success(&["close", &closed_id]);

    // --all should show all tickets
    let output = janus.run_success(&["ls", "--all"]);
    assert!(output.contains(&open_id));
    assert!(output.contains(&closed_id));

    // --all with --status should still filter by status
    let output = janus.run_success(&["ls", "--all", "--status", "complete"]);
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
    janus.run_success(&["close", &closed_id]);

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

// ============================================================================
// Add-note command tests
// ============================================================================

#[test]
fn test_add_note() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    janus.run_success(&["add-note", &id, "This is a note"]);

    let content = janus.read_ticket(&id);
    assert!(content.contains("## Notes"));
    assert!(content.contains("This is a note"));
    // Should have a timestamp
    assert!(content.contains("**20")); // Year prefix
}

#[test]
fn test_add_note_multiple() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    janus.run_success(&["add-note", &id, "Note 1"]);
    janus.run_success(&["add-note", &id, "Note 2"]);

    let content = janus.read_ticket(&id);
    assert!(content.contains("Note 1"));
    assert!(content.contains("Note 2"));
}

// ============================================================================
// Edit command tests
// ============================================================================

#[test]
fn test_edit_non_tty() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    // In non-TTY mode (like tests), it should just print the file path
    let output = janus.run_success(&["edit", &id]);
    assert!(output.contains(&id));
    assert!(output.contains(".janus"));
}

// ============================================================================
// Query command tests
// ============================================================================

#[test]
fn test_query_basic() {
    let janus = JanusTest::new();

    let id = janus
        .run_success(&["create", "Test ticket"])
        .trim()
        .to_string();

    let output = janus.run_success(&["query"]);
    assert!(output.contains(&id));
    assert!(output.contains("Test ticket"));
    assert!(output.contains("\"status\":\"new\""));
}

#[test]
fn test_query_json_format() {
    let janus = JanusTest::new();

    janus.run_success(&["create", "Test"]);

    let output = janus.run_success(&["query"]);

    // Should be valid JSON on each line
    for line in output.lines() {
        if !line.trim().is_empty() {
            let _: serde_json::Value =
                serde_json::from_str(line).expect("Output should be valid JSON");
        }
    }
}

// ============================================================================
// Error handling tests
// ============================================================================

#[test]
fn test_ticket_not_found() {
    let janus = JanusTest::new();

    let stderr = janus.run_failure(&["show", "nonexistent"]);
    assert!(stderr.contains("not found"));
}

#[test]
fn test_ambiguous_id() {
    let janus = JanusTest::new();

    // Create two tickets - they'll have the same prefix
    let id1 = janus
        .run_success(&["create", "Ticket 1"])
        .trim()
        .to_string();
    let id2 = janus
        .run_success(&["create", "Ticket 2"])
        .trim()
        .to_string();

    // Get the common prefix (before the hash)
    let prefix = id1.split('-').next().unwrap();

    // If both tickets share the prefix, this should be ambiguous
    if id2.starts_with(prefix) && id1.split('-').last() != id2.split('-').last() {
        let stderr = janus.run_failure(&["show", prefix]);
        assert!(stderr.contains("ambiguous") || stderr.contains("multiple"));
    }
}

#[test]
fn test_dep_add_nonexistent() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    let stderr = janus.run_failure(&["dep", "add", &id, "nonexistent"]);
    assert!(stderr.contains("not found"));
}

// ============================================================================
// Alias tests
// ============================================================================

#[test]
fn test_create_alias() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["c", "Test ticket"]);
    let id = output.trim();
    assert!(janus.ticket_exists(id));
}

#[test]
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
fn test_help() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["--help"]);
    assert!(output.contains("Plain-text issue tracking"));
    assert!(output.contains("create"));
    assert!(output.contains("show"));
    assert!(output.contains("dep"));
    assert!(output.contains("link"));
}

#[test]
fn test_version() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["--version"]);
    assert!(output.contains("janus"));
}

// ============================================================================
// Config command tests
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

    janus.run_success(&["config", "set", "default_remote", "github:myorg/myrepo"]);
    let output = janus.run_success(&["config", "show"]);
    assert!(output.contains("github"));
    assert!(output.contains("myorg"));
}

#[test]
fn test_config_set_linear_default_remote() {
    let janus = JanusTest::new();

    janus.run_success(&["config", "set", "default_remote", "linear:myorg"]);
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

    let stderr = janus.run_failure(&["config", "set", "default_remote", "invalid"]);
    assert!(stderr.contains("invalid") || stderr.contains("format"));
}

#[test]
fn test_config_file_created() {
    let janus = JanusTest::new();

    janus.run_success(&["config", "set", "default_remote", "github:owner/repo"]);

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
        "Error should mention the prefix is reserved, got: {}",
        stderr
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
        "Error should mention invalid characters, got: {}",
        stderr
    );
}

#[test]
fn test_push_not_configured() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    let stderr = janus.run_failure(&["remote", "push", &id]);
    // Should fail due to no default_remote config
    assert!(
        stderr.contains("not configured") || stderr.contains("default_remote"),
        "Should fail due to missing config: {}",
        stderr
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
    fs::create_dir_all(&janus_dir.join("items")).unwrap();

    let ticket_path = janus_dir.join("items").join("j-a1b2.md");
    let content = r#"---
id: j-a1b2
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
    fs::create_dir_all(&janus_dir.join("items")).unwrap();

    let ticket_path = janus_dir.join("items").join("j-test.md");
    let content = r#"---
id: j-test
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
    assert!(status_output.contains("Cached tickets"));
}

#[test]
fn test_cache_clear_command() {
    let janus = JanusTest::new();

    let janus_dir = janus.temp_dir.path().join(".janus");
    fs::create_dir_all(&janus_dir.join("items")).unwrap();

    let ticket_path = janus_dir.join("items").join("j-test.md");
    let content = r#"---
id: j-test
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

    let output = janus.run_success(&["cache", "clear"]);
    assert!(output.contains("clear"));

    let output2 = janus.run_success(&["ls"]);
    assert!(output2.contains("j-test"));
}

#[test]
fn test_cache_rebuild_command() {
    let janus = JanusTest::new();

    let janus_dir = janus.temp_dir.path().join(".janus");
    fs::create_dir_all(&janus_dir.join("items")).unwrap();

    let ticket_path = janus_dir.join("items").join("j-test.md");
    let content = r#"---
id: j-test
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
fn test_cache_path_command() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["cache", "path"]);
    let path_str = output.trim();
    let cache_path = std::path::PathBuf::from(path_str);

    assert!(cache_path.is_absolute());
    assert!(cache_path.to_string_lossy().contains("janus"));
    assert!(
        cache_path
            .extension()
            .map(|ext| ext == "db")
            .unwrap_or(false)
    );
}

#[test]
fn test_cache_corrupted_database() {
    let janus = JanusTest::new();

    let janus_dir = janus.temp_dir.path().join(".janus");
    fs::create_dir_all(&janus_dir.join("items")).unwrap();

    let ticket_path = janus_dir.join("items").join("j-test.md");
    let content = r#"---
id: j-test
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

    let cache_path_output = janus.run_success(&["cache", "path"]);
    let cache_path = std::path::PathBuf::from(cache_path_output.trim());

    assert!(cache_path.exists(), "Cache file should exist after ls");

    let corrupted_data = b"This is corrupted database data, not SQLite format";
    fs::write(&cache_path, corrupted_data).unwrap();

    let stderr = janus.run(&["ls"]).stderr;
    let stderr_str = String::from_utf8_lossy(&stderr);
    let stdout = janus.run(&["ls"]).stdout;
    let stdout_str = String::from_utf8_lossy(&stdout);

    assert!(
        stderr_str.contains("Warning")
            || stderr_str.contains("corrupted")
            || stdout_str.contains("j-test"),
        "Should warn about corruption or fall back to file reads. stderr: {}, stdout: {}",
        stderr_str,
        stdout_str
    );
}

#[test]
fn test_cache_rebuild_after_corruption() {
    let janus = JanusTest::new();

    let janus_dir = janus.temp_dir.path().join(".janus");
    fs::create_dir_all(&janus_dir.join("items")).unwrap();

    let ticket_path = janus_dir.join("items").join("j-test.md");
    let content = r#"---
id: j-test
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

    let cache_path_output = janus.run_success(&["cache", "path"]);
    let cache_path = std::path::PathBuf::from(cache_path_output.trim());

    fs::write(&cache_path, b"corrupted data").unwrap();

    let output = janus.run_success(&["cache", "rebuild"]);
    assert!(output.contains("rebuilt"));

    let stdout = janus.run_success(&["ls"]);
    assert!(stdout.contains("j-test"));
}

#[test]
fn test_cache_no_directory_works_without_cache() {
    let janus = JanusTest::new();

    let janus_dir = janus.temp_dir.path().join(".janus");
    fs::create_dir_all(&janus_dir.join("items")).unwrap();

    let ticket_path = janus_dir.join("items").join("j-test.md");
    let content = r#"---
id: j-test
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

    let cache_path_output = janus.run_success(&["cache", "path"]);
    let cache_path = std::path::PathBuf::from(cache_path_output.trim());
    let cache_dir = cache_path.parent().unwrap();

    if cache_dir.exists() {
        fs::remove_dir_all(&cache_dir).ok();
    }

    let stdout1 = janus.run(&["ls"]).stdout;
    let stdout1_str = String::from_utf8_lossy(&stdout1);
    assert!(stdout1_str.contains("j-test"));
}

#[test]
fn test_cache_unavailable_degrades_gracefully() {
    let janus = JanusTest::new();

    let janus_dir = janus.temp_dir.path().join(".janus");
    fs::create_dir_all(&janus_dir.join("items")).unwrap();

    let ticket_path = janus_dir.join("items").join("j-test.md");
    let content = r#"---
id: j-test
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

    let cache_path_output = janus.run_success(&["cache", "path"]);
    let cache_path = std::path::PathBuf::from(cache_path_output.trim());

    let corrupt_content = vec![0xFF, 0xFE, 0xFD, 0xFC, 0xFB, 0xFA];
    fs::write(&cache_path, &corrupt_content).unwrap();

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
    assert!(content.contains(&format!("id: {}", id)));
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
    assert!(output.contains(&format!("id: {}", id)));
    assert!(output.contains("# Raw Test Plan"));
}

#[test]
fn test_plan_show_with_tickets() {
    let janus = JanusTest::new();

    // Create tickets with known IDs
    let ticket1_content = r#"---
id: j-task1
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
    janus.write_plan("plan-test", &content);

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
    janus.write_plan("plan-phased", &plan_content);

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
    janus.write_plan("plan-missing", &content);

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
    janus.write_plan("plan-complete", &complete_plan);

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
    janus.write_plan("plan-freeform", &content);

    // Create the referenced ticket
    let ticket_content = r#"---
id: j-test1
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
    janus.run_success(&["close", &ticket1]);

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
    janus.run_success(&["close", &ticket1]);

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
    janus.run_success(&["close", &ticket1]);

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
    janus.run_success(&["close", &ticket1]);

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
    janus.run_success(&["close", &ticket1]);

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
    janus.write_plan("plan-allcanc", &plan_content);

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
    janus.write_plan("plan-mixfinish", &plan_content);

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
    janus.write_plan("plan-allnext", &plan_content);

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
        phases.push(format!("--phase"));
        phases.push(format!("Phase {}", i));
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
            content.contains(&format!("Phase {}", i)),
            "Should contain Phase {}",
            i
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
id: j-bulk{:02}
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Bulk Task {}

Description for task {}.
"#,
            i, i, i
        );
        janus.write_ticket(&format!("j-bulk{:02}", i), &ticket_content);
        ticket_ids.push(format!("j-bulk{:02}", i));
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

{}
"#,
        tickets_list
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
    janus.write_plan("plan-manymissing", &plan_content);

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
    janus.write_plan("plan-ac", &plan_content);

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
    janus.write_plan("plan-emptyph", &plan_content);

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
    janus.write_plan("plan-code", &plan_content);

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
    janus.write_plan("plan-inprog", &plan_content);

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
    janus.write_plan("plan-ph12", &plan_content);

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
fn test_adopt_command_removed() {
    let output = Command::new(concat!(env!("CARGO_MANIFEST_DIR"), "/target/debug/janus"))
        .args(["adopt", "github:foo/bar/1"])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success());
}

#[test]
fn test_push_command_removed() {
    let output = Command::new(concat!(env!("CARGO_MANIFEST_DIR"), "/target/debug/janus"))
        .args(["push", "j-1234"])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success());
}

#[test]
fn test_remote_link_command_removed() {
    let output = Command::new(concat!(env!("CARGO_MANIFEST_DIR"), "/target/debug/janus"))
        .args(["remote-link", "j-1234", "github:foo/bar/1"])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success());
}

#[test]
fn test_sync_command_removed() {
    let output = Command::new(concat!(env!("CARGO_MANIFEST_DIR"), "/target/debug/janus"))
        .args(["sync", "j-1234"])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success());
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
    let combined = format!("{}{}", stdout, stderr);
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

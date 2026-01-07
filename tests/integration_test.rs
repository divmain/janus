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
        "-a",
        "testuser",
        "--external-ref",
        "gh-123",
    ]);
    let id = output.trim();

    let content = janus.read_ticket(id);
    assert!(content.contains("# Bug ticket"));
    assert!(content.contains("This is a description"));
    assert!(content.contains("priority: 0"));
    assert!(content.contains("type: bug"));
    assert!(content.contains("assignee: testuser"));
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

    let output = janus.run_success(&["ready"]);
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
    let output = janus.run_success(&["ready"]);
    assert!(!output.contains(&blocked_id));

    // Close dependency
    janus.run_success(&["close", &dep_id]);

    // Now ready
    let output = janus.run_success(&["ready"]);
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

    let output = janus.run_success(&["blocked"]);

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

    let output = janus.run_success(&["closed"]);
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

    let output = janus.run_success(&["closed", "--limit", "2"]);
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines.len(), 2);
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

    let output = janus.run_success(&["ready"]);
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

    let stderr = janus.run_failure(&["adopt", "invalid"]);
    assert!(stderr.contains("invalid") || stderr.contains("expected"));
}

#[test]
fn test_adopt_with_reserved_prefix_fails() {
    let janus = JanusTest::new();

    let stderr = janus.run_failure(&["adopt", "github:test/test/123", "--prefix", "plan"]);
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
    let stderr = janus.run_failure(&["push", &id]);
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
    let stderr = janus.run_failure(&["remote-link", &id, "invalid"]);
    assert!(stderr.contains("invalid") || stderr.contains("expected"));
}

#[test]
fn test_sync_not_linked() {
    let janus = JanusTest::new();

    let id = janus.run_success(&["create", "Test"]).trim().to_string();
    let stderr = janus.run_failure(&["sync", &id]);
    assert!(stderr.contains("not linked"));
}

#[test]
fn test_help_shows_new_commands() {
    let janus = JanusTest::new();

    let output = janus.run_success(&["--help"]);
    assert!(output.contains("adopt"), "Should show adopt command");
    assert!(output.contains("push"), "Should show push command");
    assert!(
        output.contains("remote-link"),
        "Should show remote-link command"
    );
    assert!(output.contains("sync"), "Should show sync command");
    assert!(output.contains("config"), "Should show config command");
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

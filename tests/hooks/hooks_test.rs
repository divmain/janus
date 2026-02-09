#[path = "../common/mod.rs"]
mod common;

use common::JanusTest;
use janus::error::JanusError;
use serial_test::serial;
use std::fs;
use std::os::unix::fs::symlink;

// ============================================================================
// Hook Integration Tests
// ============================================================================

#[test]
#[serial]
fn test_hook_pre_write_aborts_ticket_create() {
    let janus = JanusTest::new();

    // Create a pre-write hook that always fails
    janus.write_hook_script("pre-write.sh", "#!/bin/sh\necho 'Abort!' >&2\nexit 1\n");

    // Create config to enable the hook
    janus.write_config(
        r#"
hooks:
  enabled: true
  timeout: 30
  scripts:
    pre_write: pre-write.sh
"#,
    );

    // Ticket creation should fail because pre-hook returns non-zero
    let stderr = janus.run_failure(&["create", "Test ticket"]);
    assert!(
        stderr.contains("pre-hook") || stderr.contains("failed"),
        "Error message should mention pre-hook failure: {stderr}"
    );
}

#[test]
#[serial]
fn test_hook_post_write_runs_after_ticket_create() {
    let janus = JanusTest::new();

    // Create a post-write hook that writes to a marker file
    let marker_file = janus.temp_dir.path().join("hook_ran.txt");
    let script_content = format!(
        r#"#!/bin/sh
echo "ITEM_TYPE=$JANUS_ITEM_TYPE" >> "{}"
echo "EVENT=$JANUS_EVENT" >> "{}"
echo "ITEM_ID=$JANUS_ITEM_ID" >> "{}"
exit 0
"#,
        marker_file.display(),
        marker_file.display(),
        marker_file.display()
    );
    janus.write_hook_script("post-write.sh", &script_content);

    // Create config to enable the hook
    janus.write_config(
        r#"
hooks:
  enabled: true
  timeout: 30
  scripts:
    post_write: post-write.sh
"#,
    );

    // Create a ticket
    let output = janus.run_success(&["create", "Test ticket"]);
    let id = output.trim();
    assert!(!id.is_empty(), "Should output a ticket ID");

    // Give the hook a moment to complete
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Check that the hook ran
    let marker_content = match fs::read_to_string(&marker_file) {
        Ok(content) => content,
        Err(e) => panic!(
            "Hook marker file should exist at {}: {}",
            marker_file.display(),
            e
        ),
    };
    assert!(
        marker_content.contains("ITEM_TYPE=ticket"),
        "Hook should receive ticket item type. Got: {marker_content}"
    );
    assert!(
        marker_content.contains("EVENT=post_write"),
        "Hook should receive post_write event. Got: {marker_content}"
    );
}

#[test]
#[serial]
fn test_hook_ticket_created_event_fires() {
    let janus = JanusTest::new();

    // Create a ticket_created hook that writes to a marker file
    let marker_file = janus.temp_dir.path().join("ticket_created.txt");
    let script_content = format!(
        r#"#!/bin/sh
echo "ITEM_TYPE=$JANUS_ITEM_TYPE" >> "{}"
echo "EVENT=$JANUS_EVENT" >> "{}"
exit 0
"#,
        marker_file.display(),
        marker_file.display()
    );
    janus.write_hook_script("ticket-created.sh", &script_content);

    // Create config to enable the hook
    janus.write_config(
        r#"
hooks:
  enabled: true
  timeout: 30
  scripts:
    ticket_created: ticket-created.sh
"#,
    );

    // Create a ticket
    let output = janus.run_success(&["create", "Test ticket"]);
    assert!(!output.trim().is_empty(), "Should output a ticket ID");

    // Give the hook a moment to complete
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Check that the hook ran
    let marker_content = fs::read_to_string(&marker_file).expect("Hook marker file should exist");
    assert!(
        marker_content.contains("EVENT=ticket_created"),
        "Hook should receive ticket_created event"
    );
}

#[test]
#[serial]
fn test_hook_pre_write_aborts_plan_create() {
    let janus = JanusTest::new();

    // Create a pre-write hook that always fails
    janus.write_hook_script(
        "pre-write.sh",
        "#!/bin/sh\necho 'Plan creation blocked!' >&2\nexit 1\n",
    );

    // Create config to enable the hook
    janus.write_config(
        r#"
hooks:
  enabled: true
  timeout: 30
  scripts:
    pre_write: pre-write.sh
"#,
    );

    // Plan creation should fail because pre-hook returns non-zero
    let stderr = janus.run_failure(&["plan", "create", "Test plan"]);
    assert!(
        stderr.contains("pre-hook") || stderr.contains("failed"),
        "Error message should mention pre-hook failure: {stderr}"
    );
}

#[test]
#[serial]
fn test_hook_plan_created_event_fires() {
    let janus = JanusTest::new();

    // Create a plan_created hook that writes to a marker file
    let marker_file = janus.temp_dir.path().join("plan_created.txt");
    let script_content = format!(
        r#"#!/bin/sh
echo "ITEM_TYPE=$JANUS_ITEM_TYPE" >> "{}"
echo "EVENT=$JANUS_EVENT" >> "{}"
exit 0
"#,
        marker_file.display(),
        marker_file.display()
    );
    janus.write_hook_script("plan-created.sh", &script_content);

    // Create config to enable the hook
    janus.write_config(
        r#"
hooks:
  enabled: true
  timeout: 30
  scripts:
    plan_created: plan-created.sh
"#,
    );

    // Create a plan
    let output = janus.run_success(&["plan", "create", "Test plan"]);
    assert!(!output.trim().is_empty(), "Should output a plan ID");

    // Give the hook a moment to complete
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Check that the hook ran
    let marker_content = fs::read_to_string(&marker_file).expect("Hook marker file should exist");
    assert!(
        marker_content.contains("EVENT=plan_created"),
        "Hook should receive plan_created event"
    );
    assert!(
        marker_content.contains("ITEM_TYPE=plan"),
        "Hook should receive plan item type"
    );
}

#[test]
#[serial]
fn test_hook_pre_delete_aborts_plan_delete() {
    let janus = JanusTest::new();

    // First create a plan
    let plan_id = janus
        .run_success(&["plan", "create", "Test plan"])
        .trim()
        .to_string();

    // Create a pre-delete hook that always fails
    janus.write_hook_script(
        "pre-delete.sh",
        "#!/bin/sh\necho 'Delete blocked!' >&2\nexit 1\n",
    );

    // Create config to enable the hook
    janus.write_config(
        r#"
hooks:
  enabled: true
  timeout: 30
  scripts:
    pre_delete: pre-delete.sh
"#,
    );

    // Plan deletion should fail because pre-hook returns non-zero
    let stderr = janus.run_failure(&["plan", "delete", &plan_id, "--force"]);
    assert!(
        stderr.contains("pre-hook") || stderr.contains("failed"),
        "Error message should mention pre-hook failure: {stderr}"
    );

    // Plan should still exist
    assert!(janus.plan_exists(&plan_id), "Plan should still exist");
}

#[test]
#[serial]
fn test_hook_plan_deleted_event_fires() {
    let janus = JanusTest::new();

    // First create a plan
    let plan_id = janus
        .run_success(&["plan", "create", "Test plan"])
        .trim()
        .to_string();

    // Create a plan_deleted hook that writes to a marker file
    let marker_file = janus.temp_dir.path().join("plan_deleted.txt");
    let script_content = format!(
        r#"#!/bin/sh
echo "ITEM_TYPE=$JANUS_ITEM_TYPE" >> "{}"
echo "EVENT=$JANUS_EVENT" >> "{}"
echo "ITEM_ID=$JANUS_ITEM_ID" >> "{}"
exit 0
"#,
        marker_file.display(),
        marker_file.display(),
        marker_file.display()
    );
    janus.write_hook_script("plan-deleted.sh", &script_content);

    // Create config to enable the hook
    janus.write_config(
        r#"
hooks:
  enabled: true
  timeout: 30
  scripts:
    plan_deleted: plan-deleted.sh
"#,
    );

    // Delete the plan
    janus.run_success(&["plan", "delete", &plan_id, "--force"]);

    // Give the hook a moment to complete
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Check that the hook ran
    let marker_content = fs::read_to_string(&marker_file).expect("Hook marker file should exist");
    assert!(
        marker_content.contains("EVENT=plan_deleted"),
        "Hook should receive plan_deleted event"
    );
    assert!(
        marker_content.contains(&format!("ITEM_ID={plan_id}")),
        "Hook should receive the plan ID"
    );
}

#[test]
#[serial]
fn test_hook_ticket_updated_on_status_change() {
    let janus = JanusTest::new();

    // First create a ticket
    let ticket_id = janus
        .run_success(&["create", "Test ticket"])
        .trim()
        .to_string();

    // Create a ticket_updated hook that writes to a marker file
    let marker_file = janus.temp_dir.path().join("ticket_updated.txt");
    let script_content = format!(
        r#"#!/bin/sh
echo "EVENT=$JANUS_EVENT" >> "{}"
echo "FIELD_NAME=$JANUS_FIELD_NAME" >> "{}"
echo "OLD_VALUE=$JANUS_OLD_VALUE" >> "{}"
echo "NEW_VALUE=$JANUS_NEW_VALUE" >> "{}"
exit 0
"#,
        marker_file.display(),
        marker_file.display(),
        marker_file.display(),
        marker_file.display()
    );
    janus.write_hook_script("ticket-updated.sh", &script_content);

    // Create config to enable the hook
    janus.write_config(
        r#"
hooks:
  enabled: true
  timeout: 30
  scripts:
    ticket_updated: ticket-updated.sh
"#,
    );

    // Change the ticket status
    janus.run_success(&["status", &ticket_id, "in_progress"]);

    // Give the hook a moment to complete
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Check that the hook ran
    let marker_content = fs::read_to_string(&marker_file).expect("Hook marker file should exist");
    assert!(
        marker_content.contains("EVENT=ticket_updated"),
        "Hook should receive ticket_updated event"
    );
    assert!(
        marker_content.contains("FIELD_NAME=status"),
        "Hook should receive field name"
    );
    assert!(
        marker_content.contains("NEW_VALUE=in_progress"),
        "Hook should receive new value"
    );
}

#[test]
#[serial]
fn test_hook_disabled_does_not_run() {
    let janus = JanusTest::new();

    // Create a pre-write hook that would fail if run
    janus.write_hook_script(
        "pre-write.sh",
        "#!/bin/sh\necho 'Should not run!' >&2\nexit 1\n",
    );

    // Create config with hooks DISABLED
    janus.write_config(
        r#"
hooks:
  enabled: false
  timeout: 30
  scripts:
    pre_write: pre-write.sh
"#,
    );

    // Ticket creation should succeed because hooks are disabled
    let output = janus.run_success(&["create", "Test ticket"]);
    let id = output.trim();
    assert!(!id.is_empty(), "Should output a ticket ID");
    assert!(janus.ticket_exists(id), "Ticket file should exist");
}

// ============================================================================
// Tests moved from src/commands/hook.rs (Phase 5)
// ============================================================================

use janus::commands::{cmd_hook_disable, cmd_hook_enable, cmd_hook_list, cmd_hook_run};
use janus::remote::Config;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

fn setup_test_env_hooks() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let janus_dir = temp_dir.path().join(".janus");
    let hooks_dir = janus_dir.join("hooks");
    fs::create_dir_all(&hooks_dir).unwrap();
    temp_dir
}

#[test]
#[serial]
fn test_hook_list_no_config() {
    let temp_dir = setup_test_env_hooks();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    // Should succeed with default config
    let result = cmd_hook_list(false);
    assert!(result.is_ok());
}

#[test]
#[serial]
fn test_hook_list_with_config() {
    let temp_dir = setup_test_env_hooks();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    // Create config with hooks
    let config_content = r#"
hooks:
  enabled: true
  timeout: 60
  scripts:
    post_write: post-write.sh
    ticket_created: on-created.sh
"#;
    let config_path = temp_dir.path().join(".janus/config.yaml");
    fs::write(&config_path, config_content).unwrap();

    let result = cmd_hook_list(false);
    assert!(result.is_ok());
}

#[test]
#[serial]
fn test_hook_list_json() {
    let temp_dir = setup_test_env_hooks();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let config_content = r#"
hooks:
  enabled: false
  timeout: 45
  scripts:
    pre_write: pre-write.sh
"#;
    let config_path = temp_dir.path().join(".janus/config.yaml");
    fs::write(&config_path, config_content).unwrap();

    let result = cmd_hook_list(true);
    assert!(result.is_ok());
}

#[test]
#[serial]
fn test_hook_enable() {
    let temp_dir = setup_test_env_hooks();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    // Start with hooks disabled
    let config_content = r#"
hooks:
  enabled: false
"#;
    let config_path = temp_dir.path().join(".janus/config.yaml");
    fs::write(&config_path, config_content).unwrap();

    let result = cmd_hook_enable(false);
    assert!(result.is_ok());

    // Verify config was updated
    let config = Config::load().unwrap();
    assert!(config.hooks.enabled);
}

#[test]
#[serial]
fn test_hook_disable() {
    let temp_dir = setup_test_env_hooks();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    // Start with hooks enabled (default)
    let config_path = temp_dir.path().join(".janus/config.yaml");
    fs::write(&config_path, "").unwrap();

    let result = cmd_hook_disable(false);
    assert!(result.is_ok());

    // Verify config was updated
    let config = Config::load().unwrap();
    assert!(!config.hooks.enabled);
}

#[test]
#[serial]
fn test_hook_enable_json() {
    let temp_dir = setup_test_env_hooks();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = cmd_hook_enable(true);
    assert!(result.is_ok());
}

#[test]
#[serial]
fn test_hook_disable_json() {
    let temp_dir = setup_test_env_hooks();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = cmd_hook_disable(true);
    assert!(result.is_ok());
}

#[tokio::test]
#[serial]
async fn test_hook_run_no_script_configured() {
    let temp_dir = setup_test_env_hooks();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    // No hooks configured
    let result = cmd_hook_run("post_write", None).await;
    assert!(result.is_err());
}

#[tokio::test]
#[serial]
async fn test_hook_run_script_not_found() {
    let temp_dir = setup_test_env_hooks();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    // Configure a hook that points to non-existent script
    let config_content = r#"
hooks:
  enabled: true
  scripts:
    post_write: nonexistent.sh
"#;
    let config_path = temp_dir.path().join(".janus/config.yaml");
    fs::write(&config_path, config_content).unwrap();

    let result = cmd_hook_run("post_write", None).await;
    assert!(matches!(result, Err(JanusError::HookScriptNotFound(_))));
}

#[tokio::test]
#[serial]
async fn test_hook_run_success() {
    let temp_dir = setup_test_env_hooks();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    // Create a successful hook script
    let hooks_dir = temp_dir.path().join(".janus/hooks");
    let script_path = hooks_dir.join("test-hook.sh");
    fs::write(&script_path, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();

    // Configure the hook
    let config_content = r#"
hooks:
  enabled: true
  scripts:
    post_write: test-hook.sh
"#;
    let config_path = temp_dir.path().join(".janus/config.yaml");
    fs::write(&config_path, config_content).unwrap();

    let result = cmd_hook_run("post_write", None).await;
    assert!(result.is_ok(), "Expected Ok, got: {result:?}");
}

#[tokio::test]
async fn test_hook_run_invalid_event() {
    let result = cmd_hook_run("invalid_event", None).await;
    assert!(matches!(result, Err(JanusError::InvalidHookEvent(_))));
}

#[tokio::test]
async fn test_invalid_hook_event_error_message() {
    let result = cmd_hook_run("not_a_real_event", None).await;
    match result {
        Err(JanusError::InvalidHookEvent(event)) => {
            assert_eq!(event, "not_a_real_event");
        }
        other => panic!("Expected InvalidHookEvent, got: {other:?}"),
    }
}

#[test]
#[serial]
#[cfg(unix)]
fn test_hook_symlink_escape_blocked() {
    let janus = JanusTest::new();

    // Create a malicious script outside the hooks directory
    let malicious_script = janus.temp_dir.path().join("malicious.sh");
    fs::write(
        &malicious_script,
        "#!/bin/sh\necho 'MALICIOUS CODE EXECUTED'\nexit 0\n",
    )
    .expect("Failed to write malicious script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&malicious_script, fs::Permissions::from_mode(0o755))
            .expect("Failed to set permissions");
    }

    // Create a symlink in hooks directory that points outside
    let hooks_dir = janus.temp_dir.path().join(".janus").join("hooks");
    fs::create_dir_all(&hooks_dir).expect("Failed to create hooks directory");
    let symlink_path = hooks_dir.join("pre-write.sh");
    #[cfg(unix)]
    {
        symlink(&malicious_script, &symlink_path).expect("Failed to create symlink");
    }

    // Create config to enable the hook
    janus.write_config(
        r#"
hooks:
  enabled: true
  timeout: 30
  scripts:
    pre_write: pre-write.sh
"#,
    );

    // Ticket creation should fail because hook resolves outside hooks directory
    let stderr = janus.run_failure(&["create", "Test ticket"]);
    assert!(
        stderr.contains("security violation") || stderr.contains("outside hooks directory"),
        "Error should mention security violation: {stderr}"
    );
    // Ensure the malicious script was NOT executed
    assert!(!stderr.contains("MALICIOUS CODE EXECUTED"));
}

#[test]
#[serial]
#[cfg(unix)]
fn test_hook_run_symlink_escape_blocked() {
    let janus = JanusTest::new();

    // First, create a ticket WITHOUT the hook enabled to get an ID
    janus.write_config(
        r#"
hooks:
  enabled: false
"#,
    );
    let output = janus.run_success(&["create", "Test ticket"]);
    let ticket_id = output.trim();

    // Create a malicious script outside the hooks directory
    let malicious_script = janus.temp_dir.path().join("malicious.sh");
    fs::write(
        &malicious_script,
        "#!/bin/sh\necho 'MALICIOUS CODE EXECUTED'\nexit 0\n",
    )
    .expect("Failed to write malicious script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&malicious_script, fs::Permissions::from_mode(0o755))
            .expect("Failed to set permissions");
    }

    // Create a symlink in hooks directory that points outside
    let hooks_dir = janus.temp_dir.path().join(".janus").join("hooks");
    fs::create_dir_all(&hooks_dir).expect("Failed to create hooks directory");
    let symlink_path = hooks_dir.join("pre-write.sh");
    #[cfg(unix)]
    {
        symlink(&malicious_script, &symlink_path).expect("Failed to create symlink");
    }

    // Create config to enable the hook
    janus.write_config(
        r#"
hooks:
  enabled: true
  timeout: 30
  scripts:
    pre_write: pre-write.sh
"#,
    );

    // Try to run the hook manually - should fail with security error
    let stderr = janus.run_failure(&["hook", "run", "pre_write", "--id", ticket_id]);
    assert!(
        stderr.contains("security violation") || stderr.contains("outside hooks directory"),
        "Error should mention security violation: {stderr}"
    );
    // Ensure the malicious script was NOT executed
    assert!(!stderr.contains("MALICIOUS CODE EXECUTED"));
}

// ============================================================================
// Plan Import Hook Exactly-Once Semantics Tests
// ============================================================================

#[test]
#[serial]
fn test_plan_import_post_write_fires_exactly_once() {
    let janus = JanusTest::new();

    // Create a post_write hook that appends to a counter file each time it fires
    let counter_file = janus.temp_dir.path().join("post_write_count.txt");
    let script_content = format!(
        r#"#!/bin/sh
echo "POST_WRITE:$JANUS_ITEM_TYPE:$JANUS_ITEM_ID" >> "{}"
exit 0
"#,
        counter_file.display()
    );
    janus.write_hook_script("post-write.sh", &script_content);

    janus.write_config(
        r#"
hooks:
  enabled: true
  timeout: 30
  scripts:
    post_write: post-write.sh
"#,
    );

    // Create a plan document to import
    let plan_doc = r#"# Hook Test Plan

Description.

## Design

Design details.

## Implementation

### Phase 1: Setup

#### Task One

First task.
"#;
    let plan_path = janus.temp_dir.path().join("hook_test_plan.md");
    fs::write(&plan_path, plan_doc).expect("Failed to write plan file");

    // Import the plan
    let output = janus.run_success(&["plan", "import", plan_path.to_str().unwrap()]);
    let plan_id = output.trim();
    assert!(plan_id.starts_with("plan-"), "Should return a plan ID");

    // Give hooks a moment to complete
    std::thread::sleep(std::time::Duration::from_millis(200));

    // Read counter file and count how many times post_write fired for the plan
    let counter_content = fs::read_to_string(&counter_file).expect("Counter file should exist");
    let plan_post_write_count = counter_content
        .lines()
        .filter(|line| line.contains("POST_WRITE:plan:"))
        .count();

    assert_eq!(
        plan_post_write_count, 1,
        "PostWrite hook should fire exactly once for the plan import. \
         Got {plan_post_write_count} firings. Content:\n{counter_content}"
    );
}

#[test]
#[serial]
fn test_plan_import_plan_created_fires_exactly_once() {
    let janus = JanusTest::new();

    // Create a plan_created hook that appends to a counter file each time it fires
    let counter_file = janus.temp_dir.path().join("plan_created_count.txt");
    let script_content = format!(
        r#"#!/bin/sh
echo "PLAN_CREATED:$JANUS_ITEM_TYPE:$JANUS_ITEM_ID" >> "{}"
exit 0
"#,
        counter_file.display()
    );
    janus.write_hook_script("plan-created.sh", &script_content);

    janus.write_config(
        r#"
hooks:
  enabled: true
  timeout: 30
  scripts:
    plan_created: plan-created.sh
"#,
    );

    // Create a plan document to import
    let plan_doc = r#"# Created Hook Test Plan

Description.

## Design

Design details.

## Implementation

### Phase 1: Setup

#### Task One

First task.
"#;
    let plan_path = janus.temp_dir.path().join("created_hook_plan.md");
    fs::write(&plan_path, plan_doc).expect("Failed to write plan file");

    // Import the plan
    let output = janus.run_success(&["plan", "import", plan_path.to_str().unwrap()]);
    let plan_id = output.trim();
    assert!(plan_id.starts_with("plan-"), "Should return a plan ID");

    // Give hooks a moment to complete
    std::thread::sleep(std::time::Duration::from_millis(200));

    // Read counter file and count how many times plan_created fired
    let counter_content = fs::read_to_string(&counter_file).expect("Counter file should exist");
    let plan_created_count = counter_content
        .lines()
        .filter(|line| line.starts_with("PLAN_CREATED:"))
        .count();

    assert_eq!(
        plan_created_count, 1,
        "PlanCreated hook should fire exactly once for the plan import. \
         Got {plan_created_count} firings. Content:\n{counter_content}"
    );
}

#[test]
#[serial]
fn test_plan_import_does_not_fire_plan_updated() {
    let janus = JanusTest::new();

    // Create a plan_updated hook that writes to a marker file
    let marker_file = janus.temp_dir.path().join("plan_updated_marker.txt");
    let script_content = format!(
        r#"#!/bin/sh
echo "PLAN_UPDATED:$JANUS_ITEM_TYPE:$JANUS_ITEM_ID" >> "{}"
exit 0
"#,
        marker_file.display()
    );
    janus.write_hook_script("plan-updated.sh", &script_content);

    janus.write_config(
        r#"
hooks:
  enabled: true
  timeout: 30
  scripts:
    plan_updated: plan-updated.sh
"#,
    );

    // Create a plan document to import
    let plan_doc = r#"# Updated Hook Test Plan

Description.

## Design

Design details.

## Implementation

### Phase 1: Setup

#### Task One

First task.
"#;
    let plan_path = janus.temp_dir.path().join("updated_hook_plan.md");
    fs::write(&plan_path, plan_doc).expect("Failed to write plan file");

    // Import the plan
    let output = janus.run_success(&["plan", "import", plan_path.to_str().unwrap()]);
    let plan_id = output.trim();
    assert!(plan_id.starts_with("plan-"), "Should return a plan ID");

    // Give hooks a moment to complete
    std::thread::sleep(std::time::Duration::from_millis(200));

    // PlanUpdated should NOT fire during import — it's a create, not an update
    assert!(
        !marker_file.exists(),
        "PlanUpdated hook should NOT fire during plan import. \
         Import creates a new plan, so only PlanCreated should fire. \
         Marker file content: {:?}",
        fs::read_to_string(&marker_file).ok()
    );
}

#[test]
#[serial]
fn test_plan_import_pre_write_fires_exactly_once() {
    let janus = JanusTest::new();

    // Create a pre_write hook that appends to a counter file and succeeds
    let counter_file = janus.temp_dir.path().join("pre_write_count.txt");
    let script_content = format!(
        r#"#!/bin/sh
echo "PRE_WRITE:$JANUS_ITEM_TYPE:$JANUS_ITEM_ID" >> "{}"
exit 0
"#,
        counter_file.display()
    );
    janus.write_hook_script("pre-write.sh", &script_content);

    janus.write_config(
        r#"
hooks:
  enabled: true
  timeout: 30
  scripts:
    pre_write: pre-write.sh
"#,
    );

    // Create a plan document to import
    let plan_doc = r#"# Pre-Write Hook Test Plan

Description.

## Design

Design details.

## Implementation

### Phase 1: Setup

#### Task One

First task.
"#;
    let plan_path = janus.temp_dir.path().join("pre_write_hook_plan.md");
    fs::write(&plan_path, plan_doc).expect("Failed to write plan file");

    // Import the plan
    let output = janus.run_success(&["plan", "import", plan_path.to_str().unwrap()]);
    let plan_id = output.trim();
    assert!(plan_id.starts_with("plan-"), "Should return a plan ID");

    // Give hooks a moment to complete
    std::thread::sleep(std::time::Duration::from_millis(200));

    // Read counter file and count how many times pre_write fired for the plan
    let counter_content = fs::read_to_string(&counter_file).expect("Counter file should exist");
    let plan_pre_write_count = counter_content
        .lines()
        .filter(|line| line.contains("PRE_WRITE:plan:"))
        .count();

    assert_eq!(
        plan_pre_write_count, 1,
        "PreWrite hook should fire exactly once for the plan during import. \
         Got {plan_pre_write_count} firings. Content:\n{counter_content}"
    );
}

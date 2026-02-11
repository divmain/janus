//! Hook execution system for Janus.
//!
//! This module provides functionality to run user-defined scripts before and after
//! Janus operations. Hooks are configured in `.janus/config.yaml` and scripts live
//! in `.janus/hooks/`.
//!
//! # Hook Types
//!
//! - **Pre-hooks** (`pre_write`, `pre_delete`): Run before operations and can abort
//!   them by returning a non-zero exit code.
//! - **Post-hooks** (`post_write`, `post_delete`, `*_created`, `*_updated`, `*_deleted`):
//!   Run after operations. Failures are logged as warnings but don't abort.
//!
//! # Hook Failure Logging
//!
//! Post-hook failures are automatically logged to `.janus/hooks.log` with timestamps
//! for later review. This provides observability in automated environments where
//! stderr output might be lost. Use `janus hook log` to view the failure log.
//!
//! # Environment Variables
//!
//! Hook scripts receive context via environment variables:
//! - `JANUS_EVENT`: The event name (e.g., "ticket_created")
//! - `JANUS_ITEM_TYPE`: The item type ("ticket" or "plan")
//! - `JANUS_ITEM_ID`: The item ID (e.g., "j-1234")
//! - `JANUS_FILE_PATH`: Path to the item file
//! - `JANUS_FIELD_NAME`: Field being modified (for updates)
//! - `JANUS_OLD_VALUE`: Previous value (for updates)
//! - `JANUS_NEW_VALUE`: New value (for updates)
//! - `JANUS_ROOT`: Path to the .janus directory

mod runner;
pub mod types;

pub use runner::context_to_env;
pub use types::{HookContext, HookEvent};

pub use crate::types::EntityType;

use crate::config::Config;
use crate::error::Result;
use runner::{execute_hook, execute_hook_async, log_hook_failure};

/// Run pre-operation hooks for the given event.
///
/// Pre-hooks can abort the operation by returning a non-zero exit code.
/// If any pre-hook fails, this function returns an error.
///
/// # Arguments
/// * `event` - The hook event to run
/// * `context` - The context to pass to the hook script
///
/// # Returns
/// * `Ok(())` if all hooks succeeded or no hooks are configured
/// * `Err(JanusError::PreHookFailed)` if a hook failed
pub fn run_pre_hooks(event: HookEvent, context: &HookContext) -> Result<()> {
    if !event.is_pre_hook() {
        return Ok(());
    }

    let config = Config::load()?;
    if !config.hooks.enabled {
        return Ok(());
    }

    if let Some(script_name) = config.hooks.get_script(event.as_str()) {
        execute_hook(event, script_name, context, &config, true)?;
    }

    Ok(())
}

/// Run post-operation hooks for the given event.
///
/// Post-hooks run after the operation completes. Failures are logged as warnings
/// but do not return errors.
///
/// # Arguments
/// * `event` - The hook event to run
/// * `context` - The context to pass to the hook script
pub fn run_post_hooks(event: HookEvent, context: &HookContext) {
    if event.is_pre_hook() {
        return;
    }

    let config = match Config::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Warning: failed to load config for hooks: {e}");
            return;
        }
    };

    if !config.hooks.enabled {
        return;
    }

    if let Some(script_name) = config.hooks.get_script(event.as_str())
        && let Err(e) = execute_hook(event, script_name, context, &config, false)
    {
        log_hook_failure(script_name, &e);
        eprintln!("Warning: post-hook '{script_name}' failed: {e}");
    }
}

/// Run pre-operation hooks for the given event (async version).
///
/// Pre-hooks can abort the operation by returning a non-zero exit code.
/// If any pre-hook fails, this function returns an error.
///
/// # Arguments
/// * `event` - The hook event to run
/// * `context` - The context to pass to the hook script
///
/// # Returns
/// * `Ok(())` if all hooks succeeded or no hooks are configured
/// * `Err(JanusError::PreHookFailed)` if a hook failed
pub async fn run_pre_hooks_async(event: HookEvent, context: &HookContext) -> Result<()> {
    if !event.is_pre_hook() {
        return Ok(());
    }

    let config = Config::load()?;
    if !config.hooks.enabled {
        return Ok(());
    }

    if let Some(script_name) = config.hooks.get_script(event.as_str()) {
        execute_hook_async(event, script_name, context, &config, true).await?;
    }

    Ok(())
}

/// Run post-operation hooks for the given event (async version).
///
/// Post-hooks run after the operation completes. Failures are logged as warnings
/// but do not return errors.
///
/// # Arguments
/// * `event` - The hook event to run
/// * `context` - The context to pass to the hook script
pub async fn run_post_hooks_async(event: HookEvent, context: &HookContext) {
    if event.is_pre_hook() {
        return;
    }

    let config = match Config::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Warning: failed to load config for hooks: {e}");
            return;
        }
    };

    if !config.hooks.enabled {
        return;
    }

    if let Some(script_name) = config.hooks.get_script(event.as_str())
        && let Err(e) = execute_hook_async(event, script_name, context, &config, false).await
    {
        log_hook_failure(script_name, &e);
        eprintln!("Warning: post-hook '{script_name}' failed: {e}");
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    use tempfile::TempDir;

    use crate::error::JanusError;
    use crate::paths::JanusRootGuard;

    fn setup_test_env() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        let janus_dir = temp_dir.path().join(".janus");
        let hooks_dir = janus_dir.join("hooks");
        fs::create_dir_all(&hooks_dir).unwrap();
        temp_dir
    }

    #[test]
    fn test_context_to_env_full() {
        let context = HookContext::new()
            .with_event(HookEvent::TicketCreated)
            .with_item_type(EntityType::Ticket)
            .with_item_id("j-1234")
            .with_file_path("/path/to/ticket.md")
            .with_field_name("status")
            .with_old_value("new")
            .with_new_value("complete");

        let janus_root = PathBuf::from(".janus");
        let env = context_to_env(&context, &janus_root);

        assert_eq!(env.get("JANUS_EVENT"), Some(&"ticket_created".to_string()));
        assert_eq!(env.get("JANUS_ITEM_TYPE"), Some(&"ticket".to_string()));
        assert_eq!(env.get("JANUS_ITEM_ID"), Some(&"j-1234".to_string()));
        assert_eq!(
            env.get("JANUS_FILE_PATH"),
            Some(&"/path/to/ticket.md".to_string())
        );
        assert_eq!(env.get("JANUS_FIELD_NAME"), Some(&"status".to_string()));
        assert_eq!(env.get("JANUS_OLD_VALUE"), Some(&"new".to_string()));
        assert_eq!(env.get("JANUS_NEW_VALUE"), Some(&"complete".to_string()));
        assert_eq!(env.get("JANUS_ROOT"), Some(&".janus".to_string()));
    }

    #[test]
    fn test_context_to_env_minimal() {
        let context = HookContext::new().with_event(HookEvent::PostWrite);

        let janus_root = PathBuf::from(".janus");
        let env = context_to_env(&context, &janus_root);

        assert_eq!(env.get("JANUS_EVENT"), Some(&"post_write".to_string()));
        assert_eq!(env.get("JANUS_ITEM_TYPE"), None);
        assert_eq!(env.get("JANUS_ITEM_ID"), None);
        assert_eq!(env.get("JANUS_ROOT"), Some(&".janus".to_string()));
    }

    #[test]
    fn test_run_pre_hooks_no_config() {
        // When there's no config file, hooks should succeed silently
        let temp_dir = setup_test_env();
        let _guard = JanusRootGuard::new(temp_dir.path().join(".janus"));

        let context = HookContext::new()
            .with_event(HookEvent::PreWrite)
            .with_item_type(EntityType::Ticket)
            .with_item_id("j-1234");

        // Should succeed even without config
        let result = run_pre_hooks(HookEvent::PreWrite, &context);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_post_hooks_no_config() {
        let temp_dir = setup_test_env();
        let _guard = JanusRootGuard::new(temp_dir.path().join(".janus"));

        let context = HookContext::new()
            .with_event(HookEvent::PostWrite)
            .with_item_type(EntityType::Ticket)
            .with_item_id("j-1234");

        // Should succeed even without config
        run_post_hooks(HookEvent::PostWrite, &context);
        // No assertion needed - post hooks don't return errors
    }

    #[test]
    fn test_hooks_disabled() {
        let temp_dir = setup_test_env();
        let _guard = JanusRootGuard::new(temp_dir.path().join(".janus"));

        // Create config with hooks disabled
        let config_content = r#"
hooks:
  enabled: false
  scripts:
    pre_write: should-not-run.sh
"#;
        let config_path = temp_dir.path().join(".janus/config.yaml");
        fs::write(&config_path, config_content).unwrap();

        let context = HookContext::new()
            .with_event(HookEvent::PreWrite)
            .with_item_type(EntityType::Ticket);

        // Should succeed because hooks are disabled
        let result = run_pre_hooks(HookEvent::PreWrite, &context);
        assert!(result.is_ok());
    }

    #[test]
    fn test_hook_script_not_found() {
        let temp_dir = setup_test_env();
        let _guard = JanusRootGuard::new(temp_dir.path().join(".janus"));

        // Create config pointing to non-existent script
        let config_content = r#"
hooks:
  enabled: true
  scripts:
    pre_write: nonexistent.sh
"#;
        let config_path = temp_dir.path().join(".janus/config.yaml");
        fs::write(&config_path, config_content).unwrap();

        let context = HookContext::new()
            .with_event(HookEvent::PreWrite)
            .with_item_type(EntityType::Ticket);

        let result = run_pre_hooks(HookEvent::PreWrite, &context);
        assert!(matches!(result, Err(JanusError::HookScriptNotFound(_))));
    }

    #[test]
    fn test_pre_hook_success() {
        let temp_dir = setup_test_env();
        let _guard = JanusRootGuard::new(temp_dir.path().join(".janus"));

        // Create a successful hook script
        let hooks_dir = temp_dir.path().join(".janus/hooks");
        let script_path = hooks_dir.join("pre-write.sh");
        fs::write(&script_path, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();

        // Create config
        let config_content = r#"
hooks:
  enabled: true
  scripts:
    pre_write: pre-write.sh
"#;
        let config_path = temp_dir.path().join(".janus/config.yaml");
        fs::write(&config_path, config_content).unwrap();

        let context = HookContext::new()
            .with_event(HookEvent::PreWrite)
            .with_item_type(EntityType::Ticket)
            .with_item_id("j-1234");

        let result = run_pre_hooks(HookEvent::PreWrite, &context);
        assert!(result.is_ok(), "Expected Ok, got: {result:?}");
    }

    #[test]
    fn test_pre_hook_failure() {
        let temp_dir = setup_test_env();
        let _guard = JanusRootGuard::new(temp_dir.path().join(".janus"));

        // Create a failing hook script
        let hooks_dir = temp_dir.path().join(".janus/hooks");
        let script_path = hooks_dir.join("pre-write.sh");
        fs::write(&script_path, "#!/bin/sh\nexit 1\n").unwrap();
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();

        // Create config
        let config_content = r#"
hooks:
  enabled: true
  scripts:
    pre_write: pre-write.sh
"#;
        let config_path = temp_dir.path().join(".janus/config.yaml");
        fs::write(&config_path, config_content).unwrap();

        let context = HookContext::new()
            .with_event(HookEvent::PreWrite)
            .with_item_type(EntityType::Ticket);

        let result = run_pre_hooks(HookEvent::PreWrite, &context);
        assert!(matches!(result, Err(JanusError::PreHookFailed { .. })));
    }

    #[test]
    fn test_post_hook_receives_env_vars() {
        let temp_dir = setup_test_env();
        let _guard = JanusRootGuard::new(temp_dir.path().join(".janus"));

        // Create a hook script that writes env vars to a file
        let hooks_dir = temp_dir.path().join(".janus/hooks");
        let script_path = hooks_dir.join("post-write.sh");
        let output_file = temp_dir.path().join("env_output.txt");
        let script_content = format!(
            r#"#!/bin/sh
echo "EVENT=$JANUS_EVENT" >> "{}"
echo "ITEM_TYPE=$JANUS_ITEM_TYPE" >> "{}"
echo "ITEM_ID=$JANUS_ITEM_ID" >> "{}"
exit 0
"#,
            output_file.display(),
            output_file.display(),
            output_file.display()
        );
        fs::write(&script_path, script_content).unwrap();
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();

        // Create config
        let config_content = r#"
hooks:
  enabled: true
  scripts:
    post_write: post-write.sh
"#;
        let config_path = temp_dir.path().join(".janus/config.yaml");
        fs::write(&config_path, config_content).unwrap();

        let context = HookContext::new()
            .with_event(HookEvent::PostWrite)
            .with_item_type(EntityType::Ticket)
            .with_item_id("j-test");

        run_post_hooks(HookEvent::PostWrite, &context);

        // Give it a moment to complete
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Check the output file
        let output = fs::read_to_string(&output_file).unwrap();
        assert!(output.contains("EVENT=post_write"));
        assert!(output.contains("ITEM_TYPE=ticket"));
        assert!(output.contains("ITEM_ID=j-test"));
    }

    #[test]
    fn test_hook_timeout() {
        let temp_dir = setup_test_env();
        let _guard = JanusRootGuard::new(temp_dir.path().join(".janus"));

        // Create a hook script that sleeps
        let hooks_dir = temp_dir.path().join(".janus/hooks");
        let script_path = hooks_dir.join("slow-hook.sh");
        fs::write(&script_path, "#!/bin/sh\nsleep 10\nexit 0\n").unwrap();
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();

        // Create config with 1 second timeout
        let config_content = r#"
hooks:
  enabled: true
  timeout: 1
  scripts:
    pre_write: slow-hook.sh
"#;
        let config_path = temp_dir.path().join(".janus/config.yaml");
        fs::write(&config_path, config_content).unwrap();

        let context = HookContext::new()
            .with_event(HookEvent::PreWrite)
            .with_item_type(EntityType::Ticket);

        let result = run_pre_hooks(HookEvent::PreWrite, &context);
        assert!(matches!(result, Err(JanusError::HookTimeout { .. })));
    }

    #[test]
    fn test_hooks_config_default() {
        use crate::config::HooksConfig;

        let config = HooksConfig::default();
        assert!(config.enabled);
        assert_eq!(config.timeout, 30);
        assert!(config.scripts.is_empty());
        assert!(config.is_default());
    }

    #[test]
    fn test_hooks_config_is_default() {
        use crate::config::HooksConfig;

        let mut config = HooksConfig::default();
        assert!(config.is_default());

        config.enabled = false;
        assert!(!config.is_default());

        config.enabled = true;
        config.timeout = 60;
        assert!(!config.is_default());

        config.timeout = 30;
        config
            .scripts
            .insert("pre_write".to_string(), "script.sh".to_string());
        assert!(!config.is_default());
    }

    #[test]
    fn test_pre_hook_failure_aborts_operation() {
        let temp_dir = setup_test_env();
        let _guard = JanusRootGuard::new(temp_dir.path().join(".janus"));

        // Create a failing hook script with specific exit code and error message
        let hooks_dir = temp_dir.path().join(".janus/hooks");
        let script_path = hooks_dir.join("pre-write.sh");
        fs::write(
            &script_path,
            "#!/bin/sh\necho 'validation failed' >&2\nexit 42\n",
        )
        .unwrap();
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();

        // Create config
        let config_content = r#"
hooks:
  enabled: true
  timeout: 0
  scripts:
    pre_write: pre-write.sh
"#;
        let config_path = temp_dir.path().join(".janus/config.yaml");
        fs::write(&config_path, config_content).unwrap();

        let context = HookContext::new()
            .with_event(HookEvent::PreWrite)
            .with_item_type(EntityType::Ticket)
            .with_item_id("j-test");

        let result = run_pre_hooks(HookEvent::PreWrite, &context);

        // Verify the error contains the expected information
        match result {
            Err(JanusError::PreHookFailed {
                hook_name,
                exit_code,
                message,
            }) => {
                assert_eq!(hook_name, "pre-write.sh");
                assert_eq!(exit_code, 42);
                assert!(message.contains("validation failed"));
            }
            other => panic!("Expected PreHookFailed, got: {other:?}"),
        }
    }

    #[test]
    fn test_post_hook_failure_continues() {
        let temp_dir = setup_test_env();
        let _guard = JanusRootGuard::new(temp_dir.path().join(".janus"));

        // Create a file to track that the hook ran
        let marker_file = temp_dir.path().join("hook_ran.txt");

        // Create a failing hook script that still writes the marker
        let hooks_dir = temp_dir.path().join(".janus/hooks");
        let script_path = hooks_dir.join("post-write.sh");
        let script_content = format!(
            "#!/bin/sh\necho 'hook ran' > \"{}\"\nexit 1\n",
            marker_file.display()
        );
        fs::write(&script_path, script_content).unwrap();
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();

        // Create config
        let config_content = r#"
hooks:
  enabled: true
  timeout: 0
  scripts:
    post_write: post-write.sh
"#;
        let config_path = temp_dir.path().join(".janus/config.yaml");
        fs::write(&config_path, config_content).unwrap();

        let context = HookContext::new()
            .with_event(HookEvent::PostWrite)
            .with_item_type(EntityType::Ticket)
            .with_item_id("j-test");

        // Post hooks don't return errors, they just log warnings
        run_post_hooks(HookEvent::PostWrite, &context);

        // Give it a moment to complete
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Verify the hook ran (marker file should exist)
        assert!(marker_file.exists(), "Hook should have run");
        let content = fs::read_to_string(&marker_file).unwrap();
        assert!(content.contains("hook ran"));
    }

    #[test]
    fn test_pre_hook_timeout_with_exit_code() {
        let temp_dir = setup_test_env();
        let _guard = JanusRootGuard::new(temp_dir.path().join(".janus"));

        // Create a hook script that sleeps
        let hooks_dir = temp_dir.path().join(".janus/hooks");
        let script_path = hooks_dir.join("slow-hook.sh");
        fs::write(&script_path, "#!/bin/sh\nsleep 10\nexit 0\n").unwrap();
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();

        // Create config with 1 second timeout
        let config_content = r#"
hooks:
  enabled: true
  timeout: 1
  scripts:
    pre_write: slow-hook.sh
"#;
        let config_path = temp_dir.path().join(".janus/config.yaml");
        fs::write(&config_path, config_content).unwrap();

        let context = HookContext::new()
            .with_event(HookEvent::PreWrite)
            .with_item_type(EntityType::Ticket);

        let result = run_pre_hooks(HookEvent::PreWrite, &context);

        // Verify the timeout error contains the expected information
        match result {
            Err(JanusError::HookTimeout { hook_name, seconds }) => {
                assert_eq!(hook_name, "slow-hook.sh");
                assert_eq!(seconds, 1);
            }
            other => panic!("Expected HookTimeout, got: {other:?}"),
        }
    }

    #[test]
    fn test_hook_script_not_found_error() {
        let temp_dir = setup_test_env();
        let _guard = JanusRootGuard::new(temp_dir.path().join(".janus"));

        // Create config pointing to non-existent script
        let config_content = r#"
hooks:
  enabled: true
  scripts:
    pre_write: does-not-exist.sh
"#;
        let config_path = temp_dir.path().join(".janus/config.yaml");
        fs::write(&config_path, config_content).unwrap();

        let context = HookContext::new()
            .with_event(HookEvent::PreWrite)
            .with_item_type(EntityType::Ticket);

        let result = run_pre_hooks(HookEvent::PreWrite, &context);

        // Verify the error contains the script path
        match result {
            Err(JanusError::HookScriptNotFound(path)) => {
                assert!(path.to_string_lossy().contains("does-not-exist.sh"));
            }
            other => panic!("Expected HookScriptNotFound, got: {other:?}"),
        }
    }

    #[test]
    fn test_pre_hook_skipped_for_post_events() {
        // Pre-hooks should not run for post events
        let context = HookContext::new()
            .with_event(HookEvent::PostWrite)
            .with_item_type(EntityType::Ticket);

        // This should succeed immediately without doing anything
        let result = run_pre_hooks(HookEvent::PostWrite, &context);
        assert!(result.is_ok());

        let result = run_pre_hooks(HookEvent::TicketCreated, &context);
        assert!(result.is_ok());
    }

    #[test]
    fn test_hook_script_with_path_separator() {
        let temp_dir = setup_test_env();
        let _guard = JanusRootGuard::new(temp_dir.path().join(".janus"));

        // Create config with script containing path separator
        let config_content = r#"
hooks:
  enabled: true
  scripts:
    pre_write: ../../../etc/passwd
"#;
        let config_path = temp_dir.path().join(".janus/config.yaml");
        fs::write(&config_path, config_content).unwrap();

        let context = HookContext::new()
            .with_event(HookEvent::PreWrite)
            .with_item_type(EntityType::Ticket);

        let result = run_pre_hooks(HookEvent::PreWrite, &context);

        // Should return a security error
        assert!(matches!(result, Err(JanusError::HookSecurity(_))));
    }

    #[test]
    fn test_hook_script_with_windows_path_separator() {
        let temp_dir = setup_test_env();
        let _guard = JanusRootGuard::new(temp_dir.path().join(".janus"));

        // Create config with script containing Windows path separator
        let config_content = "hooks:\n  enabled: true\n  scripts:\n    pre_write: \"..\\\\..\\\\windows\\\\system32\"\n";
        let config_path = temp_dir.path().join(".janus/config.yaml");
        fs::write(&config_path, config_content).unwrap();

        let context = HookContext::new()
            .with_event(HookEvent::PreWrite)
            .with_item_type(EntityType::Ticket);

        let result = run_pre_hooks(HookEvent::PreWrite, &context);

        // Should return a security error
        assert!(matches!(result, Err(JanusError::HookSecurity(_))));
    }

    #[test]
    fn test_hook_script_with_null_byte() {
        let temp_dir = setup_test_env();
        let _guard = JanusRootGuard::new(temp_dir.path().join(".janus"));

        // Create config with script containing null byte
        let config_content =
            "hooks:\n  enabled: true\n  scripts:\n    pre_write: \"foo\\x00bar\"\n";
        let config_path = temp_dir.path().join(".janus/config.yaml");
        fs::write(&config_path, config_content).unwrap();

        let context = HookContext::new()
            .with_event(HookEvent::PreWrite)
            .with_item_type(EntityType::Ticket);

        let result = run_pre_hooks(HookEvent::PreWrite, &context);

        // Should return a security error (or config parse error)
        match result {
            Err(JanusError::HookSecurity(_)) => {}
            Err(JanusError::YamlParse(_)) => {}
            other => panic!("Expected HookSecurity or YamlParse error, got: {other:?}"),
        }
    }

    #[test]
    fn test_post_hook_failure_logged() {
        let temp_dir = setup_test_env();
        let _guard = JanusRootGuard::new(temp_dir.path().join(".janus"));

        // Create a failing post-hook script
        let hooks_dir = temp_dir.path().join(".janus/hooks");
        let script_path = hooks_dir.join("post-write.sh");
        fs::write(&script_path, "#!/bin/sh\nexit 1\n").unwrap();
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();

        // Create config
        let config_content = r#"
hooks:
  enabled: true
  timeout: 0
  scripts:
    post_write: post-write.sh
"#;
        let config_path = temp_dir.path().join(".janus/config.yaml");
        fs::write(&config_path, config_content).unwrap();

        let context = HookContext::new()
            .with_event(HookEvent::PostWrite)
            .with_item_type(EntityType::Ticket)
            .with_item_id("j-test");

        // Run post hook (should fail but not return error)
        run_post_hooks(HookEvent::PostWrite, &context);

        // Give it a moment to complete
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Verify the failure was logged
        let log_path = temp_dir.path().join(".janus/hooks.log");
        assert!(log_path.exists(), "Hook log file should be created");

        let log_content = fs::read_to_string(&log_path).unwrap();
        assert!(
            log_content.contains("post-hook 'post-write.sh' failed"),
            "Log should contain failure message. Got: {log_content}"
        );
        // Verify timestamp format (ISO 8601)
        assert!(
            log_content.contains('T') && log_content.contains('Z'),
            "Log should contain ISO 8601 timestamp. Got: {log_content}"
        );
    }

    #[test]
    fn test_multiple_post_hook_failures_logged() {
        let temp_dir = setup_test_env();
        let _guard = JanusRootGuard::new(temp_dir.path().join(".janus"));

        // Create a failing post-hook script
        let hooks_dir = temp_dir.path().join(".janus/hooks");
        let script_path = hooks_dir.join("post-write.sh");
        fs::write(&script_path, "#!/bin/sh\nexit 1\n").unwrap();
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();

        // Create config
        let config_content = r#"
hooks:
  enabled: true
  timeout: 0
  scripts:
    post_write: post-write.sh
"#;
        let config_path = temp_dir.path().join(".janus/config.yaml");
        fs::write(&config_path, config_content).unwrap();

        let context = HookContext::new()
            .with_event(HookEvent::PostWrite)
            .with_item_type(EntityType::Ticket)
            .with_item_id("j-test");

        // Run post hook multiple times
        run_post_hooks(HookEvent::PostWrite, &context);
        std::thread::sleep(std::time::Duration::from_millis(50));
        run_post_hooks(HookEvent::PostWrite, &context);
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Verify both failures were logged
        let log_path = temp_dir.path().join(".janus/hooks.log");
        let log_content = fs::read_to_string(&log_path).unwrap();

        // Count occurrences of failure messages
        let failure_count = log_content
            .matches("post-hook 'post-write.sh' failed")
            .count();
        assert_eq!(failure_count, 2, "Log should contain two failure entries");
    }
}

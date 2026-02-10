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

pub mod types;

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::process::Command as TokioCommand;
use tokio::time::timeout;
use wait_timeout::ChildExt;

pub use types::{HookContext, HookEvent};

pub use crate::types::EntityType;

use crate::config::Config;
use crate::error::{JanusError, Result};
use crate::types::janus_root;
use crate::utils::iso_date;

/// The directory within .janus where hook scripts are stored.
const HOOKS_DIR: &str = "hooks";

/// The file within .janus where hook failures are logged.
const HOOK_LOG_FILE: &str = "hooks.log";

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

/// Validate a script name for security (path traversal prevention).
///
/// # Arguments
/// * `script_name` - The script name to validate
///
/// # Returns
/// * `Ok(())` if the script name is valid
/// * `Err(JanusError::HookSecurity)` if the script name contains path separators
fn validate_script_name(script_name: &str) -> Result<()> {
    if script_name.contains('/') || script_name.contains('\\') || script_name.contains('\0') {
        return Err(JanusError::HookSecurity("Invalid script name".to_string()));
    }
    Ok(())
}

/// Prepare hook execution by resolving paths and building environment.
///
/// This function performs all the shared preparation work:
/// - Script name validation
/// - Path resolution (with symlink canonicalization)
/// - Security checks (ensure script is within hooks directory)
/// - Environment variable construction
///
/// # Arguments
/// * `event` - The hook event being run
/// * `script_name` - The name of the script (relative to .janus/hooks/)
/// * `context` - The context to pass to the hook script
///
/// # Returns
/// A tuple of (script_path, environment_variables, janus_root)
fn prepare_hook_execution(
    event: HookEvent,
    script_name: &str,
    context: &HookContext,
) -> Result<(PathBuf, HashMap<String, String>, PathBuf)> {
    validate_script_name(script_name)?;

    let j_root = janus_root();
    let hooks_dir = j_root.join(HOOKS_DIR).canonicalize()?;
    let script_path = hooks_dir.join(script_name);

    if !script_path.exists() {
        return Err(JanusError::HookScriptNotFound(script_path));
    }

    // Canonicalize the script path to resolve any symlinks (especially important on macOS
    // where /var is a symlink to /private/var)
    let script_path = script_path.canonicalize()?;

    // Security check: ensure the canonicalized script path is still within the hooks directory
    if !script_path.starts_with(&hooks_dir) {
        return Err(JanusError::HookSecurity(format!(
            "Script path '{}' resolves outside hooks directory",
            crate::utils::format_relative_path(&script_path)
        )));
    }

    // Use the event parameter to override context.event for env vars
    let context_with_event = context.clone().with_event(event);
    let env_vars = context_to_env(&context_with_event, &j_root);

    Ok((script_path, env_vars, j_root))
}

/// Build an appropriate error for a failed hook.
///
/// # Arguments
/// * `script_name` - The name of the hook that failed
/// * `exit_code` - The exit code from the hook process
/// * `stderr` - The stderr output from the hook
/// * `is_pre_hook` - Whether this is a pre-hook (affects error type)
///
/// # Returns
/// A `JanusError` appropriate for the hook type
fn build_hook_error(
    script_name: &str,
    exit_code: i32,
    stderr: String,
    is_pre_hook: bool,
) -> JanusError {
    if is_pre_hook {
        JanusError::PreHookFailed {
            hook_name: script_name.to_string(),
            exit_code,
            message: stderr,
        }
    } else {
        JanusError::PostHookFailed {
            hook_name: script_name.to_string(),
            message: stderr,
        }
    }
}

/// Execute a hook script with the given context.
///
/// # Arguments
/// * `event` - The hook event being run
/// * `script_name` - The name of the script (relative to .janus/hooks/)
/// * `context` - The context to pass to the hook script
/// * `config` - The configuration containing hook settings
/// * `is_pre_hook` - Whether this is a pre-hook (affects error handling)
///
/// # Returns
/// * `Ok(())` if the hook succeeded
/// * `Err` if the hook failed and is_pre_hook is true
fn execute_hook(
    event: HookEvent,
    script_name: &str,
    context: &HookContext,
    config: &Config,
    is_pre_hook: bool,
) -> Result<()> {
    let (script_path, env_vars, j_root) = prepare_hook_execution(event, script_name, context)?;

    let mut cmd = std::process::Command::new(&script_path);
    cmd.envs(env_vars);
    cmd.current_dir(&j_root);

    let timeout_secs = config.hooks.timeout;

    if timeout_secs == 0 {
        let output = cmd.output()?;

        if !output.status.success() {
            let exit_code = output.status.code().unwrap_or(-1);
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            return Err(build_hook_error(
                script_name,
                exit_code,
                stderr,
                is_pre_hook,
            ));
        }
    } else {
        let mut child = cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        match child.wait_timeout(Duration::from_secs(timeout_secs))? {
            Some(status) => {
                let output = child.wait_with_output()?;

                if !status.success() {
                    let exit_code = status.code().unwrap_or(-1);
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    return Err(build_hook_error(
                        script_name,
                        exit_code,
                        stderr,
                        is_pre_hook,
                    ));
                }
            }
            None => {
                if let Err(e) = child.kill() {
                    eprintln!("Warning: failed to kill timed-out hook '{script_name}': {e}");
                }
                match child.wait_timeout(Duration::from_secs(5)) {
                    Ok(Some(_)) => {}
                    Ok(None) => {
                        eprintln!("Warning: hook '{script_name}' did not terminate after SIGKILL")
                    }
                    Err(e) => {
                        eprintln!("Warning: error waiting for hook '{script_name}' cleanup: {e}")
                    }
                }

                return Err(JanusError::HookTimeout {
                    hook_name: script_name.to_string(),
                    seconds: timeout_secs,
                });
            }
        }
    }

    Ok(())
}

/// Execute a hook script with the given context (async version).
///
/// # Arguments
/// * `event` - The hook event being run
/// * `script_name` - The name of the script (relative to .janus/hooks/)
/// * `context` - The context to pass to the hook script
/// * `config` - The configuration containing hook settings
/// * `is_pre_hook` - Whether this is a pre-hook (affects error handling)
///
/// # Returns
/// * `Ok(())` if the hook succeeded
/// * `Err` if the hook failed and is_pre_hook is true
async fn execute_hook_async(
    event: HookEvent,
    script_name: &str,
    context: &HookContext,
    config: &Config,
    is_pre_hook: bool,
) -> Result<()> {
    let (script_path, env_vars, j_root) = prepare_hook_execution(event, script_name, context)?;

    let mut cmd = TokioCommand::new(&script_path);
    cmd.envs(env_vars);
    cmd.current_dir(&j_root);

    let timeout_secs = config.hooks.timeout;

    if timeout_secs == 0 {
        let output = cmd.output().await?;

        if !output.status.success() {
            let exit_code = output.status.code().unwrap_or(-1);
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            return Err(build_hook_error(
                script_name,
                exit_code,
                stderr,
                is_pre_hook,
            ));
        }
    } else {
        let mut child = cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        match timeout(Duration::from_secs(timeout_secs), child.wait()).await {
            Ok(Ok(status)) => {
                let output = child.wait_with_output().await?;

                if !status.success() {
                    let exit_code = status.code().unwrap_or(-1);
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    return Err(build_hook_error(
                        script_name,
                        exit_code,
                        stderr,
                        is_pre_hook,
                    ));
                }
            }
            Ok(Err(e)) => {
                return Err(JanusError::Io(e));
            }
            Err(_) => {
                if let Err(e) = child.kill().await {
                    eprintln!("Warning: failed to kill timed-out hook '{script_name}': {e}");
                }
                // Give it a moment to clean up
                match timeout(Duration::from_secs(5), child.wait()).await {
                    Ok(_) => {}
                    Err(_) => {
                        eprintln!("Warning: hook '{script_name}' did not terminate after SIGKILL")
                    }
                }

                return Err(JanusError::HookTimeout {
                    hook_name: script_name.to_string(),
                    seconds: timeout_secs,
                });
            }
        }
    }

    Ok(())
}

/// Convert a HookContext to environment variables for the hook script.
///
/// # Arguments
/// * `context` - The hook context
/// * `janus_root` - Path to the .janus directory
///
/// # Returns
/// A HashMap of environment variable names to values
pub fn context_to_env(context: &HookContext, janus_root: &Path) -> HashMap<String, String> {
    let mut env = HashMap::new();

    if let Some(event) = &context.event {
        env.insert("JANUS_EVENT".to_string(), event.to_string());
    }

    if let Some(item_type) = &context.item_type {
        env.insert("JANUS_ITEM_TYPE".to_string(), item_type.to_string());
    }

    if let Some(item_id) = &context.item_id {
        env.insert("JANUS_ITEM_ID".to_string(), item_id.clone());
    }

    if let Some(file_path) = &context.file_path {
        env.insert(
            "JANUS_FILE_PATH".to_string(),
            crate::utils::format_relative_path(file_path),
        );
    }

    if let Some(field_name) = &context.field_name {
        env.insert("JANUS_FIELD_NAME".to_string(), field_name.clone());
    }

    if let Some(old_value) = &context.old_value {
        env.insert("JANUS_OLD_VALUE".to_string(), old_value.clone());
    }

    if let Some(new_value) = &context.new_value {
        env.insert("JANUS_NEW_VALUE".to_string(), new_value.clone());
    }

    env.insert("JANUS_ROOT".to_string(), janus_root.display().to_string());

    env
}

/// Log a hook failure to the hooks.log file.
///
/// Appends a timestamped entry to `.janus/hooks.log` with information about
/// the hook failure. If writing to the log fails, a warning is printed to stderr
/// but the error is not propagated since this is a non-critical operation.
///
/// # Arguments
/// * `hook_name` - The name of the hook that failed
/// * `error` - The error that occurred
fn log_hook_failure(hook_name: &str, error: &JanusError) {
    let log_path = janus_root().join(HOOK_LOG_FILE);
    let timestamp = iso_date();

    // Extract the stderr message from PostHookFailed error, or use the full error
    let error_detail = match error {
        JanusError::PostHookFailed { message, .. } => {
            if message.is_empty() {
                "exited with non-zero status".to_string()
            } else {
                message.clone()
            }
        }
        _ => error.to_string(),
    };

    let log_entry = format!("{timestamp}: post-hook '{hook_name}' failed: {error_detail}\n");

    // Try to append to the log file, but don't fail if we can't
    match OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .and_then(|mut file| file.write_all(log_entry.as_bytes()))
    {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Warning: failed to write to hook log file: {e}");
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    use serial_test::serial;
    use tempfile::TempDir;

    use crate::test_guards::CwdGuard;

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
    #[serial]
    fn test_run_pre_hooks_no_config() {
        // When there's no config file, hooks should succeed silently
        let temp_dir = setup_test_env();
        let _cwd_guard = CwdGuard::new().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let context = HookContext::new()
            .with_event(HookEvent::PreWrite)
            .with_item_type(EntityType::Ticket)
            .with_item_id("j-1234");

        // Should succeed even without config
        let result = run_pre_hooks(HookEvent::PreWrite, &context);
        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    fn test_run_post_hooks_no_config() {
        let temp_dir = setup_test_env();
        let _cwd_guard = CwdGuard::new().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let context = HookContext::new()
            .with_event(HookEvent::PostWrite)
            .with_item_type(EntityType::Ticket)
            .with_item_id("j-1234");

        // Should succeed even without config
        run_post_hooks(HookEvent::PostWrite, &context);
        // No assertion needed - post hooks don't return errors
    }

    #[test]
    #[serial]
    fn test_hooks_disabled() {
        let temp_dir = setup_test_env();
        let _cwd_guard = CwdGuard::new().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

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
    #[serial]
    fn test_hook_script_not_found() {
        let temp_dir = setup_test_env();
        let _cwd_guard = CwdGuard::new().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

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
    #[serial]
    fn test_pre_hook_success() {
        let temp_dir = setup_test_env();
        let _cwd_guard = CwdGuard::new().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

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
    #[serial]
    fn test_pre_hook_failure() {
        let temp_dir = setup_test_env();
        let _cwd_guard = CwdGuard::new().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

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
    #[serial]
    fn test_post_hook_receives_env_vars() {
        let temp_dir = setup_test_env();
        let _cwd_guard = CwdGuard::new().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

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
    #[serial]
    fn test_hook_timeout() {
        let temp_dir = setup_test_env();
        let _cwd_guard = CwdGuard::new().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

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
    #[serial]
    fn test_pre_hook_failure_aborts_operation() {
        let temp_dir = setup_test_env();
        let _cwd_guard = CwdGuard::new().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

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
    #[serial]
    fn test_post_hook_failure_continues() {
        let temp_dir = setup_test_env();
        let _cwd_guard = CwdGuard::new().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

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
    #[serial]
    fn test_pre_hook_timeout_with_exit_code() {
        let temp_dir = setup_test_env();
        let _cwd_guard = CwdGuard::new().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

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
    #[serial]
    fn test_hook_script_not_found_error() {
        let temp_dir = setup_test_env();
        let _cwd_guard = CwdGuard::new().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

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
    #[serial]
    fn test_hook_script_with_path_separator() {
        let temp_dir = setup_test_env();
        let _cwd_guard = CwdGuard::new().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

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
    #[serial]
    fn test_hook_script_with_windows_path_separator() {
        let temp_dir = setup_test_env();
        let _cwd_guard = CwdGuard::new().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

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
    #[serial]
    fn test_hook_script_with_null_byte() {
        let temp_dir = setup_test_env();
        let _cwd_guard = CwdGuard::new().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

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
    #[serial]
    fn test_post_hook_failure_logged() {
        let temp_dir = setup_test_env();
        let _cwd_guard = CwdGuard::new().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

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
    #[serial]
    fn test_multiple_post_hook_failures_logged() {
        let temp_dir = setup_test_env();
        let _cwd_guard = CwdGuard::new().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

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

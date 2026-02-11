//! Hook execution engine.
//!
//! This module handles the low-level details of running hook scripts:
//! process spawning, timeout handling, output capture, environment variable
//! construction, and failure logging.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::process::Command as TokioCommand;
use tokio::time::timeout;
use wait_timeout::ChildExt;

use super::types::{HookContext, HookEvent};
use crate::config::Config;
use crate::error::{JanusError, Result};
use crate::types::janus_root;
use crate::utils::iso_date;

/// The directory within .janus where hook scripts are stored.
const HOOKS_DIR: &str = "hooks";

/// The file within .janus where hook failures are logged.
const HOOK_LOG_FILE: &str = "hooks.log";

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

/// Check command output and return error if the command failed.
///
/// Shared helper for both sync and async no-timeout paths.
fn check_output(output: &std::process::Output, script_name: &str, is_pre_hook: bool) -> Result<()> {
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
    Ok(())
}

/// Check exit status and return error if the process failed.
///
/// Shared helper for both sync and async timeout paths.
fn check_status(
    status: &std::process::ExitStatus,
    stderr: &str,
    script_name: &str,
    is_pre_hook: bool,
) -> Result<()> {
    if !status.success() {
        let exit_code = status.code().unwrap_or(-1);
        return Err(build_hook_error(
            script_name,
            exit_code,
            stderr.to_string(),
            is_pre_hook,
        ));
    }
    Ok(())
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
pub(super) fn execute_hook(
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
        check_output(&output, script_name, is_pre_hook)?;
    } else {
        let mut child = cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        match child.wait_timeout(Duration::from_secs(timeout_secs))? {
            Some(status) => {
                let output = child.wait_with_output()?;
                check_status(
                    &status,
                    &String::from_utf8_lossy(&output.stderr),
                    script_name,
                    is_pre_hook,
                )?;
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
pub(super) async fn execute_hook_async(
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
        check_output(&output, script_name, is_pre_hook)?;
    } else {
        let mut child = cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        match timeout(Duration::from_secs(timeout_secs), child.wait()).await {
            Ok(Ok(status)) => {
                let output = child.wait_with_output().await?;
                check_status(
                    &status,
                    &String::from_utf8_lossy(&output.stderr),
                    script_name,
                    is_pre_hook,
                )?;
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
pub(super) fn log_hook_failure(hook_name: &str, error: &JanusError) {
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

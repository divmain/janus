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

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

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

/// Internal hook execution with timeout and output capture (async version).
///
/// This is the core execution function used by all public async variants.
/// It handles spawning, timeout enforcement, cleanup on timeout,
/// and returns the full output for the caller to interpret.
///
/// # Arguments
/// * `script_path` - Path to the hook script
/// * `env_vars` - Environment variables to set
/// * `j_root` - Janus root directory (working directory)
/// * `script_name` - Name of the script (for error messages)
/// * `timeout_secs` - Timeout in seconds (0 for no timeout)
///
/// # Returns
/// * `Ok((ExitStatus, Vec<u8>, Vec<u8>))` - (status, stdout, stderr)
/// * `Err` - If the hook times out or an IO error occurs
async fn run_hook_with_timeout_and_capture_async(
    script_path: &Path,
    env_vars: &HashMap<String, String>,
    j_root: &Path,
    script_name: &str,
    timeout_secs: u64,
) -> Result<(std::process::ExitStatus, Vec<u8>, Vec<u8>)> {
    let mut cmd = TokioCommand::new(script_path);
    cmd.envs(env_vars);
    cmd.current_dir(j_root);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    if timeout_secs == 0 {
        let output = cmd.output().await?;
        Ok((output.status, output.stdout, output.stderr))
    } else {
        let mut child = cmd.spawn()?;

        match timeout(Duration::from_secs(timeout_secs), child.wait()).await {
            Ok(Ok(status)) => {
                let output = child.wait_with_output().await?;
                Ok((status, output.stdout, output.stderr))
            }
            Ok(Err(e)) => Err(JanusError::Io(e)),
            Err(_) => {
                // Timeout occurred - attempt to kill the child process
                if let Err(e) = child.kill().await {
                    eprintln!("ERROR: failed to kill timed-out hook '{script_name}': {e}");
                    // Note: If SIGKILL fails, the process may become a zombie.
                    // Manual cleanup (e.g., kill -9 <pid>) may be required in edge cases.
                }
                // Wait up to 5 seconds for the process to terminate after SIGKILL
                match timeout(Duration::from_secs(5), child.wait()).await {
                    Ok(_) => {}
                    Err(_) => {
                        eprintln!(
                            "ERROR: hook '{script_name}' did not terminate after SIGKILL; manual cleanup may be needed"
                        );
                    }
                }

                Err(JanusError::HookTimeout {
                    hook_name: script_name.to_string(),
                    seconds: timeout_secs,
                })
            }
        }
    }
}

/// Internal hook execution with timeout and output capture (sync version).
///
/// This is the core execution function used by the sync public variant.
/// It handles spawning, timeout enforcement, cleanup on timeout,
/// and returns the full output for the caller to interpret.
///
/// # Arguments
/// * `script_path` - Path to the hook script
/// * `env_vars` - Environment variables to set
/// * `j_root` - Janus root directory (working directory)
/// * `script_name` - Name of the script (for error messages)
/// * `timeout_secs` - Timeout in seconds (0 for no timeout)
///
/// # Returns
/// * `Ok((ExitStatus, Vec<u8>, Vec<u8>))` - (status, stdout, stderr)
/// * `Err` - If the hook times out or an IO error occurs
fn run_hook_with_timeout_and_capture(
    script_path: &Path,
    env_vars: &HashMap<String, String>,
    j_root: &Path,
    script_name: &str,
    timeout_secs: u64,
) -> Result<(std::process::ExitStatus, Vec<u8>, Vec<u8>)> {
    let mut cmd = std::process::Command::new(script_path);
    cmd.envs(env_vars);
    cmd.current_dir(j_root);

    if timeout_secs == 0 {
        let output = cmd.output()?;
        Ok((output.status, output.stdout, output.stderr))
    } else {
        let mut child = cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        match child.wait_timeout(Duration::from_secs(timeout_secs))? {
            Some(status) => {
                let output = child.wait_with_output()?;
                Ok((status, output.stdout, output.stderr))
            }
            None => {
                // Timeout occurred - attempt to kill the child process
                if let Err(e) = child.kill() {
                    eprintln!("ERROR: failed to kill timed-out hook '{script_name}': {e}");
                    // Note: If SIGKILL fails, the process may become a zombie.
                    // Manual cleanup (e.g., kill -9 <pid>) may be required in edge cases.
                }
                // Wait up to 5 seconds for the process to terminate after SIGKILL
                match child.wait_timeout(Duration::from_secs(5)) {
                    Ok(Some(_)) => {}
                    Ok(None) => {
                        eprintln!(
                            "ERROR: hook '{script_name}' did not terminate after SIGKILL; manual cleanup may be needed"
                        );
                    }
                    Err(e) => {
                        eprintln!("ERROR: error waiting for hook '{script_name}' cleanup: {e}");
                    }
                }

                Err(JanusError::HookTimeout {
                    hook_name: script_name.to_string(),
                    seconds: timeout_secs,
                })
            }
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
pub(super) fn execute_hook(
    event: HookEvent,
    script_name: &str,
    context: &HookContext,
    config: &Config,
    is_pre_hook: bool,
) -> Result<()> {
    let (script_path, env_vars, j_root) = prepare_hook_execution(event, script_name, context)?;

    let timeout_secs = config.hooks.timeout;
    let (status, _, stderr) = run_hook_with_timeout_and_capture(
        &script_path,
        &env_vars,
        &j_root,
        script_name,
        timeout_secs,
    )?;

    check_status(
        &status,
        &String::from_utf8_lossy(&stderr),
        script_name,
        is_pre_hook,
    )
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

    let timeout_secs = config.hooks.timeout;
    let (status, _, stderr) = run_hook_with_timeout_and_capture_async(
        &script_path,
        &env_vars,
        &j_root,
        script_name,
        timeout_secs,
    )
    .await?;

    check_status(
        &status,
        &String::from_utf8_lossy(&stderr),
        script_name,
        is_pre_hook,
    )
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
    #[cfg(unix)]
    let result = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(&log_path)
        .and_then(|mut file| file.write_all(log_entry.as_bytes()));

    #[cfg(not(unix))]
    let result = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .and_then(|mut file| file.write_all(log_entry.as_bytes()));

    if let Err(e) = result {
        eprintln!("Warning: failed to write to hook log file: {e}");
    }
}

/// Result of executing a hook script.
#[derive(Debug)]
pub struct HookExecutionResult {
    /// Whether the hook succeeded
    pub success: bool,
    /// Exit code (None if process was killed or didn't exit normally)
    pub exit_code: Option<i32>,
    /// Standard output from the hook
    pub stdout: String,
    /// Standard error from the hook
    pub stderr: String,
    /// Environment variables that were set
    pub env_vars: HashMap<String, String>,
}

/// Execute a hook script and return detailed results.
///
/// This is the primary hook execution API used by both the internal hook system
/// and the CLI "hook run" command. It provides complete execution logic including:
/// - Script name validation (path traversal prevention)
/// - Path resolution and security checks
/// - Environment variable construction
/// - Timeout enforcement
/// - Output capture
///
/// # Arguments
/// * `event` - The hook event being run
/// * `script_name` - The name of the script (relative to .janus/hooks/)
/// * `context` - The context to pass to the hook script
/// * `timeout_secs` - Timeout in seconds (0 for no timeout)
///
/// # Returns
/// * `Ok(HookExecutionResult)` with execution details
/// * `Err` if the script is not found or security checks fail
pub async fn execute_hook_with_result(
    event: HookEvent,
    script_name: &str,
    context: &HookContext,
    timeout_secs: u64,
) -> Result<HookExecutionResult> {
    let (script_path, env_vars, j_root) = prepare_hook_execution(event, script_name, context)?;

    let (status, stdout, stderr) = run_hook_with_timeout_and_capture_async(
        &script_path,
        &env_vars,
        &j_root,
        script_name,
        timeout_secs,
    )
    .await?;

    Ok(HookExecutionResult {
        success: status.success(),
        exit_code: status.code(),
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        env_vars,
    })
}

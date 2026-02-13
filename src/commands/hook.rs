//! Commands for managing hooks.
//!
//! - `list`: Show configured hooks
//! - `install`: Install a hook recipe from GitHub
//! - `run`: Run a hook manually for testing
//! - `enable`: Enable hooks
//! - `disable`: Disable hooks
//! - `log`: View hook failure log

use std::collections::HashMap;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use owo_colors::OwoColorize;
use serde::Deserialize;
use serde_json::json;

use super::{CommandOutput, interactive};
use crate::cli::OutputOptions;
use crate::config::Config;
use crate::error::{JanusError, Result};
use crate::hooks::types::HookEvent;
use crate::hooks::{HookContext, execute_hook_with_result};
use crate::ticket::Ticket;
use crate::types::EntityType;
use crate::types::janus_root;
use crate::utils::is_stdin_tty;

/// Base URL for GitHub API
const GITHUB_API_BASE: &str = "https://api.github.com/repos/divmain/janus/contents/hook_recipes";

/// Response from GitHub API for repository contents
#[derive(Debug, Deserialize)]
struct GitHubContent {
    name: String,
    path: String,
    #[serde(rename = "type")]
    content_type: String,
    /// Base64-encoded content (only for files) - not used but present in API response
    #[allow(dead_code)]
    content: Option<String>,
    /// Download URL for the file
    download_url: Option<String>,
}

/// Recipe configuration file structure
#[derive(Debug, Deserialize)]
struct RecipeConfig {
    hooks: Option<RecipeHooksConfig>,
}

#[derive(Debug, Deserialize)]
struct RecipeHooksConfig {
    scripts: Option<HashMap<String, String>>,
}

/// List configured hooks
pub fn cmd_hook_list(output: OutputOptions) -> Result<()> {
    let config = Config::load()?;

    // Build JSON output
    let mut scripts_map = serde_json::Map::new();
    for (event, script) in &config.hooks.scripts {
        scripts_map.insert(event.clone(), json!(script));
    }

    let json_output = json!({
        "enabled": config.hooks.enabled,
        "timeout": config.hooks.timeout,
        "scripts": scripts_map,
    });

    // Build text output
    let mut text_output = String::new();

    let status = if config.hooks.enabled {
        "enabled".green().to_string()
    } else {
        "disabled".red().to_string()
    };
    text_output.push_str(&format!("Hooks: {status}\n"));
    text_output.push_str(&format!("Timeout: {}s\n", config.hooks.timeout));
    text_output.push('\n');

    if config.hooks.scripts.is_empty() {
        text_output.push_str("No hooks configured.\n");
        text_output.push('\n');
        text_output.push_str(&format!(
            "To add hooks, edit {} or run:\n",
            ".janus/config.yaml".cyan()
        ));
        text_output.push_str("  janus hook install <recipe>\n");
    } else {
        text_output.push_str("Configured hooks:\n");
        let mut events: Vec<_> = config.hooks.scripts.iter().collect();
        events.sort_by_key(|(k, _)| *k);
        for (event, script) in events {
            text_output.push_str(&format!("  {} → {}\n", event.cyan(), script));
        }
    }

    CommandOutput::new(json_output)
        .with_text(text_output)
        .print(output)
}

/// Phase 1: Fetch hook scripts from GitHub
///
/// Downloads the recipe configuration and all files from the recipe's
/// files directory via the GitHub API.
async fn fetch_hook_scripts(
    recipe: &str,
    client: &reqwest::Client,
) -> Result<(RecipeConfig, Vec<(String, String)>)> {
    let recipe_url = format!("{GITHUB_API_BASE}/{recipe}");
    let response = client
        .get(&recipe_url)
        .header("User-Agent", "janus-cli")
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(JanusError::HookRecipeNotFound(recipe.to_string()));
    }

    if !response.status().is_success() {
        return Err(JanusError::HookFetchFailed(format!(
            "GitHub API error: {} - {}",
            response.status(),
            response.text().await.unwrap_or_default()
        )));
    }

    let contents: Vec<GitHubContent> = response.json().await?;

    // Find config.yaml and files directory
    let mut config_content: Option<String> = None;
    let mut files_to_install: Vec<(String, String)> = Vec::new();

    for item in &contents {
        if item.name == "config.yaml" && item.content_type == "file" {
            if let Some(download_url) = &item.download_url {
                let content = client
                    .get(download_url)
                    .header("User-Agent", "janus-cli")
                    .send()
                    .await?
                    .text()
                    .await?;
                config_content = Some(content);
            }
        } else if item.name == "files" && item.content_type == "dir" {
            files_to_install = fetch_files_recursive(client, &item.path).await?;
        }
    }

    let recipe_config: RecipeConfig = if let Some(ref content) = config_content {
        serde_yaml_ng::from_str(content)?
    } else {
        return Err(JanusError::HookFetchFailed(format!(
            "recipe '{recipe}' is missing config.yaml"
        )));
    };

    Ok((recipe_config, files_to_install))
}

/// Files to be written: (target_path, content, is_executable)
type FilesToWrite = Vec<(PathBuf, String, bool)>;

/// Phase 2: Resolve file conflicts interactively or via force flag
///
/// Determines which files should be written vs skipped based on existing
/// files and user preferences (interactive mode, JSON mode, force flag).
fn resolve_conflicts(
    files_to_install: &[(String, String)],
    force: bool,
    output: OutputOptions,
) -> Result<(FilesToWrite, Vec<String>)> {
    let janus_dir = janus_root();
    let mut files_to_write: Vec<(PathBuf, String, bool)> = Vec::new();
    let mut files_skipped: Vec<String> = Vec::new();

    for (relative_path, content) in files_to_install {
        let target_path = janus_dir.join(relative_path);
        let is_hook_script = relative_path.starts_with("hooks/");

        if target_path.exists() {
            if output.json && !force {
                // In JSON mode without force, skip existing files
                files_skipped.push(relative_path.clone());
            } else if output.json && force {
                // In JSON mode with force, overwrite
                files_to_write.push((target_path, content.clone(), is_hook_script));
            } else {
                // Interactive mode - prompt user
                let choices = [("r", "Replace"), ("a", "Abort"), ("s", "Skip")];
                let idx = interactive::prompt_choice(
                    &format!("File {} already exists", relative_path.yellow()),
                    &choices,
                    Some("s"),
                )?;

                match idx {
                    0 => {
                        files_to_write.push((target_path, content.clone(), is_hook_script));
                    }
                    1 => {
                        println!("Installation aborted.");
                        return Ok((Vec::new(), Vec::new()));
                    }
                    _ => {
                        println!("  Skipping {relative_path}");
                        files_skipped.push(relative_path.clone());
                    }
                }
            }
        } else {
            files_to_write.push((target_path, content.clone(), is_hook_script));
        }
    }

    Ok((files_to_write, files_skipped))
}

/// Phase 3: Write hook files to disk with proper permissions
///
/// Creates parent directories, writes file content, and sets executable
/// permissions on hook scripts.
fn write_hook_files(
    files_to_write: &[(PathBuf, String, bool)],
    output: OutputOptions,
) -> Result<Vec<String>> {
    let mut installed_files: Vec<String> = Vec::new();

    for (path, content, is_executable) in files_to_write {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                JanusError::Io(std::io::Error::new(
                    e.kind(),
                    format!(
                        "Failed to create directory for hook at {}: {}",
                        crate::utils::format_relative_path(parent),
                        e
                    ),
                ))
            })?;
        }
        fs::write(path, content).map_err(|e| {
            JanusError::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to write hook file at {}: {}",
                    crate::utils::format_relative_path(path),
                    e
                ),
            ))
        })?;

        // Set executable bit on hook scripts
        #[cfg(unix)]
        if *is_executable {
            let mut perms = fs::metadata(path)
                .map_err(|e| {
                    JanusError::Io(std::io::Error::new(
                        e.kind(),
                        format!(
                            "Failed to get metadata for hook at {}: {}",
                            crate::utils::format_relative_path(path),
                            e
                        ),
                    ))
                })?
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(path, perms).map_err(|e| {
                JanusError::Io(std::io::Error::new(
                    e.kind(),
                    format!(
                        "Failed to set permissions for hook at {}: {}",
                        crate::utils::format_relative_path(path),
                        e
                    ),
                ))
            })?;
        }

        if !output.json {
            println!(
                "  Installed {}",
                crate::utils::format_relative_path(path).green()
            );
        }

        installed_files.push(
            path.file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default(),
        );
    }

    Ok(installed_files)
}

/// Phase 4: Update hook configuration
///
/// Merges recipe hooks configuration into the main Janus config file.
fn update_hook_config(recipe_config: &RecipeConfig, output: OutputOptions) -> Result<bool> {
    let mut config_updated = false;

    if let Some(hooks_config) = &recipe_config.hooks {
        if let Some(scripts) = &hooks_config.scripts {
            let mut config = Config::load()?;
            for (event, script) in scripts {
                config.hooks.scripts.insert(event.clone(), script.clone());
            }
            config.save()?;
            config_updated = true;
            if !output.json {
                println!("  Updated {}", "config.yaml".green());
            }
        }
    }

    Ok(config_updated)
}

/// Install a hook recipe from GitHub
///
/// Orchestrates the four phases of hook installation:
/// 1. Fetch hook scripts from GitHub
/// 2. Resolve file conflicts
/// 3. Write hook files
/// 4. Update hook configuration
pub async fn cmd_hook_install(recipe: &str, force: bool, output: OutputOptions) -> Result<()> {
    if !output.json {
        println!("Fetching recipe '{}'...", recipe.cyan());
    }

    let client = reqwest::Client::new();

    // Phase 1: Fetch hook scripts
    let (recipe_config, files_to_install) = fetch_hook_scripts(recipe, &client).await?;

    // Security warning for interactive mode before installing remote scripts
    if !output.json && !force && is_stdin_tty() {
        let confirmed = interactive::confirm(&format!(
            "You are about to install and execute scripts from {}. Continue",
            "github.com/divmain/janus".cyan()
        ))?;
        if !confirmed {
            println!("Installation aborted.");
            return Ok(());
        }
        println!();
    }

    // Phase 2: Resolve conflicts
    let (files_to_write, files_skipped) = resolve_conflicts(&files_to_install, force, output)?;

    // Early exit if user aborted during conflict resolution
    if files_to_write.is_empty() && files_skipped.is_empty() {
        return Ok(());
    }

    // Phase 3: Write hook files
    let installed_files = write_hook_files(&files_to_write, output)?;

    // Phase 4: Update hook configuration
    let config_updated = update_hook_config(&recipe_config, output)?;

    // Output results
    if output.json {
        CommandOutput::new(json!({
            "action": "hook_install",
            "recipe": recipe,
            "success": true,
            "installed_files": installed_files,
            "skipped_files": files_skipped,
            "config_updated": config_updated,
        }))
        .with_text(format!("Recipe '{recipe}' installed successfully"))
        .print(output)?;
    } else {
        println!();
        println!(
            "{} Recipe '{}' installed successfully!",
            "✓".green(),
            recipe
        );
    }

    Ok(())
}

/// Type alias for the recursive future
type RecursiveFetchFuture<'a> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<(String, String)>>> + Send + 'a>>;

/// Recursively fetch all files from a GitHub directory
fn fetch_files_recursive<'a>(
    client: &'a reqwest::Client,
    path: &'a str,
) -> RecursiveFetchFuture<'a> {
    Box::pin(async move {
        let url = format!("https://api.github.com/repos/divmain/janus/contents/{path}");
        let response = client
            .get(&url)
            .header("User-Agent", "janus-cli")
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(JanusError::HookFetchFailed(format!(
                "failed to fetch {}: {}",
                path,
                response.status()
            )));
        }

        let contents: Vec<GitHubContent> = response.json().await?;
        let mut files: Vec<(String, String)> = Vec::new();

        // The path in GitHub API looks like "hook_recipes/git-sync/files/..."
        // We want to strip "hook_recipes/<recipe>/files/" to get the relative path
        let files_prefix = path.find("/files").map(|i| &path[..i + 6]).unwrap_or(path);

        for item in contents {
            if item.content_type == "file" {
                if let Some(download_url) = &item.download_url {
                    let content = client
                        .get(download_url)
                        .header("User-Agent", "janus-cli")
                        .send()
                        .await?
                        .text()
                        .await?;

                    // Get relative path within .janus/ (strip "hook_recipes/<recipe>/files/")
                    let relative_path = item
                        .path
                        .strip_prefix(files_prefix)
                        .unwrap_or(&item.path)
                        .trim_start_matches('/');
                    files.push((relative_path.to_string(), content));
                }
            } else if item.content_type == "dir" {
                // Recursively fetch subdirectory
                let mut subfiles = fetch_files_recursive(client, &item.path).await?;
                files.append(&mut subfiles);
            }
        }

        Ok(files)
    })
}

/// Run a hook manually for testing.
///
/// This is a thin CLI wrapper around the shared hook runner. All execution
/// logic (validation, environment, timeout, output handling) is delegated to
/// the runner module; this function only handles UX concerns like formatting
/// output and displaying environment variables.
pub async fn cmd_hook_run(event: &str, id: Option<&str>) -> Result<()> {
    let hook_event: HookEvent = event.parse()?;

    let config = Config::load()?;

    // Get the script for this event
    let script_name = config
        .hooks
        .get_script(hook_event.as_str())
        .ok_or_else(|| {
            JanusError::Config(format!(
                "No hook configured for event '{event}'. Configure it in .janus/config.yaml"
            ))
        })?;

    // Build context
    let mut context = HookContext::new().with_event(hook_event);

    // If an ID is provided, try to find the item and add context
    if let Some(item_id) = id {
        // Try to find as ticket first
        if let Ok(ticket) = Ticket::find(item_id).await {
            context = context
                .with_item_type(EntityType::Ticket)
                .with_item_id(&ticket.id)
                .with_file_path(&ticket.file_path);
        } else if let Ok(plan) = crate::plan::Plan::find(item_id).await {
            context = context
                .with_item_type(EntityType::Plan)
                .with_item_id(&plan.id)
                .with_file_path(&plan.file_path);
        } else {
            return Err(JanusError::ItemNotFound(item_id.to_string()));
        }
    }

    println!("Running hook: {} → {}", event.cyan(), script_name);
    println!();

    // Execute the hook using the shared runner (with timeout enforcement)
    let result =
        execute_hook_with_result(hook_event, script_name, &context, config.hooks.timeout).await?;

    println!("Environment variables:");
    let mut sorted_vars: Vec<_> = result.env_vars.iter().collect();
    sorted_vars.sort_by_key(|(k, _)| *k);
    for (key, value) in sorted_vars {
        println!("  {}={}", key.dimmed(), value);
    }
    println!();

    // Print output
    if !result.stdout.is_empty() {
        println!("stdout:");
        println!("{}", result.stdout);
    }

    if !result.stderr.is_empty() {
        println!("stderr:");
        println!("{}", result.stderr.red());
    }

    if result.success {
        println!("{} Hook completed successfully", "✓".green());
    } else {
        let exit_code = result.exit_code.unwrap_or(-1);
        println!(
            "{} Hook failed with exit code {}",
            "✗".red(),
            exit_code.to_string().red()
        );
    }

    Ok(())
}

/// Enable hooks
pub fn cmd_hook_enable(output: OutputOptions) -> Result<()> {
    let mut config = Config::load()?;

    if config.hooks.enabled {
        CommandOutput::new(json!({
            "action": "no_change",
            "hooks_enabled": true,
        }))
        .with_text(format!("{} Hooks already enabled", "ℹ".cyan()))
        .print(output)
    } else {
        config.hooks.enabled = true;
        config.save()?;

        CommandOutput::new(json!({
            "action": "enabled",
            "hooks_enabled": true,
        }))
        .with_text(format!("{} Hooks enabled", "✓".green()))
        .print(output)
    }
}

/// Disable hooks
pub fn cmd_hook_disable(output: OutputOptions) -> Result<()> {
    let mut config = Config::load()?;

    if !config.hooks.enabled {
        CommandOutput::new(json!({
            "action": "no_change",
            "hooks_enabled": false,
        }))
        .with_text(format!("{} Hooks already disabled", "ℹ".cyan()))
        .print(output)
    } else {
        config.hooks.enabled = false;
        config.save()?;

        CommandOutput::new(json!({
            "action": "disabled",
            "hooks_enabled": false,
        }))
        .with_text(format!("{} Hooks disabled", "✓".red()))
        .print(output)
    }
}

/// Display hook failure log
pub fn cmd_hook_log(lines: Option<usize>, output: OutputOptions) -> Result<()> {
    let log_path = janus_root().join("hooks.log");

    if !log_path.exists() {
        let json_output = json!({
            "entries": [],
            "message": "No hook failures logged"
        });
        let text_output = format!(
            "No hook failures logged.\n\nThe hook failure log will appear here after a post-hook fails.\nLog file: {}",
            crate::utils::format_relative_path(&log_path)
        );
        return CommandOutput::new(json_output)
            .with_text(text_output)
            .print(output);
    }

    let content = fs::read_to_string(&log_path).map_err(|e| {
        JanusError::Io(std::io::Error::new(
            e.kind(),
            format!(
                "Failed to read hook log at {}: {}",
                crate::utils::format_relative_path(&log_path),
                e
            ),
        ))
    })?;
    let mut log_lines: Vec<&str> = content.lines().collect();

    // If lines is specified, take only the last N lines
    if let Some(n) = lines {
        let start = log_lines.len().saturating_sub(n);
        log_lines = log_lines[start..].to_vec();
    }

    // Build JSON output
    let entries: Vec<_> = log_lines
        .iter()
        .filter_map(|line| {
            // Parse log format: "TIMESTAMP: post-hook 'NAME' failed: ERROR"
            let parts: Vec<&str> = line.splitn(2, ": post-hook '").collect();
            if parts.len() == 2 {
                let timestamp = parts[0];
                let rest: Vec<&str> = parts[1].splitn(2, "' failed: ").collect();
                if rest.len() == 2 {
                    return Some(json!({
                        "timestamp": timestamp,
                        "hook": rest[0],
                        "error": rest[1],
                    }));
                }
            }
            None
        })
        .collect();

    let json_output = json!({
        "entries": entries,
        "total": entries.len(),
    });

    // Build text output
    let mut text_output = String::new();

    if log_lines.is_empty() {
        text_output.push_str("No entries in hook failure log.");
    } else {
        let count = log_lines.len();
        text_output.push_str("Hook Failure Log:\n");
        text_output.push('\n');
        for line in &log_lines {
            // Parse and colorize the output
            let parts: Vec<&str> = line.splitn(2, ": post-hook '").collect();
            if parts.len() == 2 {
                let timestamp = parts[0];
                let rest: Vec<&str> = parts[1].splitn(2, "' failed: ").collect();
                if rest.len() == 2 {
                    let hook_name = rest[0];
                    let error = rest[1];
                    text_output.push_str(&format!(
                        "{} {} {} {}\n",
                        timestamp.dimmed(),
                        hook_name.cyan(),
                        "failed:".red(),
                        error
                    ));
                } else {
                    text_output.push_str(&format!("{line}\n"));
                }
            } else {
                text_output.push_str(&format!("{line}\n"));
            }
        }
        text_output.push('\n');
        text_output.push_str(&format!("{count} entries shown"));
        if lines.is_some() {
            text_output.push('\n');
            text_output.push_str(&format!(
                "Log file: {}",
                crate::utils::format_relative_path(&log_path)
            ));
        }
    }

    CommandOutput::new(json_output)
        .with_text(text_output)
        .print(output)
}

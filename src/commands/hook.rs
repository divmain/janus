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
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use owo_colors::OwoColorize;
use serde::Deserialize;
use serde_json::json;

use super::{CommandOutput, print_json};
use crate::error::{JanusError, Result};
use crate::hooks::types::HookEvent;
use crate::hooks::{HookContext, ItemType, context_to_env};
use crate::remote::config::Config;
use crate::ticket::Ticket;
use crate::types::TICKETS_DIR;

/// The directory within .janus where hook scripts are stored.
const HOOKS_DIR: &str = "hooks";

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
pub fn cmd_hook_list(output_json: bool) -> Result<()> {
    let config = Config::load()?;

    if output_json {
        let mut scripts_map = serde_json::Map::new();
        for (event, script) in &config.hooks.scripts {
            scripts_map.insert(event.clone(), json!(script));
        }

        print_json(&json!({
            "enabled": config.hooks.enabled,
            "timeout": config.hooks.timeout,
            "scripts": scripts_map,
        }))?;
    } else {
        let status = if config.hooks.enabled {
            "enabled".green().to_string()
        } else {
            "disabled".red().to_string()
        };
        println!("Hooks: {}", status);
        println!("Timeout: {}s", config.hooks.timeout);
        println!();

        if config.hooks.scripts.is_empty() {
            println!("No hooks configured.");
            println!();
            println!("To add hooks, edit {} or run:", ".janus/config.yaml".cyan());
            println!("  janus hook install <recipe>");
        } else {
            println!("Configured hooks:");
            let mut events: Vec<_> = config.hooks.scripts.iter().collect();
            events.sort_by_key(|(k, _)| *k);
            for (event, script) in events {
                println!("  {} → {}", event.cyan(), script);
            }
        }
    }

    Ok(())
}

/// Install a hook recipe from GitHub
pub async fn cmd_hook_install(recipe: &str) -> Result<()> {
    println!("Fetching recipe '{}'...", recipe.cyan());

    let client = reqwest::Client::new();

    // Fetch the recipe directory contents
    let recipe_url = format!("{}/{}", GITHUB_API_BASE, recipe);
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
    let mut files_to_install: Vec<(String, String)> = Vec::new(); // (relative_path, content)

    for item in &contents {
        if item.name == "config.yaml" && item.content_type == "file" {
            // Fetch config.yaml content
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
            // Recursively fetch all files in the files directory
            files_to_install = fetch_files_recursive(&client, &item.path).await?;
        }
    }

    let recipe_config: RecipeConfig = if let Some(ref content) = config_content {
        serde_yaml_ng::from_str(content)?
    } else {
        return Err(JanusError::HookFetchFailed(format!(
            "recipe '{}' is missing config.yaml",
            recipe
        )));
    };

    // Check for conflicts and prompt user
    let janus_dir = PathBuf::from(TICKETS_DIR);
    let mut files_to_write: Vec<(PathBuf, String, bool)> = Vec::new(); // (path, content, is_executable)

    for (relative_path, content) in &files_to_install {
        let target_path = janus_dir.join(relative_path);
        let is_hook_script = relative_path.starts_with("hooks/");

        if target_path.exists() {
            print!(
                "File {} already exists. [R]eplace/[A]bort/[S]kip? ",
                relative_path.yellow()
            );
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            match input.trim().to_lowercase().as_str() {
                "r" | "replace" => {
                    files_to_write.push((target_path, content.clone(), is_hook_script));
                }
                "a" | "abort" => {
                    println!("Installation aborted.");
                    return Ok(());
                }
                _ => {
                    println!("  Skipping {}", relative_path);
                }
            }
        } else {
            files_to_write.push((target_path, content.clone(), is_hook_script));
        }
    }

    // Create directories and write files
    for (path, content, is_executable) in &files_to_write {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                JanusError::Io(std::io::Error::new(
                    e.kind(),
                    format!("Failed to create directory for hook at {}: {}", parent.display(), e),
                ))
            })?;
        }
        fs::write(path, content).map_err(|e| {
            JanusError::Io(std::io::Error::new(
                e.kind(),
                format!("Failed to write hook file at {}: {}", path.display(), e),
            ))
        })?;

        // Set executable bit on hook scripts
        if *is_executable {
            let mut perms = fs::metadata(path).map_err(|e| {
                JanusError::Io(std::io::Error::new(
                    e.kind(),
                    format!("Failed to get metadata for hook at {}: {}", path.display(), e),
                ))
            })?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(path, perms).map_err(|e| {
                JanusError::Io(std::io::Error::new(
                    e.kind(),
                    format!("Failed to set permissions for hook at {}: {}", path.display(), e),
                ))
            })?;
        }

        println!("  Installed {}", path.display().to_string().green());
    }

    // Merge config into .janus/config.yaml
    if let Some(hooks_config) = recipe_config.hooks
        && let Some(scripts) = hooks_config.scripts
    {
        let mut config = Config::load()?;
        for (event, script) in scripts {
            config.hooks.scripts.insert(event, script);
        }
        config.save()?;
        println!("  Updated {}", "config.yaml".green());
    }

    println!();
    println!(
        "{} Recipe '{}' installed successfully!",
        "✓".green(),
        recipe
    );

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
        let url = format!(
            "https://api.github.com/repos/divmain/janus/contents/{}",
            path
        );
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

/// Run a hook manually for testing
pub async fn cmd_hook_run(event: &str, id: Option<&str>) -> Result<()> {
    let hook_event: HookEvent = event.parse()?;

    let config = Config::load()?;

    // Get the script for this event
    let script_name = config
        .hooks
        .get_script(hook_event.as_str())
        .ok_or_else(|| {
            JanusError::Other(format!(
                "No hook configured for event '{}'. Configure it in .janus/config.yaml",
                event
            ))
        })?;

    let janus_root = PathBuf::from(TICKETS_DIR);
    let hooks_dir = janus_root.join(HOOKS_DIR).canonicalize()?;
    let script_path = hooks_dir.join(script_name);

    if !script_path.exists() {
        return Err(JanusError::HookScriptNotFound(script_path));
    }

    // Canonicalize the script path to resolve any symlinks
    let script_path = script_path.canonicalize()?;

    // Security check: ensure the canonicalized script path is still within the hooks directory
    if !script_path.starts_with(&hooks_dir) {
        return Err(JanusError::HookSecurity(format!(
            "Script path '{}' resolves outside hooks directory",
            script_path.display()
        )));
    }

    // Build context
    let mut context = HookContext::new().with_event(hook_event);

    // If an ID is provided, try to find the item and add context
    if let Some(item_id) = id {
        // Try to find as ticket first
        if let Ok(ticket) = Ticket::find(item_id).await {
            context = context
                .with_item_type(ItemType::Ticket)
                .with_item_id(&ticket.id)
                .with_file_path(&ticket.file_path);
        } else if let Ok(plan) = crate::plan::Plan::find(item_id).await {
            context = context
                .with_item_type(ItemType::Plan)
                .with_item_id(&plan.id)
                .with_file_path(&plan.file_path);
        } else {
            return Err(JanusError::Other(format!(
                "Could not find ticket or plan with ID '{}'",
                item_id
            )));
        }
    }

    // Build environment variables
    let env_vars = context_to_env(&context, &janus_root);

    println!("Running hook: {} → {}", event.cyan(), script_name);
    println!();
    println!("Environment variables:");
    let mut sorted_vars: Vec<_> = env_vars.iter().collect();
    sorted_vars.sort_by_key(|(k, _)| *k);
    for (key, value) in sorted_vars {
        println!("  {}={}", key.dimmed(), value);
    }
    println!();

    // Execute the script
    let output = std::process::Command::new(&script_path)
        .envs(env_vars)
        .current_dir(&janus_root)
        .output()?;

    // Print output
    if !output.stdout.is_empty() {
        println!("stdout:");
        println!("{}", String::from_utf8_lossy(&output.stdout));
    }

    if !output.stderr.is_empty() {
        println!("stderr:");
        println!("{}", String::from_utf8_lossy(&output.stderr).red());
    }

    if output.status.success() {
        println!("{} Hook completed successfully", "✓".green());
    } else {
        let exit_code = output.status.code().unwrap_or(-1);
        println!(
            "{} Hook failed with exit code {}",
            "✗".red(),
            exit_code.to_string().red()
        );
    }

    Ok(())
}

/// Enable hooks
pub fn cmd_hook_enable(output_json: bool) -> Result<()> {
    let mut config = Config::load()?;
    config.hooks.enabled = true;
    config.save()?;

    CommandOutput::new(json!({
        "action": "enabled",
        "hooks_enabled": true,
    }))
    .with_text(format!("{} Hooks enabled", "✓".green()))
    .print(output_json)
}

/// Disable hooks
pub fn cmd_hook_disable(output_json: bool) -> Result<()> {
    let mut config = Config::load()?;
    config.hooks.enabled = false;
    config.save()?;

    CommandOutput::new(json!({
        "action": "disabled",
        "hooks_enabled": false,
    }))
    .with_text(format!("{} Hooks disabled", "✓".red()))
    .print(output_json)
}

/// Display hook failure log
pub fn cmd_hook_log(lines: Option<usize>, output_json: bool) -> Result<()> {
    let log_path = PathBuf::from(TICKETS_DIR).join("hooks.log");

    if !log_path.exists() {
        if output_json {
            print_json(&json!({
                "entries": [],
                "message": "No hook failures logged"
            }))?;
        } else {
            println!("No hook failures logged.");
            println!();
            println!("The hook failure log will appear here after a post-hook fails.");
            println!("Log file: {}", log_path.display());
        }
        return Ok(());
    }

    let content = fs::read_to_string(&log_path).map_err(|e| {
        JanusError::Io(std::io::Error::new(
            e.kind(),
            format!("Failed to read hook log at {}: {}", log_path.display(), e),
        ))
    })?;
    let mut log_lines: Vec<&str> = content.lines().collect();

    // If lines is specified, take only the last N lines
    if let Some(n) = lines {
        let start = log_lines.len().saturating_sub(n);
        log_lines = log_lines[start..].to_vec();
    }

    if output_json {
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

        print_json(&json!({
            "entries": entries,
            "total": entries.len(),
        }))?;
    } else if log_lines.is_empty() {
        println!("No entries in hook failure log.");
    } else {
        let count = log_lines.len();
        println!("Hook Failure Log:");
        println!();
        for line in &log_lines {
            // Parse and colorize the output
            let parts: Vec<&str> = line.splitn(2, ": post-hook '").collect();
            if parts.len() == 2 {
                let timestamp = parts[0];
                let rest: Vec<&str> = parts[1].splitn(2, "' failed: ").collect();
                if rest.len() == 2 {
                    let hook_name = rest[0];
                    let error = rest[1];
                    println!(
                        "{} {} {} {}",
                        timestamp.dimmed(),
                        hook_name.cyan(),
                        "failed:".red(),
                        error
                    );
                } else {
                    println!("{}", line);
                }
            } else {
                println!("{}", line);
            }
        }
        println!();
        println!("{} entries shown", count);
        if lines.is_some() {
            println!("Log file: {}", log_path.display());
        }
    }

    Ok(())
}

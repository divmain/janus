//! Commands for managing hooks.
//!
//! - `list`: Show configured hooks
//! - `install`: Install a hook recipe from GitHub
//! - `run`: Run a hook manually for testing
//! - `enable`: Enable hooks
//! - `disable`: Disable hooks

use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use owo_colors::OwoColorize;
use serde::Deserialize;
use serde_json::json;

use super::print_json;
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
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)?;

        // Set executable bit on hook scripts
        if *is_executable {
            let mut perms = fs::metadata(path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(path, perms)?;
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
pub fn cmd_hook_run(event: &str, id: Option<&str>) -> Result<()> {
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
    let script_path = janus_root.join(HOOKS_DIR).join(script_name);

    if !script_path.exists() {
        return Err(JanusError::HookScriptNotFound(script_path));
    }

    // Canonicalize the script path to resolve any symlinks
    let script_path = script_path.canonicalize()?;

    // Build context
    let mut context = HookContext::new().with_event(hook_event);

    // If an ID is provided, try to find the item and add context
    if let Some(item_id) = id {
        // Try to find as ticket first
        if let Ok(ticket) = Ticket::find(item_id) {
            context = context
                .with_item_type(ItemType::Ticket)
                .with_item_id(&ticket.id)
                .with_file_path(&ticket.file_path);
        } else if let Ok(plan) = crate::plan::Plan::find(item_id) {
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

    if output_json {
        print_json(&json!({
            "action": "enabled",
            "hooks_enabled": true,
        }))?;
    } else {
        println!("{} Hooks enabled", "✓".green());
    }

    Ok(())
}

/// Disable hooks
pub fn cmd_hook_disable(output_json: bool) -> Result<()> {
    let mut config = Config::load()?;
    config.hooks.enabled = false;
    config.save()?;

    if output_json {
        print_json(&json!({
            "action": "disabled",
            "hooks_enabled": false,
        }))?;
    } else {
        println!("{} Hooks disabled", "✓".red());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use serial_test::serial;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_env() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        let janus_dir = temp_dir.path().join(".janus");
        let hooks_dir = janus_dir.join("hooks");
        fs::create_dir_all(&hooks_dir).unwrap();
        temp_dir
    }

    #[test]
    #[serial]
    fn test_hook_list_no_config() {
        let temp_dir = setup_test_env();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        // Should succeed with default config
        let result = cmd_hook_list(false);
        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    fn test_hook_list_with_config() {
        let temp_dir = setup_test_env();
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
        let temp_dir = setup_test_env();
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
        let temp_dir = setup_test_env();
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
        let temp_dir = setup_test_env();
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
        let temp_dir = setup_test_env();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let result = cmd_hook_enable(true);
        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    fn test_hook_disable_json() {
        let temp_dir = setup_test_env();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let result = cmd_hook_disable(true);
        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    fn test_hook_run_no_script_configured() {
        let temp_dir = setup_test_env();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        // No hooks configured
        let result = cmd_hook_run("post_write", None);
        assert!(result.is_err());
    }

    #[test]
    #[serial]
    fn test_hook_run_script_not_found() {
        let temp_dir = setup_test_env();
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

        let result = cmd_hook_run("post_write", None);
        assert!(matches!(result, Err(JanusError::HookScriptNotFound(_))));
    }

    #[test]
    #[serial]
    fn test_hook_run_success() {
        let temp_dir = setup_test_env();
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

        let result = cmd_hook_run("post_write", None);
        assert!(result.is_ok(), "Expected Ok, got: {:?}", result);
    }

    #[test]
    fn test_hook_run_invalid_event() {
        let result = cmd_hook_run("invalid_event", None);
        assert!(matches!(result, Err(JanusError::InvalidHookEvent(_))));
    }

    #[test]
    fn test_invalid_hook_event_error_message() {
        let result = cmd_hook_run("not_a_real_event", None);
        match result {
            Err(JanusError::InvalidHookEvent(event)) => {
                assert_eq!(event, "not_a_real_event");
            }
            other => panic!("Expected InvalidHookEvent, got: {:?}", other),
        }
    }

    #[test]
    fn test_hook_recipe_not_found_error() {
        // Test the error variant directly
        let error = JanusError::HookRecipeNotFound("nonexistent-recipe".to_string());
        let message = error.to_string();
        assert!(message.contains("nonexistent-recipe"));
        assert!(message.contains("not found"));
    }

    #[test]
    fn test_hook_fetch_failed_error() {
        // Test the error variant directly
        let error = JanusError::HookFetchFailed("network error".to_string());
        let message = error.to_string();
        assert!(message.contains("network error"));
        assert!(message.contains("failed to fetch"));
    }
}

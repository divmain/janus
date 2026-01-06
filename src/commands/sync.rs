//! Commands for syncing with remote issue trackers.
//!
//! - `adopt`: Fetch a remote issue and create a local ticket
//! - `push`: Create a remote issue from a local ticket  
//! - `remote-link`: Link a local ticket to an existing remote issue
//! - `sync`: Synchronize state between local and remote

use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

use owo_colors::OwoColorize;
use serde_json::json;

use crate::error::{JanusError, Result};
use crate::remote::config::Config;
use crate::remote::github::GitHubProvider;
use crate::remote::linear::LinearProvider;
use crate::remote::{IssueUpdates, Platform, RemoteIssue, RemoteProvider, RemoteRef, RemoteStatus};
use crate::ticket::Ticket;
use crate::types::TICKETS_ITEMS_DIR;
use crate::utils::{ensure_dir, generate_id_with_custom_prefix, iso_date};

/// Adopt a remote issue and create a local ticket
pub async fn cmd_adopt(
    remote_ref_str: &str,
    prefix: Option<&str>,
    output_json: bool,
) -> Result<()> {
    // Validate prefix before attempting to fetch remote issue
    if let Some(p) = prefix {
        crate::utils::validate_prefix(p)?;
    }

    let config = Config::load()?;
    let remote_ref = RemoteRef::parse(remote_ref_str, Some(&config))?;

    // Fetch the remote issue
    let remote_issue = match remote_ref.platform() {
        Platform::GitHub => {
            let provider = GitHubProvider::from_config(&config)?;
            provider.fetch_issue(&remote_ref).await?
        }
        Platform::Linear => {
            let provider = LinearProvider::from_config(&config)?;
            provider.fetch_issue(&remote_ref).await?
        }
    };

    // Create the local ticket
    let id = create_ticket_from_remote(&remote_issue, &remote_ref, prefix)?;

    if output_json {
        let status = remote_issue.status.to_ticket_status();
        let output = json!({
            "id": id,
            "action": "adopted",
            "remote_ref": remote_ref.to_string(),
            "title": remote_issue.title,
            "url": remote_issue.url,
            "status": status.to_string(),
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("Created {} from {}", id.cyan(), remote_ref);
        println!("  Title: {}", remote_issue.title);
        println!("  URL: {}", remote_issue.url.dimmed());
    }

    Ok(())
}

/// Create a local ticket from a remote issue
fn create_ticket_from_remote(
    remote_issue: &RemoteIssue,
    remote_ref: &RemoteRef,
    prefix: Option<&str>,
) -> Result<String> {
    ensure_dir()?;

    let id = generate_id_with_custom_prefix(prefix)?;
    let now = iso_date();

    // Map remote status to local status
    let status = remote_issue.status.to_ticket_status();

    // Map priority (default to P2)
    let priority = remote_issue.priority.unwrap_or(2);

    // Build frontmatter
    let mut frontmatter_lines = vec![
        "---".to_string(),
        format!("id: {}", id),
        format!("status: {}", status),
        "deps: []".to_string(),
        "links: []".to_string(),
        format!("created: {}", now),
        "type: task".to_string(),
        format!("priority: {}", priority),
    ];

    frontmatter_lines.push(format!("remote: {}", remote_ref));
    frontmatter_lines.push("---".to_string());

    let frontmatter = frontmatter_lines.join("\n");

    // Build body
    let body = if remote_issue.body.is_empty() {
        format!("# {}\n", remote_issue.title)
    } else {
        format!("# {}\n\n{}\n", remote_issue.title, remote_issue.body)
    };

    let content = format!("{}\n{}", frontmatter, body);

    let file_path = PathBuf::from(TICKETS_ITEMS_DIR).join(format!("{}.md", id));
    fs::create_dir_all(TICKETS_ITEMS_DIR)?;
    fs::write(file_path, content)?;

    Ok(id)
}

/// Push a local ticket to create a remote issue
pub async fn cmd_push(local_id: &str, output_json: bool) -> Result<()> {
    let config = Config::load()?;

    // Find and read the local ticket
    let ticket = Ticket::find(local_id)?;
    let metadata = ticket.read()?;

    // Check if already linked
    if metadata.remote.is_some() {
        return Err(JanusError::AlreadyLinked(
            metadata.remote.unwrap_or_default(),
        ));
    }

    // Get title and body
    let title = metadata.title.unwrap_or_else(|| "Untitled".to_string());
    let content = ticket.read_content()?;
    let body = extract_body(&content);

    // Determine which provider to use
    let default_remote = config.default_remote.as_ref().ok_or_else(|| {
        JanusError::Config(
            "No default_remote configured. Run: janus config set default_remote <platform:org>"
                .to_string(),
        )
    })?;

    let remote_ref = match default_remote.platform {
        Platform::GitHub => {
            let provider = GitHubProvider::from_config(&config)?;
            provider.create_issue(&title, &body).await?
        }
        Platform::Linear => {
            let provider = LinearProvider::from_config(&config)?;
            provider.create_issue(&title, &body).await?
        }
    };

    // Update the local ticket with the remote reference
    ticket.update_field("remote", &remote_ref.to_string())?;

    if output_json {
        let output = json!({
            "id": ticket.id,
            "action": "pushed",
            "remote_ref": remote_ref.to_string(),
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("Created {}", remote_ref.to_string().green());
        println!("Updated {} -> remote: {}", ticket.id.cyan(), remote_ref);
    }

    Ok(())
}

/// Link a local ticket to an existing remote issue
pub async fn cmd_remote_link(
    local_id: &str,
    remote_ref_str: &str,
    output_json: bool,
) -> Result<()> {
    let config = Config::load()?;

    // Find the local ticket
    let ticket = Ticket::find(local_id)?;
    let metadata = ticket.read()?;

    // Check if already linked
    if metadata.remote.is_some() {
        return Err(JanusError::AlreadyLinked(
            metadata.remote.unwrap_or_default(),
        ));
    }

    // Parse and validate the remote reference
    let remote_ref = RemoteRef::parse(remote_ref_str, Some(&config))?;

    // Verify the remote issue exists
    let _remote_issue = match remote_ref.platform() {
        Platform::GitHub => {
            let provider = GitHubProvider::from_config(&config)?;
            provider.fetch_issue(&remote_ref).await?
        }
        Platform::Linear => {
            let provider = LinearProvider::from_config(&config)?;
            provider.fetch_issue(&remote_ref).await?
        }
    };

    // Update the local ticket
    ticket.update_field("remote", &remote_ref.to_string())?;

    if output_json {
        let output = json!({
            "id": ticket.id,
            "action": "remote_linked",
            "remote_ref": remote_ref.to_string(),
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!(
            "Linked {} -> {}",
            ticket.id.cyan(),
            remote_ref.to_string().green()
        );
    }

    Ok(())
}

/// Sync a local ticket with its remote issue
pub async fn cmd_sync(local_id: &str, output_json: bool) -> Result<()> {
    let config = Config::load()?;

    // Find and read the local ticket
    let ticket = Ticket::find(local_id)?;
    let metadata = ticket.read()?;

    // Get the remote reference
    let remote_ref_str = metadata.remote.as_ref().ok_or(JanusError::NotLinked)?;
    let remote_ref = RemoteRef::parse(remote_ref_str, Some(&config))?;

    // Fetch the remote issue
    let remote_issue = match remote_ref.platform() {
        Platform::GitHub => {
            let provider = GitHubProvider::from_config(&config)?;
            provider.fetch_issue(&remote_ref).await?
        }
        Platform::Linear => {
            let provider = LinearProvider::from_config(&config)?;
            provider.fetch_issue(&remote_ref).await?
        }
    };

    // Get local values
    let local_title = metadata.title.clone().unwrap_or_default();
    let local_status = metadata.status.unwrap_or_default();
    let local_content = ticket.read_content()?;
    let _local_body = extract_body(&local_content);

    // For JSON output, just report differences without making changes
    if output_json {
        let remote_status = remote_issue.status.to_ticket_status();
        let mut differences: Vec<serde_json::Value> = Vec::new();

        if local_title != remote_issue.title {
            differences.push(json!({
                "field": "title",
                "local": local_title,
                "remote": remote_issue.title,
            }));
        }

        if local_status != remote_status {
            differences.push(json!({
                "field": "status",
                "local": local_status.to_string(),
                "remote": remote_status.to_string(),
            }));
        }

        let output = json!({
            "id": ticket.id,
            "remote_ref": remote_ref.to_string(),
            "already_in_sync": differences.is_empty(),
            "differences": differences,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    // Track changes
    let mut changes_made = false;
    let mut local_updates: Vec<(&str, String)> = Vec::new();
    let mut remote_updates = IssueUpdates::default();

    // Compare title
    if local_title != remote_issue.title {
        println!("\n{}", "Title differs:".yellow());
        println!("  Local:  {}", local_title);
        println!("  Remote: {}", remote_issue.title);

        match prompt_sync_choice()? {
            SyncChoice::LocalToRemote => {
                remote_updates.title = Some(local_title.clone());
                println!("  -> Will update remote title");
                changes_made = true;
            }
            SyncChoice::RemoteToLocal => {
                // Update title in content
                let new_content = update_title_in_content(&local_content, &remote_issue.title);
                ticket.write(&new_content)?;
                println!("  -> Updated local title");
                changes_made = true;
            }
            SyncChoice::Skip => {
                println!("  -> Skipped");
            }
        }
    }

    // Compare status
    let remote_status = remote_issue.status.to_ticket_status();
    if local_status != remote_status {
        println!("\n{}", "Status differs:".yellow());
        println!("  Local:  {}", local_status);
        println!("  Remote: {} ({})", remote_status, remote_issue.status);

        match prompt_sync_choice()? {
            SyncChoice::LocalToRemote => {
                remote_updates.status = Some(RemoteStatus::from_ticket_status(local_status));
                println!("  -> Will update remote status");
                changes_made = true;
            }
            SyncChoice::RemoteToLocal => {
                local_updates.push(("status", remote_status.to_string()));
                println!("  -> Will update local status");
                changes_made = true;
            }
            SyncChoice::Skip => {
                println!("  -> Skipped");
            }
        }
    }

    // Apply local updates
    for (field, value) in local_updates {
        ticket.update_field(field, &value)?;
    }

    // Apply remote updates
    if !remote_updates.is_empty() {
        match remote_ref.platform() {
            Platform::GitHub => {
                let provider = GitHubProvider::from_config(&config)?;
                provider.update_issue(&remote_ref, remote_updates).await?;
            }
            Platform::Linear => {
                let provider = LinearProvider::from_config(&config)?;
                provider.update_issue(&remote_ref, remote_updates).await?;
            }
        }
    }

    if changes_made {
        println!("\n{}", "Sync complete.".green());
    } else {
        println!("\n{}", "Already in sync.".green());
    }

    Ok(())
}

/// Extract the body content from a ticket file (after frontmatter, excluding title)
fn extract_body(content: &str) -> String {
    // Find end of frontmatter
    if let Some(end_idx) = content.find("\n---\n") {
        let after_frontmatter = &content[end_idx + 5..];
        // Skip the title line (# Title)
        let lines: Vec<&str> = after_frontmatter.lines().collect();
        let body_start = lines
            .iter()
            .position(|l| !l.starts_with('#') && !l.is_empty())
            .unwrap_or(0);
        lines[body_start..].join("\n").trim().to_string()
    } else {
        String::new()
    }
}

/// Update the title in ticket content
fn update_title_in_content(content: &str, new_title: &str) -> String {
    let title_re = regex::Regex::new(r"(?m)^#\s+.*$").unwrap();
    title_re
        .replace(content, format!("# {}", new_title))
        .into_owned()
}

#[derive(Debug, Clone, Copy)]
enum SyncChoice {
    LocalToRemote,
    RemoteToLocal,
    Skip,
}

fn prompt_sync_choice() -> Result<SyncChoice> {
    print!("Sync? [l]ocal->remote, [r]emote->local, [s]kip: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    match input.trim().to_lowercase().as_str() {
        "l" | "local" => Ok(SyncChoice::LocalToRemote),
        "r" | "remote" => Ok(SyncChoice::RemoteToLocal),
        "s" | "skip" | "" => Ok(SyncChoice::Skip),
        _ => Ok(SyncChoice::Skip),
    }
}

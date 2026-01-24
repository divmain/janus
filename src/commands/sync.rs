//! Commands for syncing with remote issue trackers.
//!
//! - `adopt`: Fetch a remote issue and create a local ticket
//! - `push`: Create a remote issue from a local ticket
//! - `remote-link`: Link a local ticket to an existing remote issue
//! - `sync`: Synchronize state between local and remote

use std::io::{self, Write};

use owo_colors::OwoColorize;
use serde_json::json;

use super::{CommandOutput, print_json};
use crate::error::{JanusError, Result};
use crate::remote::config::Config;
use crate::remote::{
    IssueUpdates, RemoteIssue, RemoteProvider, RemoteRef, RemoteStatus, create_provider,
};
use crate::ticket::extract_body;
use crate::ticket::{Ticket, TicketBuilder, update_title};

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
    let provider = create_provider(&remote_ref.platform(), &config)?;
    let remote_issue = provider.fetch_issue(&remote_ref).await?;

    // Create the local ticket
    let id = create_ticket_from_remote(&remote_issue, &remote_ref, prefix)?;

    let status = remote_issue.status.to_ticket_status();
    let title = remote_issue.title.clone();
    let url = remote_issue.url.clone();
    let remote_ref_str = remote_ref.to_string();

    CommandOutput::new(json!({
        "id": id,
        "action": "adopted",
        "remote_ref": remote_ref_str,
        "title": title,
        "url": url,
        "status": status.to_string(),
    }))
    .with_text_fn(move || {
        format!(
            "Created {} from {}\n  Title: {}\n  URL: {}",
            id.cyan(),
            remote_ref_str,
            title,
            url.dimmed()
        )
    })
    .print(output_json)
}

/// Create a local ticket from a remote issue
fn create_ticket_from_remote(
    remote_issue: &RemoteIssue,
    remote_ref: &RemoteRef,
    prefix: Option<&str>,
) -> Result<String> {
    let status = remote_issue.status.to_ticket_status();

    let priority = remote_issue.priority.unwrap_or(2);

    let body = if remote_issue.body.is_empty() {
        None
    } else {
        Some(remote_issue.body.clone())
    };

    let (id, _file_path) = TicketBuilder::new(&remote_issue.title)
        .description(body)
        .prefix(prefix)
        .ticket_type("task")
        .status(status.to_string())
        .priority(priority.to_string())
        .remote(Some(remote_ref.to_string()))
        .run_hooks(false)
        .build()?;

    Ok(id)
}

/// Push a local ticket to create a remote issue
pub async fn cmd_push(local_id: &str, output_json: bool) -> Result<()> {
    let config = Config::load()?;

    // Find and read the local ticket
    let ticket = Ticket::find(local_id).await?;
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

    let provider = create_provider(&default_remote.platform, &config)?;
    let remote_ref = provider.create_issue(&title, &body).await?;

    // Update the local ticket with the remote reference
    let remote_ref_str = remote_ref.to_string();
    ticket.update_field("remote", &remote_ref_str)?;

    let ticket_id = ticket.id.clone();
    CommandOutput::new(json!({
        "id": ticket_id,
        "action": "pushed",
        "remote_ref": remote_ref_str,
    }))
    .with_text_fn(move || {
        format!(
            "Created {}\nUpdated {} -> remote: {}",
            remote_ref_str.green(),
            ticket_id.cyan(),
            remote_ref_str
        )
    })
    .print(output_json)
}

/// Link a local ticket to an existing remote issue
pub async fn cmd_remote_link(
    local_id: &str,
    remote_ref_str: &str,
    output_json: bool,
) -> Result<()> {
    let config = Config::load()?;

    // Find the local ticket
    let ticket = Ticket::find(local_id).await?;
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
    let provider = create_provider(&remote_ref.platform(), &config)?;
    let _remote_issue = provider.fetch_issue(&remote_ref).await?;

    // Update the local ticket
    let remote_ref_str = remote_ref.to_string();
    ticket.update_field("remote", &remote_ref_str)?;

    let ticket_id = ticket.id.clone();
    CommandOutput::new(json!({
        "id": ticket_id,
        "action": "remote_linked",
        "remote_ref": remote_ref_str,
    }))
    .with_text_fn(move || format!("Linked {} -> {}", ticket_id.cyan(), remote_ref_str.green()))
    .print(output_json)
}

/// Sync plan representing the computed differences between local and remote
#[derive(Debug)]
struct SyncPlan {
    title_diff: Option<TitleDiff>,
    status_diff: Option<StatusDiff>,
}

/// Title difference information
#[derive(Debug)]
struct TitleDiff {
    local: String,
    remote: String,
}

/// Status difference information (includes both local and remote status representations)
#[derive(Debug)]
struct StatusDiff {
    local: crate::types::TicketStatus,
    remote_status: crate::types::TicketStatus,
    remote_raw: crate::remote::RemoteStatus,
}

/// User decision for how to sync a field difference
#[derive(Debug, Clone)]
enum SyncDecision {
    UpdateLocal { field: String, value: String },
    UpdateRemote(IssueUpdates),
    Skip,
    UpdateLocalTitle { new_content: String },
}

/// Compute the sync state by comparing local and remote ticket values
fn compute_sync_state(
    local_title: String,
    local_status: crate::types::TicketStatus,
    remote_issue: &RemoteIssue,
) -> SyncPlan {
    let title_diff = if local_title != remote_issue.title {
        Some(TitleDiff {
            local: local_title,
            remote: remote_issue.title.clone(),
        })
    } else {
        None
    };

    let remote_ticket_status = remote_issue.status.to_ticket_status();
    let status_diff = if local_status != remote_ticket_status {
        Some(StatusDiff {
            local: local_status,
            remote_status: remote_ticket_status,
            remote_raw: remote_issue.status.clone(),
        })
    } else {
        None
    };

    SyncPlan {
        title_diff,
        status_diff,
    }
}

/// Generate JSON output for sync state
fn generate_sync_json(
    ticket_id: String,
    remote_ref: &RemoteRef,
    sync_plan: &SyncPlan,
) -> serde_json::Value {
    let mut differences: Vec<serde_json::Value> = Vec::new();

    if let Some(ref diff) = sync_plan.title_diff {
        differences.push(json!({
            "field": "title",
            "local": diff.local,
            "remote": diff.remote,
        }));
    }

    if let Some(ref diff) = sync_plan.status_diff {
        differences.push(json!({
            "field": "status",
            "local": diff.local.to_string(),
            "remote": diff.remote_status.to_string(),
        }));
    }

    json!({
        "id": ticket_id,
        "remote_ref": remote_ref.to_string(),
        "already_in_sync": differences.is_empty(),
        "differences": differences,
    })
}

/// Prompt user for sync choices and build a sync plan with decisions
fn prompt_user_for_action(
    sync_plan: &SyncPlan,
    local_content: &str,
) -> Result<(Vec<SyncDecision>, bool)> {
    let mut decisions = Vec::new();
    let mut changes_made = false;

    if let Some(ref diff) = sync_plan.title_diff {
        println!("\n{}", "Title differs:".yellow());
        println!("  Local:  {}", diff.local);
        println!("  Remote: {}", diff.remote);

        match prompt_sync_choice()? {
            SyncChoice::LocalToRemote => {
                let updates = IssueUpdates {
                    title: Some(diff.local.clone()),
                    ..Default::default()
                };
                decisions.push(SyncDecision::UpdateRemote(updates));
                println!("  -> Will update remote title");
                changes_made = true;
            }
            SyncChoice::RemoteToLocal => {
                let new_content = update_title(local_content, &diff.remote);
                decisions.push(SyncDecision::UpdateLocalTitle { new_content });
                println!("  -> Will update local title");
                changes_made = true;
            }
            SyncChoice::Skip => {
                decisions.push(SyncDecision::Skip);
                println!("  -> Skipped");
            }
        }
    }

    if let Some(ref diff) = sync_plan.status_diff {
        println!("\n{}", "Status differs:".yellow());
        println!("  Local:  {}", diff.local);
        println!("  Remote: {} ({})", diff.remote_status, diff.remote_raw);

        match prompt_sync_choice()? {
            SyncChoice::LocalToRemote => {
                let updates = IssueUpdates {
                    status: Some(RemoteStatus::from_ticket_status(diff.local)),
                    ..Default::default()
                };
                decisions.push(SyncDecision::UpdateRemote(updates));
                println!("  -> Will update remote status");
                changes_made = true;
            }
            SyncChoice::RemoteToLocal => {
                decisions.push(SyncDecision::UpdateLocal {
                    field: "status".to_string(),
                    value: diff.remote_status.to_string(),
                });
                println!("  -> Will update local status");
                changes_made = true;
            }
            SyncChoice::Skip => {
                decisions.push(SyncDecision::Skip);
                println!("  -> Skipped");
            }
        }
    }

    Ok((decisions, changes_made))
}

/// Apply sync changes to local and/or remote tickets
async fn apply_sync_changes(
    decisions: &[SyncDecision],
    ticket: &Ticket,
    remote_ref: &RemoteRef,
    config: &Config,
) -> Result<()> {
    let mut remote_updates_to_apply: Option<IssueUpdates> = None;

    for decision in decisions {
        match decision {
            SyncDecision::UpdateLocal { field, value } => {
                ticket.update_field(field, value)?;
            }
            SyncDecision::UpdateRemote(updates) => {
                if remote_updates_to_apply.is_none() {
                    remote_updates_to_apply = Some(IssueUpdates::default());
                }
                let remote_updates = &mut remote_updates_to_apply.as_mut().unwrap();
                if updates.title.is_some() {
                    remote_updates.title = updates.title.clone();
                }
                if updates.status.is_some() {
                    remote_updates.status = updates.status.clone();
                }
            }
            SyncDecision::UpdateLocalTitle { new_content } => {
                ticket.write(new_content)?;
            }
            SyncDecision::Skip => {}
        }
    }

    if let Some(remote_updates) = remote_updates_to_apply {
        let provider = create_provider(&remote_ref.platform(), config)?;
        provider.update_issue(remote_ref, remote_updates).await?;
    }

    Ok(())
}

/// Sync a local ticket with its remote issue
pub async fn cmd_sync(local_id: &str, output_json: bool) -> Result<()> {
    let config = Config::load()?;

    let ticket = Ticket::find(local_id).await?;
    let metadata = ticket.read()?;

    let remote_ref_str = metadata.remote.as_ref().ok_or(JanusError::NotLinked)?;
    let remote_ref = RemoteRef::parse(remote_ref_str, Some(&config))?;

    let provider = create_provider(&remote_ref.platform(), &config)?;
    let remote_issue = provider.fetch_issue(&remote_ref).await?;

    let local_title = metadata.title.clone().ok_or_else(|| {
        JanusError::Other(format!(
            "Ticket {} is missing required field 'title' (file may be corrupted)",
            ticket.id
        ))
    })?;
    let local_status = metadata.status.ok_or_else(|| {
        JanusError::Other(format!(
            "Ticket {} is missing required field 'status' (file may be corrupted)",
            ticket.id
        ))
    })?;
    let local_content = ticket.read_content()?;
    let _local_body = extract_body(&local_content);

    let sync_plan = compute_sync_state(local_title, local_status, &remote_issue);

    if output_json {
        let json_output = generate_sync_json(ticket.id.clone(), &remote_ref, &sync_plan);
        print_json(&json_output)?;
        return Ok(());
    }

    let (decisions, changes_made) = prompt_user_for_action(&sync_plan, &local_content)?;

    apply_sync_changes(&decisions, &ticket, &remote_ref, &config).await?;

    if changes_made {
        println!("\n{}", "Sync complete.".green());
    } else {
        println!("\n{}", "Already in sync.".green());
    }

    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum SyncChoice {
    LocalToRemote,
    RemoteToLocal,
    Skip,
}

fn prompt_sync_choice() -> Result<SyncChoice> {
    loop {
        print!("Sync? [L]ocal->remote (default), [r]emote->local, [s]kip: ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        match input.trim().to_lowercase().as_str() {
            "l" | "local" | "" => return Ok(SyncChoice::LocalToRemote),
            "r" | "remote" => return Ok(SyncChoice::RemoteToLocal),
            "s" | "skip" => return Ok(SyncChoice::Skip),
            _ => println!(
                "Invalid input. Please enter 'l', 'r', or 's' (or press Enter for local->remote)."
            ),
        }
    }
}

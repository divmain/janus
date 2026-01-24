use std::io::{self, Write};

use crate::error::Result;
use crate::remote::{IssueUpdates, RemoteRef, RemoteStatus};
use crate::ticket::update_title;

use super::sync_strategy::SyncPlan;

#[derive(Debug, Clone, Copy)]
pub enum SyncChoice {
    LocalToRemote,
    RemoteToLocal,
    Skip,
}

#[derive(Debug, Clone)]
pub enum SyncDecision {
    UpdateLocal { field: String, value: String },
    UpdateRemote(IssueUpdates),
    Skip,
    UpdateLocalTitle { new_content: String },
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

pub fn prompt_user_for_action(
    sync_plan: &SyncPlan,
    local_content: &str,
) -> Result<(Vec<SyncDecision>, bool)> {
    let mut decisions = Vec::new();
    let mut changes_made = false;

    if let Some(ref diff) = sync_plan.title_diff {
        use owo_colors::OwoColorize;
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
        use owo_colors::OwoColorize;
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

pub fn generate_sync_json(
    ticket_id: String,
    remote_ref: &RemoteRef,
    sync_plan: &SyncPlan,
) -> serde_json::Value {
    use serde_json::json;
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

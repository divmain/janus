//! `janus archive` command.
//!
//! Runs the auto-archive sweep on demand. The same sweep also runs on
//! `janus board` launch; this command is for scripting or for users who want
//! to archive immediately.

use std::time::Duration;

use serde_json::json;

use super::CommandOutput;
use crate::archive::{ArchiveResult, sweep_with_threshold, ticket_age};
use crate::cli::OutputOptions;
use crate::config::Config;
use crate::error::Result;
use crate::store::get_or_init_store;
use crate::types::TicketStatus;

/// Archive Complete tickets that have aged past the configured threshold.
pub async fn cmd_archive(
    days_override: Option<u32>,
    dry_run: bool,
    output: OutputOptions,
) -> Result<()> {
    let config = Config::load().unwrap_or_default();
    let days = days_override.unwrap_or(config.archive.days);

    let Some(threshold) = days_to_threshold(days) else {
        return CommandOutput::new(json!({
            "archived": [],
            "dry_run": dry_run,
            "days": days,
            "disabled": true,
        }))
        .with_text("Auto-archive is disabled (archive.days = 0).")
        .print(output);
    };

    let store = get_or_init_store().await?;
    let tickets = store.get_all_tickets();

    if dry_run {
        let candidates: Vec<String> = tickets
            .iter()
            .filter(|t| t.status == Some(TicketStatus::Complete))
            .filter(|t| {
                ticket_age(t, std::time::SystemTime::now()).is_some_and(|age| age >= threshold)
            })
            .filter_map(|t| t.id.as_ref().map(|id| id.to_string()))
            .collect();
        let text = format_dry_run(&candidates, days);
        return CommandOutput::new(json!({
            "archived": [],
            "candidates": candidates,
            "dry_run": true,
            "days": days,
        }))
        .with_text(text)
        .print(output);
    }

    let result = sweep_with_threshold(&tickets, threshold).await?;
    let text = format_result(&result, days);
    CommandOutput::new(json!({
        "archived": result.archived_ids,
        "errors": result
            .errors
            .iter()
            .map(|(id, err)| json!({"id": id, "error": err}))
            .collect::<Vec<_>>(),
        "dry_run": false,
        "days": days,
    }))
    .with_text(text)
    .print(output)
}

fn days_to_threshold(days: u32) -> Option<Duration> {
    if days == 0 {
        None
    } else {
        Some(Duration::from_secs(days as u64 * 86_400))
    }
}

fn format_dry_run(candidates: &[String], days: u32) -> String {
    if candidates.is_empty() {
        format!("No tickets are older than {days} day(s).")
    } else {
        let ids = candidates.join(", ");
        format!(
            "Would archive {} ticket(s) older than {} day(s): {}",
            candidates.len(),
            days,
            ids
        )
    }
}

fn format_result(result: &ArchiveResult, days: u32) -> String {
    let mut parts = Vec::new();
    if result.archived_ids.is_empty() {
        parts.push(format!("No tickets older than {days} day(s) to archive."));
    } else {
        parts.push(format!(
            "Archived {} ticket(s): {}",
            result.archived_ids.len(),
            result.archived_ids.join(", ")
        ));
    }
    if !result.errors.is_empty() {
        parts.push(format!(
            "Errors on {} ticket(s): {}",
            result.errors.len(),
            result
                .errors
                .iter()
                .map(|(id, err)| format!("{id} ({err})"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    parts.join("\n")
}

//! Objective list command

use owo_colors::OwoColorize;
use serde_json::json;

use crate::cli::OutputOptions;
use crate::commands::CommandOutput;
use crate::error::Result;
use crate::objective::{compute_objective_status, get_all_objectives};
use crate::plan::build_plan_map;
use crate::ticket::build_ticket_map;
use crate::types::ObjectiveStatus;

/// List objectives
///
/// # Arguments
/// * `status_filter` - Optional status filter: "unrealized" or "achieved"
/// * `output` - Output options (JSON vs text)
pub async fn cmd_objective_ls(status_filter: Option<&str>, output: OutputOptions) -> Result<()> {
    let result = get_all_objectives().await?;
    let objectives = result.into_objectives();

    // Build maps for status computation
    let ticket_map = build_ticket_map().await?;
    let plan_map = build_plan_map().await?;

    // Parse the filter once
    let filter: Option<ObjectiveStatus> = if let Some(s) = status_filter {
        Some(s.parse().map_err(|_| {
            crate::error::JanusError::InvalidInput(format!(
                "Invalid objective status filter '{s}'. Valid values: unrealized, achieved"
            ))
        })?)
    } else {
        None
    };

    // Compute statuses and filter
    let filtered: Vec<_> = objectives
        .iter()
        .map(|meta| {
            let status =
                compute_objective_status(meta.satisfied_by.as_deref(), &ticket_map, &plan_map);
            (meta, status)
        })
        .filter(|(_, status)| filter.is_none() || filter == Some(*status))
        .collect();

    // Build JSON output
    let json_objectives: Vec<serde_json::Value> = filtered
        .iter()
        .map(|(meta, status)| {
            json!({
                "id": meta.id,
                "title": meta.title,
                "status": status.to_string(),
                "satisfied_by": meta.satisfied_by,
            })
        })
        .collect();

    // Build text output
    let text_output = if filtered.is_empty() {
        "No objectives found".to_string()
    } else {
        filtered
            .iter()
            .map(|(meta, status)| {
                let id = meta.id.as_deref().unwrap_or("???");
                let title = meta.title.as_deref().unwrap_or("");
                let status_badge = format_objective_status(status);
                let sat_by = meta.satisfied_by.as_deref().unwrap_or("-");
                format!(
                    "{:16} {} {:16} {}",
                    id.cyan(),
                    status_badge,
                    sat_by.dimmed(),
                    title
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    CommandOutput::new(serde_json::Value::Array(json_objectives))
        .with_text(text_output)
        .print(output)
}

/// Format an objective status with color for list display
fn format_objective_status(status: &ObjectiveStatus) -> String {
    let badge = format!("[{status}]");
    match status {
        ObjectiveStatus::Achieved => badge.green().to_string(),
        ObjectiveStatus::Unrealized => badge.yellow().to_string(),
    }
}

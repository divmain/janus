//! Objective show command

use owo_colors::OwoColorize;
use serde_json::json;

use crate::cli::OutputOptions;
use crate::commands::CommandOutput;
use crate::error::Result;
use crate::objective::{Objective, compute_objective_status};
use crate::plan::build_plan_map;
use crate::ticket::build_ticket_map;
use crate::types::ObjectiveStatus;

/// Show objective details
///
/// # Arguments
/// * `id` - Objective ID (full or partial)
/// * `raw` - If true, show raw markdown content
/// * `output` - Output options (JSON vs text)
pub async fn cmd_objective_show(id: &str, raw: bool, output: OutputOptions) -> Result<()> {
    let objective = Objective::find(id).await?;

    if raw {
        let content = objective.read_content()?;
        if output.json {
            return CommandOutput::new(json!({
                "id": objective.id,
                "raw": content,
            }))
            .print(output);
        }
        println!("{content}");
        return Ok(());
    }

    let metadata = objective.read()?;

    // Build maps for status computation
    let ticket_map = build_ticket_map().await?;
    let plan_map = build_plan_map().await?;

    let status = compute_objective_status(&metadata.satisfied_by, &ticket_map, &plan_map);

    if output.json {
        return CommandOutput::new(json!({
            "id": metadata.id,
            "uuid": metadata.uuid,
            "created": metadata.created,
            "title": metadata.title,
            "status": status.to_string(),
            "satisfied_by": metadata.satisfied_by,
            "description": metadata.description,
            "acceptance_criteria": metadata.acceptance_criteria,
        }))
        .print(output);
    }

    // Build text output
    let title = metadata.title.as_deref().unwrap_or("(untitled)");
    let id_str = metadata.id.as_deref().unwrap_or(&objective.id);

    let status_badge = format_objective_status_colored(status);

    println!("{} {}", "Objective:".bold(), id_str.cyan());
    println!("{} {}", "Title:".bold(), title);
    println!("{} {}", "Status:".bold(), status_badge);

    if !metadata.satisfied_by.is_empty() {
        println!("{} {}", "Satisfied by:".bold(), metadata.satisfied_by.join(", ").cyan());
    }

    if let Some(ref created) = metadata.created {
        println!("{} {}", "Created:".bold(), created.to_string().dimmed());
    }

    if let Some(ref desc) = metadata.description {
        println!("\n{}", "## Description".bold());
        println!("{desc}");
    }

    if !metadata.acceptance_criteria.is_empty() {
        println!("\n{}", "## Acceptance Criteria".bold());
        for criterion in &metadata.acceptance_criteria {
            println!("- {criterion}");
        }
    }

    if let Some(ref notes) = metadata.notes_raw {
        println!("\n{}", "## Notes".bold());
        println!("{notes}");
    }

    Ok(())
}

/// Format an objective status with color
fn format_objective_status_colored(status: ObjectiveStatus) -> String {
    let badge = format!("[{status}]");
    match status {
        ObjectiveStatus::Achieved => badge.green().to_string(),
        ObjectiveStatus::Unrealized => badge.yellow().to_string(),
    }
}

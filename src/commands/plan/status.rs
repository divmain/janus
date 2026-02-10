//! Plan status command

use owo_colors::OwoColorize;
use serde_json::json;

use crate::commands::CommandOutput;
use crate::display::format_status_colored;
use crate::error::Result;

use crate::plan::{Plan, compute_all_phase_statuses, compute_plan_status};
use crate::ticket::build_ticket_map;

/// Show plan status summary
///
/// # Arguments
/// * `id` - The plan ID (can be partial)
/// * `output_json` - If true, output result as JSON
pub async fn cmd_plan_status(id: &str, output_json: bool) -> Result<()> {
    let plan = Plan::find(id).await?;
    let metadata = plan.read()?;
    let ticket_map = build_ticket_map().await?;

    // Compute overall plan status
    let plan_status = compute_plan_status(&metadata, &ticket_map);

    let phase_statuses = compute_all_phase_statuses(&metadata, &ticket_map);
    let phases_json: Vec<_> = phase_statuses
        .iter()
        .map(|ps| {
            json!({
                "number": ps.phase_number,
                "name": ps.phase_name,
                "status": ps.status.to_string(),
                "completed_count": ps.completed_count,
                "total_count": ps.total_count,
            })
        })
        .collect();

    let output = json!({
        "plan_id": plan.id,
        "title": metadata.title,
        "status": plan_status.status.to_string(),
        "completed_count": plan_status.completed_count,
        "total_count": plan_status.total_count,
        "progress_percent": plan_status.progress_percent(),
        "phases": phases_json,
    });

    if output_json {
        CommandOutput::new(output).print(output_json)?;
        return Ok(());
    }

    // Print header
    let title = metadata.title.as_deref().unwrap_or("Untitled");
    let plan_id = metadata.id.as_deref().unwrap_or(&plan.id);
    println!("Plan: {} - {}", plan_id.cyan(), title);
    println!("Status: {}", format_status_colored(plan_status.status));
    println!("Progress: {} tickets", plan_status.progress_string());

    // If phased, show breakdown by phase
    if metadata.is_phased() && !phase_statuses.is_empty() {
        println!();
        println!("Phases:");

        // Find max lengths for alignment
        let max_name_len = phase_statuses
            .iter()
            .map(|ps| ps.phase_name.len())
            .max()
            .unwrap_or(0)
            .max(12);

        for ps in &phase_statuses {
            let status_badge = format_status_colored(ps.status);
            let progress = format!("({}/{})", ps.completed_count, ps.total_count);
            println!(
                "  {}. {} {:width$} {}",
                ps.phase_number,
                status_badge,
                ps.phase_name,
                progress.dimmed(),
                width = max_name_len
            );
        }
    }

    Ok(())
}

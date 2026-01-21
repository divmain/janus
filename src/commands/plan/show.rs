//! Plan show command

use std::collections::HashMap;

use owo_colors::OwoColorize;
use serde_json::json;

use super::print_ticket_line;
use crate::commands::print_json;
use crate::commands::ticket_minimal_json_with_exists;
use crate::display::format_status_colored;
use crate::error::{JanusError, Result};
use crate::plan::types::{PlanMetadata, PlanSection};
use crate::plan::{Plan, compute_all_phase_statuses, compute_plan_status};
use crate::ticket::build_ticket_map;
use crate::types::TicketMetadata;

/// Display a plan with full reconstruction
///
/// # Arguments
/// * `id` - The plan ID (can be partial)
/// * `raw` - If true, show raw file content instead of enhanced output
/// * `tickets_only` - If true, show only the ticket list with statuses
/// * `phases_only` - If true, show only phase summary (phased plans)
/// * `verbose_phases` - Phase numbers for which to show full completion summaries
/// * `output_json` - If true, output as JSON
pub async fn cmd_plan_show(
    id: &str,
    raw: bool,
    tickets_only: bool,
    phases_only: bool,
    verbose_phases: &[String],
    output_json: bool,
) -> Result<()> {
    let plan = Plan::find(id).await?;

    if raw {
        // Just print the raw content
        let content = plan.read_content()?;
        println!("{}", content);
        return Ok(());
    }

    let metadata = plan.read()?;

    // Validate --verbose-phase usage
    if !verbose_phases.is_empty() && !metadata.is_phased() {
        return Err(JanusError::VerbosePhaseRequiresPhasedPlan);
    }

    let ticket_map = build_ticket_map().await?;

    // Handle JSON output format
    if output_json {
        return show_plan_json(&metadata, &ticket_map);
    }

    // Handle --tickets-only
    if tickets_only {
        return show_tickets_only(&metadata, &ticket_map);
    }

    // Handle --phases-only
    if phases_only {
        return show_phases_only(&metadata, &ticket_map);
    }

    // Compute overall plan status
    let plan_status = compute_plan_status(&metadata, &ticket_map);

    // Print title with status badge
    if let Some(ref title) = metadata.title {
        println!("{}", format!("# {}", title).bold());
    }

    // Print status and progress
    let status_badge = format_status_colored(plan_status.status);
    let progress = plan_status.progress_string();
    println!();
    println!("{} Progress: {} tickets", status_badge, progress);

    // Print description if present
    if let Some(ref description) = metadata.description {
        println!();
        println!("{}", description);
    }

    // Print acceptance criteria if present
    if !metadata.acceptance_criteria.is_empty() {
        println!();
        println!("{}", "## Acceptance Criteria".bold());
        println!();
        for criterion in &metadata.acceptance_criteria {
            // TODO: Could enhance with checkboxes based on some condition
            println!("- [ ] {}", criterion);
        }
    }

    // Print sections in order
    let phase_statuses = compute_all_phase_statuses(&metadata, &ticket_map);
    let mut phase_idx = 0;

    for section in &metadata.sections {
        println!();
        match section {
            PlanSection::Phase(phase) => {
                // Get the precomputed phase status
                let phase_status = phase_statuses.get(phase_idx);
                phase_idx += 1;

                // Print phase header with status and progress
                let status_str = phase_status
                    .map(|s| format_status_colored(s.status))
                    .unwrap_or_default();
                let progress_str = phase_status
                    .map(|s| format!("({}/{})", s.completed_count, s.total_count))
                    .unwrap_or_default();

                if phase.name.is_empty() {
                    println!(
                        "{} {} {}",
                        format!("## Phase {}", phase.number).bold(),
                        status_str,
                        progress_str.dimmed()
                    );
                } else {
                    println!(
                        "{} {} {}",
                        format!("## Phase {}: {}", phase.number, phase.name).bold(),
                        status_str,
                        progress_str.dimmed()
                    );
                }

                // Print phase description
                if let Some(ref desc) = phase.description {
                    println!();
                    println!("{}", desc);
                }

                // Print success criteria
                if !phase.success_criteria.is_empty() {
                    println!();
                    println!("{}", "### Success Criteria".bold());
                    println!();
                    for criterion in &phase.success_criteria {
                        println!("- {}", criterion);
                    }
                }

                // Print tickets with status
                if !phase.tickets.is_empty() {
                    println!();
                    println!("{}", "### Tickets".bold());
                    println!();
                    let full_summary = verbose_phases.contains(&phase.number);
                    for (i, ticket_id) in phase.tickets.iter().enumerate() {
                        print_ticket_line(i + 1, ticket_id, &ticket_map, full_summary);
                    }
                }
            }
            PlanSection::Tickets(tickets) => {
                // Simple plan tickets section
                println!("{}", "## Tickets".bold());
                println!();
                for (i, ticket_id) in tickets.iter().enumerate() {
                    print_ticket_line(i + 1, ticket_id, &ticket_map, false);
                }
            }
            PlanSection::FreeForm(freeform) => {
                // Free-form section: print verbatim
                println!("{}", format!("## {}", freeform.heading).bold());
                if !freeform.content.is_empty() {
                    println!();
                    println!("{}", freeform.content);
                }
            }
        }
    }

    Ok(())
}

/// Show plan as JSON
fn show_plan_json(
    metadata: &PlanMetadata,
    ticket_map: &HashMap<String, TicketMetadata>,
) -> Result<()> {
    let plan_status = compute_plan_status(metadata, ticket_map);
    let phase_statuses = compute_all_phase_statuses(metadata, ticket_map);

    let tickets_info: Vec<serde_json::Value> = metadata
        .all_tickets()
        .iter()
        .map(|tid| {
            let ticket = ticket_map.get(*tid);
            ticket_minimal_json_with_exists(tid, ticket)
        })
        .collect();

    let phases_info: Vec<serde_json::Value> = metadata
        .phases()
        .iter()
        .zip(phase_statuses.iter())
        .map(|(phase, ps)| {
            let phase_tickets: Vec<serde_json::Value> = phase
                .tickets
                .iter()
                .map(|tid| {
                    let ticket = ticket_map.get(tid);
                    ticket_minimal_json_with_exists(tid, ticket)
                })
                .collect();

            json!({
                "number": phase.number,
                "name": phase.name,
                "status": ps.status.to_string(),
                "completed_count": ps.completed_count,
                "total_count": ps.total_count,
                "tickets": phase_tickets,
            })
        })
        .collect();

    let output = json!({
        "id": metadata.id,
        "uuid": metadata.uuid,
        "title": metadata.title,
        "created": metadata.created,
        "description": metadata.description,
        "status": plan_status.status.to_string(),
        "completed_count": plan_status.completed_count,
        "total_count": plan_status.total_count,
        "progress_percent": plan_status.progress_percent(),
        "acceptance_criteria": metadata.acceptance_criteria,
        "is_phased": metadata.is_phased(),
        "phases": phases_info,
        "tickets": tickets_info,
    });

    print_json(&output)?;
    Ok(())
}

/// Show only the ticket list with statuses
fn show_tickets_only(
    metadata: &PlanMetadata,
    ticket_map: &HashMap<String, TicketMetadata>,
) -> Result<()> {
    let all_tickets = metadata.all_tickets();

    if all_tickets.is_empty() {
        println!("No tickets in plan");
        return Ok(());
    }

    for (i, ticket_id) in all_tickets.iter().enumerate() {
        print_ticket_line(i + 1, ticket_id, ticket_map, false);
    }

    Ok(())
}

/// Show only phase summary (for phased plans)
fn show_phases_only(
    metadata: &PlanMetadata,
    ticket_map: &HashMap<String, TicketMetadata>,
) -> Result<()> {
    if !metadata.is_phased() {
        println!("This is a simple plan (no phases)");
        return Ok(());
    }

    let phase_statuses = compute_all_phase_statuses(metadata, ticket_map);

    if phase_statuses.is_empty() {
        println!("No phases in plan");
        return Ok(());
    }

    // Find max name length for alignment
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
            "{}. {} {:width$} {}",
            ps.phase_number,
            status_badge,
            ps.phase_name,
            progress.dimmed(),
            width = max_name_len
        );
    }

    Ok(())
}

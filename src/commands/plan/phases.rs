//! Plan phase management commands (add, remove)

use serde_json::json;

use crate::commands::CommandOutput;
use crate::error::{JanusError, Result};
use crate::events::{log_phase_added, log_phase_removed};
use crate::plan::Plan;
use crate::plan::parser::serialize_plan;
use crate::plan::types::{Phase, PlanSection};

/// Add a new phase to a plan
///
/// # Arguments
/// * `plan_id` - The plan ID (can be partial)
/// * `phase_name` - Name for the new phase
/// * `after` - Optional phase name/number to insert after
/// * `position` - Optional position (1-indexed)
/// * `output_json` - If true, output result as JSON
pub async fn cmd_plan_add_phase(
    plan_id: &str,
    phase_name: &str,
    after: Option<&str>,
    position: Option<usize>,
    output_json: bool,
) -> Result<()> {
    let plan = Plan::find(plan_id).await?;
    let mut metadata = plan.read()?;

    // Determine the phase number
    let existing_phases = metadata.phases();
    let next_number = if existing_phases.is_empty() {
        1
    } else {
        // Find the highest numeric phase number and add 1
        existing_phases
            .iter()
            .filter_map(|p| p.number.parse::<usize>().ok())
            .max()
            .unwrap_or(0)
            + 1
    };

    let new_phase = Phase::new(next_number.to_string(), phase_name.to_string());

    // Find where to insert the phase
    if let Some(after_identifier) = after {
        // Find the position of the phase to insert after
        let mut insert_idx = None;
        for (i, section) in metadata.sections.iter().enumerate() {
            if let PlanSection::Phase(phase) = section
                && (phase.number.eq_ignore_ascii_case(after_identifier)
                    || phase.name.eq_ignore_ascii_case(after_identifier))
            {
                insert_idx = Some(i + 1);
                break;
            }
        }

        if let Some(idx) = insert_idx {
            metadata.sections.insert(idx, PlanSection::Phase(new_phase));
        } else {
            return Err(JanusError::PhaseNotFound(after_identifier.to_string()));
        }
    } else if let Some(pos) = position {
        // Count phases to find the correct section index
        let mut phase_count = 0;
        let mut insert_idx = metadata.sections.len();

        for (i, section) in metadata.sections.iter().enumerate() {
            if matches!(section, PlanSection::Phase(_)) {
                phase_count += 1;
                if phase_count == pos {
                    insert_idx = i;
                    break;
                }
            }
        }

        // If position is 1, insert before first phase
        if pos == 1 {
            for (i, section) in metadata.sections.iter().enumerate() {
                if matches!(section, PlanSection::Phase(_)) {
                    insert_idx = i;
                    break;
                }
            }
        }

        metadata
            .sections
            .insert(insert_idx, PlanSection::Phase(new_phase));
    } else {
        // Append at the end
        metadata.sections.push(PlanSection::Phase(new_phase));
    }

    // Write updated plan
    let content = serialize_plan(&metadata);
    plan.write(&content)?;

    // Log the event
    log_phase_added(&plan.id, &next_number.to_string(), phase_name);

    CommandOutput::new(json!({
        "plan_id": plan.id,
        "action": "phase_added",
        "phase_number": next_number.to_string(),
        "phase_name": phase_name,
    }))
    .with_text(format!(
        "Added phase '{}' (Phase {}) to plan {}",
        phase_name, next_number, plan.id
    ))
    .print(output_json)
}

/// Remove a phase from a plan
///
/// # Arguments
/// * `plan_id` - The plan ID (can be partial)
/// * `phase` - Phase name or number to remove
/// * `force` - Force removal even if phase contains tickets
/// * `migrate` - Optional target phase to migrate tickets to
/// * `output_json` - If true, output result as JSON
pub async fn cmd_plan_remove_phase(
    plan_id: &str,
    phase: &str,
    force: bool,
    migrate: Option<&str>,
    output_json: bool,
) -> Result<()> {
    let plan = Plan::find(plan_id).await?;
    let mut metadata = plan.read()?;

    // Find the phase and its index
    let mut phase_idx = None;
    let mut phase_tickets: Vec<String> = Vec::new();
    let mut phase_name = String::new();
    let mut phase_number = String::new();

    for (i, section) in metadata.sections.iter().enumerate() {
        if let PlanSection::Phase(p) = section
            && (p.number.eq_ignore_ascii_case(phase) || p.name.eq_ignore_ascii_case(phase))
        {
            phase_idx = Some(i);
            phase_tickets = p.tickets.clone();
            phase_name = p.name.clone();
            phase_number = p.number.clone();
            break;
        }
    }

    let idx = phase_idx.ok_or_else(|| JanusError::PhaseNotFound(phase.to_string()))?;

    let mut migrated_tickets = 0;

    // Check if phase has tickets
    if !phase_tickets.is_empty() {
        if let Some(migrate_to) = migrate {
            // Migrate tickets to another phase
            let target_phase = metadata
                .find_phase_mut(migrate_to)
                .ok_or_else(|| JanusError::PhaseNotFound(migrate_to.to_string()))?;

            for ticket_id in &phase_tickets {
                target_phase.add_ticket(ticket_id.clone());
            }
            migrated_tickets = phase_tickets.len();
            if !output_json {
                println!(
                    "Migrated {} tickets to phase '{}'",
                    migrated_tickets, migrate_to
                );
            }
        } else if !force {
            return Err(JanusError::PhaseNotEmpty(phase_name));
        }
    }

    // Remove the phase
    metadata.sections.remove(idx);

    // Write updated plan
    let content = serialize_plan(&metadata);
    plan.write(&content)?;

    // Log the event
    log_phase_removed(&plan.id, &phase_number, &phase_name, migrated_tickets);

    CommandOutput::new(json!({
        "plan_id": plan.id,
        "action": "phase_removed",
        "phase_number": phase_number,
        "phase_name": phase_name,
        "migrated_tickets": migrated_tickets,
    }))
    .with_text(format!("Removed phase '{}' from plan {}", phase, plan.id))
    .print(output_json)
}

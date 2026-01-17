//! Plan ticket management commands (add, remove, move)

use serde_json::json;

use crate::commands::print_json;
use crate::error::{JanusError, Result};
use crate::plan::Plan;
use crate::plan::parser::serialize_plan;
use crate::plan::types::PlanSection;
use crate::ticket::Ticket;

/// Add a ticket to a plan
///
/// # Arguments
/// * `plan_id` - The plan ID (can be partial)
/// * `ticket_id` - The ticket ID to add
/// * `phase` - Optional phase name/number (required for phased plans)
/// * `after` - Optional ticket ID to insert after
/// * `position` - Optional position (1-indexed)
/// * `output_json` - If true, output result as JSON
pub async fn cmd_plan_add_ticket(
    plan_id: &str,
    ticket_id: &str,
    phase: Option<&str>,
    after: Option<&str>,
    position: Option<usize>,
    output_json: bool,
) -> Result<()> {
    // Validate ticket exists
    let ticket = Ticket::find_async(ticket_id).await?;
    let resolved_ticket_id = ticket.id.clone();

    let plan = Plan::find(plan_id)?;
    let mut metadata = plan.read()?;

    // Check if ticket is already in the plan
    let existing_tickets = metadata.all_tickets();
    if existing_tickets.contains(&resolved_ticket_id.as_str()) {
        return Err(JanusError::TicketAlreadyInPlan(resolved_ticket_id));
    }

    let mut added_to_phase: Option<String> = None;
    #[allow(unused_assignments)]
    let mut added_position: Option<usize> = None;

    if metadata.is_phased() {
        // Phased plan: require --phase option
        let phase_identifier = phase.ok_or(JanusError::PhasedPlanRequiresPhase)?;

        let phase_obj = metadata
            .find_phase_mut(phase_identifier)
            .ok_or_else(|| JanusError::PhaseNotFound(phase_identifier.to_string()))?;

        added_to_phase = Some(phase_obj.name.clone());

        // Add ticket to phase
        if let Some(after_id) = after {
            if !phase_obj.add_ticket_after(&resolved_ticket_id, after_id) {
                return Err(JanusError::TicketNotFound(after_id.to_string()));
            }
            added_position = phase_obj
                .tickets
                .iter()
                .position(|t| t == &resolved_ticket_id);
        } else if let Some(pos) = position {
            phase_obj.add_ticket_at_position(&resolved_ticket_id, pos);
            added_position = Some(pos.saturating_sub(1));
        } else {
            phase_obj.add_ticket(&resolved_ticket_id);
            added_position = Some(phase_obj.tickets.len().saturating_sub(1));
        }
    } else if metadata.is_simple() {
        // Simple plan: --phase option is not allowed
        if phase.is_some() {
            return Err(JanusError::SimpleplanNoPhase);
        }

        let tickets = metadata
            .tickets_section_mut()
            .ok_or_else(|| JanusError::Other("Plan has no tickets section".to_string()))?;

        // Add ticket to list
        if let Some(after_id) = after {
            if let Some(pos) = tickets.iter().position(|t| t == after_id) {
                tickets.insert(pos + 1, resolved_ticket_id.clone());
                added_position = Some(pos + 1);
            } else {
                return Err(JanusError::TicketNotFound(after_id.to_string()));
            }
        } else if let Some(pos) = position {
            let index = pos.saturating_sub(1);
            if index >= tickets.len() {
                tickets.push(resolved_ticket_id.clone());
                added_position = Some(tickets.len().saturating_sub(1));
            } else {
                tickets.insert(index, resolved_ticket_id.clone());
                added_position = Some(index);
            }
        } else {
            tickets.push(resolved_ticket_id.clone());
            added_position = Some(tickets.len().saturating_sub(1));
        }
    } else {
        return Err(JanusError::Other(
            "Plan has no tickets section or phases".to_string(),
        ));
    }

    // Write updated plan
    let content = serialize_plan(&metadata);
    plan.write(&content)?;

    if output_json {
        print_json(&json!({
            "plan_id": plan.id,
            "ticket_id": resolved_ticket_id,
            "action": "ticket_added",
            "phase": added_to_phase,
            "position": added_position,
        }))?;
    } else {
        println!("Added {} to plan {}", resolved_ticket_id, plan.id);
    }
    Ok(())
}

/// Remove a ticket from a plan
///
/// # Arguments
/// * `plan_id` - The plan ID (can be partial)
/// * `ticket_id` - The ticket ID to remove
/// * `output_json` - If true, output result as JSON
pub async fn cmd_plan_remove_ticket(
    plan_id: &str,
    ticket_id: &str,
    output_json: bool,
) -> Result<()> {
    let plan = Plan::find(plan_id)?;
    let mut metadata = plan.read()?;

    // Try to resolve the ticket ID, warning if not found
    let resolved_id = match Ticket::find_async(ticket_id).await {
        Ok(t) => t.id,
        Err(_) => {
            eprintln!("Warning: ticket '{}' not found, using ID as-is", ticket_id);
            ticket_id.to_string()
        }
    };

    let mut found = false;
    let mut removed_from_phase: Option<String> = None;

    // Search in phases
    for section in &mut metadata.sections {
        match section {
            PlanSection::Phase(phase) => {
                if phase.remove_ticket(&resolved_id) {
                    found = true;
                    removed_from_phase = Some(phase.name.clone());
                    break;
                }
            }
            PlanSection::Tickets(tickets) => {
                if let Some(pos) = tickets.iter().position(|t| t == &resolved_id) {
                    tickets.remove(pos);
                    found = true;
                    break;
                }
            }
            PlanSection::FreeForm(_) => {}
        }
    }

    if !found {
        return Err(JanusError::TicketNotInPlan(resolved_id));
    }

    // Write updated plan
    let content = serialize_plan(&metadata);
    plan.write(&content)?;

    if output_json {
        print_json(&json!({
            "plan_id": plan.id,
            "ticket_id": resolved_id,
            "action": "ticket_removed",
            "phase": removed_from_phase,
        }))?;
    } else {
        println!("Removed {} from plan {}", resolved_id, plan.id);
    }
    Ok(())
}

/// Move a ticket between phases
///
/// # Arguments
/// * `plan_id` - The plan ID (can be partial)
/// * `ticket_id` - The ticket ID to move
/// * `to_phase` - Target phase name/number
/// * `after` - Optional ticket ID to insert after in target phase
/// * `position` - Optional position in target phase (1-indexed)
/// * `output_json` - If true, output result as JSON
pub async fn cmd_plan_move_ticket(
    plan_id: &str,
    ticket_id: &str,
    to_phase: &str,
    after: Option<&str>,
    position: Option<usize>,
    output_json: bool,
) -> Result<()> {
    let plan = Plan::find(plan_id)?;
    let mut metadata = plan.read()?;

    if !metadata.is_phased() {
        return Err(JanusError::CannotMoveInSimplePlan);
    }

    // Try to resolve the ticket ID, warning if not found
    let resolved_id = match Ticket::find_async(ticket_id).await {
        Ok(t) => t.id,
        Err(_) => {
            eprintln!("Warning: ticket '{}' not found, using ID as-is", ticket_id);
            ticket_id.to_string()
        }
    };

    // Find and remove the ticket from its current phase
    let mut found_in_phase: Option<String> = None;
    for section in &mut metadata.sections {
        if let PlanSection::Phase(phase) = section
            && phase.remove_ticket(&resolved_id)
        {
            found_in_phase = Some(phase.name.clone());
            break;
        }
    }

    if found_in_phase.is_none() {
        return Err(JanusError::TicketNotInPlan(resolved_id));
    }

    // Add to target phase
    let target_phase = metadata
        .find_phase_mut(to_phase)
        .ok_or_else(|| JanusError::PhaseNotFound(to_phase.to_string()))?;

    if let Some(after_id) = after {
        if !target_phase.add_ticket_after(&resolved_id, after_id) {
            return Err(JanusError::TicketNotFound(after_id.to_string()));
        }
    } else if let Some(pos) = position {
        target_phase.add_ticket_at_position(&resolved_id, pos);
    } else {
        target_phase.add_ticket(&resolved_id);
    }

    // Write updated plan
    let content = serialize_plan(&metadata);
    plan.write(&content)?;

    if output_json {
        print_json(&json!({
            "plan_id": plan.id,
            "ticket_id": resolved_id,
            "action": "ticket_moved",
            "from_phase": found_in_phase,
            "to_phase": to_phase,
        }))?;
    } else {
        println!(
            "Moved {} to phase '{}' in plan {}",
            resolved_id, to_phase, plan.id
        );
    }
    Ok(())
}

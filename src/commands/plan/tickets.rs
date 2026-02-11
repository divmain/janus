//! Plan ticket management commands (add, remove, move)

use serde_json::json;

use crate::commands::CommandOutput;
use crate::error::{JanusError, Result};
use crate::events::{log_ticket_added_to_plan, log_ticket_moved, log_ticket_removed_from_plan};
use crate::plan::Plan;
use crate::plan::types::PlanSection;
use crate::ticket::Ticket;
use crate::types::TicketId;

/// Resolve a partial ticket ID against a list of ticket IDs in a plan.
///
/// Returns the full ID if exactly one ticket matches. Errors on no match or ambiguity.
fn resolve_after_id(partial_id: &str, tickets: &[String]) -> Result<String> {
    // Exact match first
    if let Some(id) = tickets.iter().find(|t| t.as_str() == partial_id) {
        return Ok(id.clone());
    }

    // Substring match
    let matches: Vec<&String> = tickets.iter().filter(|t| t.contains(partial_id)).collect();

    match matches.len() {
        0 => Err(JanusError::TicketNotFound(TicketId::new_unchecked(
            partial_id,
        ))),
        1 => Ok(matches[0].clone()),
        _ => Err(JanusError::AmbiguousId(
            partial_id.to_string(),
            matches.into_iter().cloned().collect(),
        )),
    }
}

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
    let ticket = Ticket::find(ticket_id).await?;
    let resolved_ticket_id = ticket.id.clone();

    let plan = Plan::find(plan_id).await?;
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
            let resolved_after = resolve_after_id(after_id, &phase_obj.ticket_list.tickets)?;
            if !phase_obj.add_ticket_after(&resolved_ticket_id, &resolved_after) {
                return Err(JanusError::TicketNotFound(TicketId::new_unchecked(
                    after_id,
                )));
            }
            added_position = phase_obj
                .ticket_list
                .tickets
                .iter()
                .position(|t| t == &resolved_ticket_id);
        } else if let Some(pos) = position {
            phase_obj.add_ticket_at_position(&resolved_ticket_id, pos);
            added_position = Some(pos.saturating_sub(1));
        } else {
            phase_obj.add_ticket(&resolved_ticket_id);
            added_position = Some(phase_obj.ticket_list.tickets.len().saturating_sub(1));
        }
    } else if metadata.is_simple() {
        // Simple plan: --phase option is not allowed
        if phase.is_some() {
            return Err(JanusError::SimpleplanNoPhase);
        }

        let ts = metadata
            .tickets_section_mut()
            .ok_or_else(|| JanusError::PlanNoTicketsSection)?;

        // Add ticket to list (mutations invalidate tickets_raw automatically)
        if let Some(after_id) = after {
            let resolved_after = resolve_after_id(after_id, &ts.ticket_list.tickets)?;
            if ts.insert_ticket_after(resolved_ticket_id.clone(), &resolved_after) {
                let pos = ts
                    .ticket_list
                    .tickets
                    .iter()
                    .position(|t| t == &resolved_ticket_id)
                    .unwrap();
                added_position = Some(pos);
            } else {
                return Err(JanusError::TicketNotFound(TicketId::new_unchecked(
                    after_id,
                )));
            }
        } else if let Some(pos) = position {
            let index = pos.saturating_sub(1);
            ts.insert_ticket_at(resolved_ticket_id.clone(), pos);
            added_position = Some(index.min(ts.ticket_list.tickets.len().saturating_sub(1)));
        } else {
            ts.add_ticket(resolved_ticket_id.clone());
            added_position = Some(ts.ticket_list.tickets.len().saturating_sub(1));
        }
    } else {
        return Err(JanusError::PlanNoTicketsOrPhases);
    }

    // Write updated plan
    plan.write_metadata(&metadata)?;

    // Log the event
    log_ticket_added_to_plan(&plan.id, &resolved_ticket_id, added_to_phase.as_deref());

    CommandOutput::new(json!({
        "plan_id": plan.id,
        "ticket_id": resolved_ticket_id,
        "action": "ticket_added",
        "phase": added_to_phase,
        "position": added_position,
    }))
    .with_text(format!("Added {} to plan {}", resolved_ticket_id, plan.id))
    .print(output_json)
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
    let plan = Plan::find(plan_id).await?;
    let mut metadata = plan.read()?;

    // Try to resolve the ticket. If it exists, use its canonical ID.
    // If it doesn't exist (dangling reference), validate the raw ID format
    // and use it directly so users can clean up stale plan references.
    let resolved_id = match Ticket::find(ticket_id).await {
        Ok(ticket) => ticket.id,
        Err(JanusError::TicketNotFound(_)) => {
            // Ticket file is gone — validate the ID format and use it as-is
            TicketId::new(ticket_id)?;
            ticket_id.to_string()
        }
        Err(e) => return Err(e),
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
            PlanSection::Tickets(ts) => {
                if ts.remove_ticket(&resolved_id) {
                    found = true;
                    break;
                }
            }
            PlanSection::FreeForm(_) => {}
        }
    }

    if !found {
        return Err(JanusError::TicketNotInPlan(resolved_id.clone()));
    }

    // Write updated plan
    plan.write_metadata(&metadata)?;

    // Log the event
    log_ticket_removed_from_plan(&plan.id, &resolved_id, removed_from_phase.as_deref());

    CommandOutput::new(json!({
        "plan_id": plan.id,
        "ticket_id": resolved_id,
        "action": "ticket_removed",
        "phase": removed_from_phase,
    }))
    .with_text(format!("Removed {} from plan {}", resolved_id, plan.id))
    .print(output_json)
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
    let plan = Plan::find(plan_id).await?;
    let mut metadata = plan.read()?;

    if !metadata.is_phased() {
        return Err(JanusError::CannotMoveInSimplePlan);
    }

    // Validate ticket exists
    let ticket = Ticket::find(ticket_id).await?;
    let resolved_id = ticket.id;

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
        let resolved_after = resolve_after_id(after_id, &target_phase.ticket_list.tickets)?;
        if !target_phase.add_ticket_after(&resolved_id, &resolved_after) {
            return Err(JanusError::TicketNotFound(TicketId::new_unchecked(
                after_id,
            )));
        }
    } else if let Some(pos) = position {
        target_phase.add_ticket_at_position(&resolved_id, pos);
    } else {
        target_phase.add_ticket(&resolved_id);
    }

    // Write updated plan
    plan.write_metadata(&metadata)?;

    // Log the event
    if let Some(from) = &found_in_phase {
        log_ticket_moved(&plan.id, &resolved_id, from, to_phase);
    }

    CommandOutput::new(json!({
        "plan_id": plan.id,
        "ticket_id": resolved_id,
        "action": "ticket_moved",
        "from_phase": found_in_phase,
        "to_phase": to_phase,
    }))
    .with_text(format!(
        "Moved {} to phase '{}' in plan {}",
        resolved_id, to_phase, plan.id
    ))
    .print(output_json)
}

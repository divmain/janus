//! Plan command implementations
//!
//! This module implements plan commands:
//! - `plan create` - Create a new plan
//! - `plan show` - Display a plan with full reconstruction
//! - `plan edit` - Open plan in $EDITOR
//! - `plan ls` - List all plans
//! - `plan add-ticket` - Add a ticket to a plan
//! - `plan remove-ticket` - Remove a ticket from a plan
//! - `plan move-ticket` - Move a ticket between phases
//! - `plan add-phase` - Add a new phase to a plan
//! - `plan remove-phase` - Remove a phase from a plan
//! - `plan reorder` - Reorder tickets or phases
//! - `plan delete` - Delete a plan
//! - `plan rename` - Rename a plan
//! - `plan next` - Show the next actionable item(s)
//! - `plan status` - Show plan status summary
//! - `plan import` - Import an AI-generated plan document
//! - `plan import-spec` - Show the importable plan format specification

use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;

use owo_colors::OwoColorize;
use serde_json::json;

use crate::error::{JanusError, Result};
use crate::plan::parser::serialize_plan;
use crate::plan::types::{Phase, PlanMetadata, PlanSection};
use crate::plan::{
    ImportablePlan, Plan, compute_all_phase_statuses, compute_plan_status, ensure_plans_dir,
    generate_plan_id, get_all_plans, parse_importable_plan,
};
use crate::ticket::{Ticket, build_ticket_map};
use crate::types::{TICKETS_ITEMS_DIR, TicketMetadata, TicketStatus, TicketType};
use crate::utils::{
    ensure_dir, generate_id_with_custom_prefix, generate_uuid, is_stdin_tty, iso_date,
    open_in_editor,
};

/// Create a new plan
///
/// # Arguments
/// * `title` - The plan title
/// * `phases` - Optional list of initial phase names (creates a phased plan if provided)
/// * `output_json` - If true, output result as JSON
pub fn cmd_plan_create(title: &str, phases: &[String], output_json: bool) -> Result<()> {
    ensure_plans_dir()?;

    let id = generate_plan_id();
    let uuid = generate_uuid();
    let now = iso_date();

    // Build the plan metadata
    let mut metadata = PlanMetadata {
        id: Some(id.clone()),
        uuid: Some(uuid.clone()),
        created: Some(now.clone()),
        title: Some(title.to_string()),
        description: None,
        acceptance_criteria: Vec::new(),
        sections: Vec::new(),
        file_path: None,
    };

    // Add phases if provided, otherwise create a simple plan with a Tickets section
    if phases.is_empty() {
        // Simple plan: add an empty Tickets section
        metadata.sections.push(PlanSection::Tickets(Vec::new()));
    } else {
        // Phased plan: add phases with numbers
        for (i, phase_name) in phases.iter().enumerate() {
            let phase = Phase::new((i + 1).to_string(), phase_name.clone());
            metadata.sections.push(PlanSection::Phase(phase));
        }
    }

    // Serialize and write the plan
    let content = serialize_plan(&metadata);
    let plan = Plan::with_id(&id);
    plan.write(&content)?;

    if output_json {
        let output = json!({
            "id": id,
            "uuid": uuid,
            "title": title,
            "created": now,
            "is_phased": !phases.is_empty(),
            "phases": phases,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", id);
    }
    Ok(())
}

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
    let plan = Plan::find(id)?;

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

    let ticket_map = build_ticket_map().await;

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
    let status_badge = format_status_badge(plan_status.status);
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
                    .map(|s| format_status_badge(s.status))
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

/// Open a plan in the default editor
///
/// # Arguments
/// * `id` - The plan ID (can be partial)
/// * `output_json` - If true, output result as JSON
pub fn cmd_plan_edit(id: &str, output_json: bool) -> Result<()> {
    let plan = Plan::find(id)?;

    if output_json {
        let output = json!({
            "id": plan.id,
            "file_path": plan.file_path.to_string_lossy(),
            "action": "edit",
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    if is_stdin_tty() {
        open_in_editor(&plan.file_path)?;
    } else {
        // Non-interactive mode: just print the file path
        println!("Edit plan file: {}", plan.file_path.display());
    }

    Ok(())
}

/// List all plans
///
/// # Arguments
/// * `status_filter` - Optional status to filter by
/// * `output_json` - If true, output as JSON
pub async fn cmd_plan_ls(status_filter: Option<&str>, output_json: bool) -> Result<()> {
    let plans = get_all_plans();
    let ticket_map = build_ticket_map().await;

    // Parse the status filter if provided
    let filter_status: Option<TicketStatus> = status_filter.and_then(|s| s.parse().ok());

    // Collect filtered plans with their statuses
    let mut filtered_plans: Vec<(&PlanMetadata, crate::plan::types::PlanStatus)> = Vec::new();

    for metadata in &plans {
        let plan_status = compute_plan_status(metadata, &ticket_map);

        // Apply status filter
        if let Some(ref filter) = filter_status
            && plan_status.status != *filter
        {
            continue;
        }

        filtered_plans.push((metadata, plan_status));
    }

    // Handle JSON output
    if output_json {
        use serde_json::json;

        let json_plans: Vec<serde_json::Value> = filtered_plans
            .iter()
            .map(|(metadata, plan_status)| {
                json!({
                    "id": metadata.id,
                    "uuid": metadata.uuid,
                    "title": metadata.title,
                    "created": metadata.created,
                    "status": plan_status.status.to_string(),
                    "completed_count": plan_status.completed_count,
                    "total_count": plan_status.total_count,
                    "progress_percent": plan_status.progress_percent(),
                    "is_phased": metadata.is_phased(),
                })
            })
            .collect();

        println!("{}", serde_json::to_string_pretty(&json_plans)?);
        return Ok(());
    }

    // Default text output
    for (metadata, plan_status) in &filtered_plans {
        let id = metadata.id.as_deref().unwrap_or("???");
        let title = metadata.title.as_deref().unwrap_or("");
        let status_badge = format_status_badge(plan_status.status);
        let progress = format!(
            "{}/{}",
            plan_status.completed_count, plan_status.total_count
        );

        println!(
            "{:12} {} {:>5}  {}",
            id.cyan(),
            status_badge,
            progress.dimmed(),
            title
        );
    }

    Ok(())
}

// ============================================================================
// Plan Manipulation Commands
// ============================================================================

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
        let output = json!({
            "plan_id": plan.id,
            "ticket_id": resolved_ticket_id,
            "action": "ticket_added",
            "phase": added_to_phase,
            "position": added_position,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
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

    // Try to resolve the ticket ID (but don't fail if ticket doesn't exist)
    let resolved_id = match Ticket::find_async(ticket_id).await {
        Ok(t) => t.id,
        Err(_) => ticket_id.to_string(), // Use as-is if ticket not found
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
        let output = json!({
            "plan_id": plan.id,
            "ticket_id": resolved_id,
            "action": "ticket_removed",
            "phase": removed_from_phase,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
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

    // Try to resolve the ticket ID
    let resolved_id = match Ticket::find_async(ticket_id).await {
        Ok(t) => t.id,
        Err(_) => ticket_id.to_string(),
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
        let output = json!({
            "plan_id": plan.id,
            "ticket_id": resolved_id,
            "action": "ticket_moved",
            "from_phase": found_in_phase,
            "to_phase": to_phase,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!(
            "Moved {} to phase '{}' in plan {}",
            resolved_id, to_phase, plan.id
        );
    }
    Ok(())
}

/// Add a new phase to a plan
///
/// # Arguments
/// * `plan_id` - The plan ID (can be partial)
/// * `phase_name` - Name for the new phase
/// * `after` - Optional phase name/number to insert after
/// * `position` - Optional position (1-indexed)
/// * `output_json` - If true, output result as JSON
pub fn cmd_plan_add_phase(
    plan_id: &str,
    phase_name: &str,
    after: Option<&str>,
    position: Option<usize>,
    output_json: bool,
) -> Result<()> {
    let plan = Plan::find(plan_id)?;
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

    if output_json {
        let output = json!({
            "plan_id": plan.id,
            "action": "phase_added",
            "phase_number": next_number.to_string(),
            "phase_name": phase_name,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!(
            "Added phase '{}' (Phase {}) to plan {}",
            phase_name, next_number, plan.id
        );
    }
    Ok(())
}

/// Remove a phase from a plan
///
/// # Arguments
/// * `plan_id` - The plan ID (can be partial)
/// * `phase` - Phase name or number to remove
/// * `force` - Force removal even if phase contains tickets
/// * `migrate` - Optional target phase to migrate tickets to
/// * `output_json` - If true, output result as JSON
pub fn cmd_plan_remove_phase(
    plan_id: &str,
    phase: &str,
    force: bool,
    migrate: Option<&str>,
    output_json: bool,
) -> Result<()> {
    let plan = Plan::find(plan_id)?;
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

    if output_json {
        let output = json!({
            "plan_id": plan.id,
            "action": "phase_removed",
            "phase_number": phase_number,
            "phase_name": phase_name,
            "migrated_tickets": migrated_tickets,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("Removed phase '{}' from plan {}", phase, plan.id);
    }
    Ok(())
}

/// Reorder tickets or phases interactively
///
/// # Arguments
/// * `plan_id` - The plan ID (can be partial)
/// * `phase` - Optional phase to reorder tickets within
/// * `reorder_phases` - If true, reorder phases instead of tickets
/// * `output_json` - If true, output result as JSON
pub fn cmd_plan_reorder(
    plan_id: &str,
    phase: Option<&str>,
    reorder_phases: bool,
    output_json: bool,
) -> Result<()> {
    let plan = Plan::find(plan_id)?;
    let mut metadata = plan.read()?;

    if reorder_phases {
        // Reorder phases
        let phases: Vec<(String, String)> = metadata
            .phases()
            .iter()
            .map(|p| (p.number.clone(), p.name.clone()))
            .collect();

        if phases.is_empty() {
            println!("No phases to reorder");
            return Ok(());
        }

        // Create a temp file with the current order
        let mut temp_content = String::new();
        for (num, name) in &phases {
            if name.is_empty() {
                temp_content.push_str(&format!("{}\n", num));
            } else {
                temp_content.push_str(&format!("{}: {}\n", num, name));
            }
        }

        // Open in editor
        let new_order = edit_in_editor(&temp_content)?;
        if new_order.trim() == temp_content.trim() {
            println!("No changes made");
            return Ok(());
        }

        // Parse new order
        let new_phase_order: Vec<String> = new_order
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                // Extract phase number (before colon or the whole line)
                l.split(':').next().unwrap_or(l).trim().to_string()
            })
            .collect();

        // Reorder sections based on new phase order
        let mut phase_sections: Vec<PlanSection> = Vec::new();
        let mut other_sections: Vec<(usize, PlanSection)> = Vec::new();

        for (i, section) in metadata.sections.drain(..).enumerate() {
            match &section {
                PlanSection::Phase(_) => phase_sections.push(section),
                _ => other_sections.push((i, section)),
            }
        }

        // Sort phase_sections according to new_phase_order
        let mut ordered_phases: Vec<PlanSection> = Vec::new();
        for phase_num in &new_phase_order {
            if let Some(idx) = phase_sections.iter().position(|s| {
                if let PlanSection::Phase(p) = s {
                    p.number.eq_ignore_ascii_case(phase_num)
                } else {
                    false
                }
            }) {
                ordered_phases.push(phase_sections.remove(idx));
            }
        }
        // Add any remaining phases that weren't in the new order
        ordered_phases.extend(phase_sections);

        // Rebuild sections maintaining relative positions of non-phase sections
        // This is a simplified approach - we put all non-phase sections back in roughly their original positions
        let mut phase_iter = ordered_phases.into_iter();
        let mut new_sections: Vec<PlanSection> = Vec::new();
        let mut other_iter = other_sections.into_iter().peekable();

        // Interleave based on original positions
        let original_len = new_sections.len();
        for _ in 0..original_len + phases.len() + other_iter.len() {
            if let Some(&(orig_idx, _)) = other_iter.peek()
                && orig_idx <= new_sections.len()
                && let Some((_, section)) = other_iter.next()
            {
                new_sections.push(section);
                continue;
            }
            if let Some(phase) = phase_iter.next() {
                new_sections.push(phase);
            }
        }
        // Push any remaining
        for (_, section) in other_iter {
            new_sections.push(section);
        }
        for phase in phase_iter {
            new_sections.push(phase);
        }

        metadata.sections = new_sections;
    } else if let Some(phase_identifier) = phase {
        // Reorder tickets within a specific phase
        let phase_obj = metadata
            .find_phase_mut(phase_identifier)
            .ok_or_else(|| JanusError::PhaseNotFound(phase_identifier.to_string()))?;

        if phase_obj.tickets.is_empty() {
            println!("No tickets to reorder in phase '{}'", phase_identifier);
            return Ok(());
        }

        // Create temp content with current order
        let temp_content: String = phase_obj
            .tickets
            .iter()
            .enumerate()
            .map(|(i, t)| format!("{}. {}\n", i + 1, t))
            .collect();

        // Open in editor
        let new_order = edit_in_editor(&temp_content)?;
        if new_order.trim() == temp_content.trim() {
            println!("No changes made");
            return Ok(());
        }

        // Parse new order - extract ticket IDs
        let new_ticket_order: Vec<String> = new_order
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| {
                // Extract ticket ID (after the number and dot)
                l.split('.')
                    .nth(1)
                    .map(|s| s.split_whitespace().next().unwrap_or("").to_string())
            })
            .filter(|s| !s.is_empty())
            .collect();

        // Validate all tickets are present
        let original_set: std::collections::HashSet<_> = phase_obj.tickets.iter().collect();
        let new_set: std::collections::HashSet<_> = new_ticket_order.iter().collect();
        if original_set != new_set {
            return Err(JanusError::Other(
                "Reordered list must contain the same tickets".to_string(),
            ));
        }

        phase_obj.tickets = new_ticket_order;
    } else if metadata.is_simple() {
        // Reorder tickets in simple plan
        let tickets = metadata
            .tickets_section_mut()
            .ok_or_else(|| JanusError::Other("Plan has no tickets section".to_string()))?;

        if tickets.is_empty() {
            println!("No tickets to reorder");
            return Ok(());
        }

        // Create temp content with current order
        let temp_content: String = tickets
            .iter()
            .enumerate()
            .map(|(i, t)| format!("{}. {}\n", i + 1, t))
            .collect();

        // Open in editor
        let new_order = edit_in_editor(&temp_content)?;
        if new_order.trim() == temp_content.trim() {
            println!("No changes made");
            return Ok(());
        }

        // Parse new order
        let new_ticket_order: Vec<String> = new_order
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| {
                l.split('.')
                    .nth(1)
                    .map(|s| s.split_whitespace().next().unwrap_or("").to_string())
            })
            .filter(|s| !s.is_empty())
            .collect();

        // Validate all tickets are present
        let original_set: std::collections::HashSet<_> = tickets.iter().collect();
        let new_set: std::collections::HashSet<_> = new_ticket_order.iter().collect();
        if original_set != new_set {
            return Err(JanusError::Other(
                "Reordered list must contain the same tickets".to_string(),
            ));
        }

        *tickets = new_ticket_order;
    } else {
        println!(
            "Use --phase to specify which phase to reorder, or --reorder-phases to reorder phases"
        );
        return Ok(());
    }

    // Write updated plan
    let content = serialize_plan(&metadata);
    plan.write(&content)?;

    if output_json {
        let output = json!({
            "plan_id": plan.id,
            "action": "reordered",
            "type": if reorder_phases { "phases" } else { "tickets" },
            "phase": phase,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("Reorder complete for plan {}", plan.id);
    }
    Ok(())
}

/// Delete a plan
///
/// # Arguments
/// * `id` - The plan ID (can be partial)
/// * `force` - Skip confirmation prompt
/// * `output_json` - If true, output result as JSON
pub fn cmd_plan_delete(id: &str, force: bool, output_json: bool) -> Result<()> {
    let plan = Plan::find(id)?;

    if !force && !output_json && is_stdin_tty() {
        // Prompt for confirmation
        print!("Delete plan {}? [y/N] ", plan.id);
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Cancelled");
            return Ok(());
        }
    }

    let plan_id = plan.id.clone();
    plan.delete()?;

    if output_json {
        let output = json!({
            "plan_id": plan_id,
            "action": "deleted",
            "success": true,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("Deleted plan {}", plan_id);
    }
    Ok(())
}

/// Rename a plan (update its title)
///
/// # Arguments
/// * `id` - The plan ID (can be partial)
/// * `new_title` - The new title
/// * `output_json` - If true, output result as JSON
pub fn cmd_plan_rename(id: &str, new_title: &str, output_json: bool) -> Result<()> {
    let plan = Plan::find(id)?;
    let mut metadata = plan.read()?;

    let old_title = metadata.title.clone().unwrap_or_default();
    metadata.title = Some(new_title.to_string());

    // Write updated plan
    let content = serialize_plan(&metadata);
    plan.write(&content)?;

    if output_json {
        let output = json!({
            "plan_id": plan.id,
            "action": "renamed",
            "old_title": old_title,
            "new_title": new_title,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!(
            "Renamed plan {} from '{}' to '{}'",
            plan.id, old_title, new_title
        );
    }
    Ok(())
}

// ============================================================================
// Next and Status Commands
// ============================================================================

/// Show the next actionable item(s) in a plan
///
/// # Arguments
/// * `id` - The plan ID (can be partial)
/// * `phase_only` - If true, show next item in current (first incomplete) phase only
/// * `all` - If true, show next item for each incomplete phase
/// * `count` - Number of next items to show
/// * `output_json` - If true, output result as JSON
pub async fn cmd_plan_next(
    id: &str,
    phase_only: bool,
    all: bool,
    count: usize,
    output_json: bool,
) -> Result<()> {
    let plan = Plan::find(id)?;
    let metadata = plan.read()?;
    let ticket_map = build_ticket_map().await;

    // Collect next items based on options
    let next_items = if metadata.is_phased() {
        get_next_items_phased(&metadata, &ticket_map, phase_only, all, count)
    } else {
        get_next_items_simple(&metadata, &ticket_map, count)
    };

    if output_json {
        let next_items_json: Vec<_> = next_items
            .iter()
            .map(|item| {
                let tickets_json: Vec<_> = item
                    .tickets
                    .iter()
                    .map(|(ticket_id, ticket_meta)| {
                        json!({
                            "id": ticket_id,
                            "title": ticket_meta.as_ref().and_then(|t| t.title.clone()),
                            "status": ticket_meta.as_ref().and_then(|t| t.status).map(|s| s.to_string()),
                            "priority": ticket_meta.as_ref().and_then(|t| t.priority).map(|p| p.as_num()),
                            "deps": ticket_meta.as_ref().map(|t| &t.deps).cloned().unwrap_or_default(),
                            "exists": ticket_meta.is_some(),
                        })
                    })
                    .collect();

                json!({
                    "phase_number": item.phase_number,
                    "phase_name": item.phase_name,
                    "tickets": tickets_json,
                })
            })
            .collect();

        let output = json!({
            "plan_id": plan.id,
            "next_items": next_items_json,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    if next_items.is_empty() {
        println!("No actionable items remaining");
        return Ok(());
    }

    // Print next items
    for item in &next_items {
        println!(
            "{}",
            format!("## Next: Phase {} - {}", item.phase_number, item.phase_name).bold()
        );
        println!();

        for (i, (ticket_id, ticket_meta)) in item.tickets.iter().enumerate() {
            let status = ticket_meta
                .as_ref()
                .and_then(|t| t.status)
                .unwrap_or_default();
            let status_badge = format_status_badge(status);
            let title = ticket_meta
                .as_ref()
                .and_then(|t| t.title.as_deref())
                .unwrap_or("");

            println!("{} {} {}", status_badge, ticket_id.cyan(), title);

            // Show priority and deps if available
            if let Some(meta) = ticket_meta {
                let priority = meta.priority.map(|p| p.as_num()).unwrap_or(2);
                println!("  Priority: P{}", priority);

                // Show dependencies with their status
                if !meta.deps.is_empty() {
                    let deps_with_status: Vec<String> = meta
                        .deps
                        .iter()
                        .map(|dep| {
                            let dep_status = ticket_map
                                .get(dep)
                                .and_then(|t| t.status)
                                .map(|s| format!("[{}]", s))
                                .unwrap_or_else(|| "[missing]".to_string());
                            format!("{} {}", dep, dep_status)
                        })
                        .collect();
                    println!("  Deps: {}", deps_with_status.join(", "));
                }
            }

            if i < item.tickets.len() - 1 {
                println!();
            }
        }
        println!();
    }

    Ok(())
}

/// Show plan status summary
///
/// # Arguments
/// * `id` - The plan ID (can be partial)
/// * `output_json` - If true, output result as JSON
pub async fn cmd_plan_status(id: &str, output_json: bool) -> Result<()> {
    let plan = Plan::find(id)?;
    let metadata = plan.read()?;
    let ticket_map = build_ticket_map().await;

    // Compute overall plan status
    let plan_status = compute_plan_status(&metadata, &ticket_map);

    if output_json {
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
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    // Print header
    let title = metadata.title.as_deref().unwrap_or("Untitled");
    let plan_id = metadata.id.as_deref().unwrap_or(&plan.id);
    println!("Plan: {} - {}", plan_id.cyan(), title);
    println!("Status: {}", format_status_badge(plan_status.status));
    println!("Progress: {} tickets", plan_status.progress_string());

    // If phased, show breakdown by phase
    if metadata.is_phased() {
        let phase_statuses = compute_all_phase_statuses(&metadata, &ticket_map);

        if !phase_statuses.is_empty() {
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
                let status_badge = format_status_badge(ps.status);
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
    }

    Ok(())
}

/// Helper struct for next item results
struct NextItemResult {
    phase_number: String,
    phase_name: String,
    tickets: Vec<(String, Option<TicketMetadata>)>,
}

/// Get next actionable items for a phased plan
fn get_next_items_phased(
    metadata: &PlanMetadata,
    ticket_map: &std::collections::HashMap<String, TicketMetadata>,
    phase_only: bool,
    all: bool,
    count: usize,
) -> Vec<NextItemResult> {
    let phases = metadata.phases();
    let mut results = Vec::new();

    for phase in &phases {
        // Compute phase status
        let phase_status = compute_phase_status_for_phase(phase, ticket_map);

        // Skip completed/cancelled phases
        if phase_status.status == TicketStatus::Complete
            || phase_status.status == TicketStatus::Cancelled
        {
            continue;
        }

        // Find next actionable tickets in this phase
        let mut next_tickets = Vec::new();
        for ticket_id in &phase.tickets {
            let ticket_meta = ticket_map.get(ticket_id).cloned();
            let status = ticket_meta
                .as_ref()
                .and_then(|t| t.status)
                .unwrap_or(TicketStatus::New);

            // Skip completed/cancelled tickets
            if status == TicketStatus::Complete || status == TicketStatus::Cancelled {
                continue;
            }

            next_tickets.push((ticket_id.clone(), ticket_meta));

            // Limit by count unless showing all
            if !all && next_tickets.len() >= count {
                break;
            }
        }

        if !next_tickets.is_empty() {
            // If showing limited count, truncate
            if !all && next_tickets.len() > count {
                next_tickets.truncate(count);
            }

            results.push(NextItemResult {
                phase_number: phase.number.clone(),
                phase_name: phase.name.clone(),
                tickets: next_tickets,
            });

            // If phase_only or not all, just return the first incomplete phase
            if phase_only || !all {
                break;
            }
        }
    }

    results
}

/// Get next actionable items for a simple plan
fn get_next_items_simple(
    metadata: &PlanMetadata,
    ticket_map: &std::collections::HashMap<String, TicketMetadata>,
    count: usize,
) -> Vec<NextItemResult> {
    let tickets = match metadata.tickets_section() {
        Some(t) => t,
        None => return Vec::new(),
    };

    let mut next_tickets = Vec::new();
    for ticket_id in tickets {
        let ticket_meta = ticket_map.get(ticket_id).cloned();
        let status = ticket_meta
            .as_ref()
            .and_then(|t| t.status)
            .unwrap_or(TicketStatus::New);

        // Skip completed/cancelled tickets
        if status == TicketStatus::Complete || status == TicketStatus::Cancelled {
            continue;
        }

        next_tickets.push((ticket_id.clone(), ticket_meta));

        if next_tickets.len() >= count {
            break;
        }
    }

    if next_tickets.is_empty() {
        return Vec::new();
    }

    vec![NextItemResult {
        phase_number: String::new(),
        phase_name: "Tickets".to_string(),
        tickets: next_tickets,
    }]
}

/// Compute phase status for a single phase (helper that doesn't require Plan)
fn compute_phase_status_for_phase(
    phase: &Phase,
    ticket_map: &std::collections::HashMap<String, TicketMetadata>,
) -> crate::plan::types::PhaseStatus {
    let total_count = phase.tickets.len();

    if total_count == 0 {
        return crate::plan::types::PhaseStatus {
            phase_number: phase.number.clone(),
            phase_name: phase.name.clone(),
            status: TicketStatus::New,
            completed_count: 0,
            total_count: 0,
        };
    }

    // Collect statuses of all referenced tickets (skipping missing ones)
    let statuses: Vec<TicketStatus> = phase
        .tickets
        .iter()
        .filter_map(|id| ticket_map.get(id))
        .filter_map(|t| t.status)
        .collect();

    let completed_count = statuses
        .iter()
        .filter(|s| **s == TicketStatus::Complete)
        .count();

    let status = compute_aggregate_status_local(&statuses);

    crate::plan::types::PhaseStatus {
        phase_number: phase.number.clone(),
        phase_name: phase.name.clone(),
        status,
        completed_count,
        total_count,
    }
}

/// Compute aggregate status from a list of ticket statuses (local helper)
fn compute_aggregate_status_local(statuses: &[TicketStatus]) -> TicketStatus {
    if statuses.is_empty() {
        return TicketStatus::New;
    }

    let all_complete = statuses.iter().all(|s| *s == TicketStatus::Complete);
    let all_cancelled = statuses.iter().all(|s| *s == TicketStatus::Cancelled);
    let all_finished = statuses
        .iter()
        .all(|s| *s == TicketStatus::Complete || *s == TicketStatus::Cancelled);
    let all_not_started = statuses
        .iter()
        .all(|s| *s == TicketStatus::New || *s == TicketStatus::Next);

    if all_complete {
        TicketStatus::Complete
    } else if all_cancelled {
        TicketStatus::Cancelled
    } else if all_finished {
        TicketStatus::Complete
    } else if all_not_started {
        TicketStatus::New
    } else {
        TicketStatus::InProgress
    }
}

// ============================================================================
// Output Format Helper Functions
// ============================================================================

/// Show plan as JSON
fn show_plan_json(
    metadata: &PlanMetadata,
    ticket_map: &std::collections::HashMap<String, TicketMetadata>,
) -> Result<()> {
    use serde_json::json;

    let plan_status = compute_plan_status(metadata, ticket_map);
    let phase_statuses = compute_all_phase_statuses(metadata, ticket_map);

    let tickets_info: Vec<serde_json::Value> = metadata
        .all_tickets()
        .iter()
        .map(|tid| {
            let ticket = ticket_map.get(*tid);
            json!({
                "id": tid,
                "status": ticket.and_then(|t| t.status).map(|s| s.to_string()),
                "title": ticket.and_then(|t| t.title.clone()),
                "exists": ticket.is_some(),
            })
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
                    json!({
                        "id": tid,
                        "status": ticket.and_then(|t| t.status).map(|s| s.to_string()),
                        "title": ticket.and_then(|t| t.title.clone()),
                        "exists": ticket.is_some(),
                    })
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

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

/// Show only the ticket list with statuses
fn show_tickets_only(
    metadata: &PlanMetadata,
    ticket_map: &std::collections::HashMap<String, TicketMetadata>,
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
    ticket_map: &std::collections::HashMap<String, TicketMetadata>,
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
        let status_badge = format_status_badge(ps.status);
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

// ============================================================================
// Helper Functions
// ============================================================================

/// Open content in an editor and return the edited content
fn edit_in_editor(content: &str) -> Result<String> {
    if !is_stdin_tty() {
        return Err(JanusError::Other(
            "Reorder requires an interactive terminal".to_string(),
        ));
    }

    // Create a temp file
    let mut temp_file = tempfile::NamedTempFile::new()?;
    temp_file.write_all(content.as_bytes())?;
    temp_file.flush()?;

    let temp_path = temp_file.path().to_path_buf();

    // Open in editor
    open_in_editor(&temp_path)?;

    // Read the edited content
    let mut edited = String::new();
    std::fs::File::open(&temp_path)?.read_to_string(&mut edited)?;

    Ok(edited)
}

/// Format a status as a colored badge
fn format_status_badge(status: TicketStatus) -> String {
    let badge = format!("[{}]", status);
    match status {
        TicketStatus::New => badge.yellow().to_string(),
        TicketStatus::Next => badge.magenta().to_string(),
        TicketStatus::InProgress => badge.cyan().to_string(),
        TicketStatus::Complete => badge.green().to_string(),
        TicketStatus::Cancelled => badge.dimmed().to_string(),
    }
}

/// Print a ticket line with status for plan show command
///
/// # Arguments
/// * `index` - The 1-based index of the ticket in the list
/// * `ticket_id` - The ticket ID
/// * `ticket_map` - Map of ticket IDs to metadata
/// * `full_summary` - If true, show full completion summary; if false, show only first 2 lines
fn print_ticket_line(
    index: usize,
    ticket_id: &str,
    ticket_map: &std::collections::HashMap<String, crate::types::TicketMetadata>,
    full_summary: bool,
) {
    if let Some(ticket) = ticket_map.get(ticket_id) {
        let status = ticket.status.unwrap_or_default();
        let status_badge = format_status_badge(status);
        let title = ticket.title.as_deref().unwrap_or("");

        println!(
            "{}. {} {} - {}",
            index,
            status_badge,
            ticket_id.cyan(),
            title
        );

        // Print completion summary if complete and has one
        if status == TicketStatus::Complete
            && let Some(ref summary) = ticket.completion_summary
        {
            // Print as indented blockquote
            if full_summary {
                // Print all lines
                for line in summary.lines() {
                    println!("   > {}", line.dimmed());
                }
            } else {
                // Print only first 2 lines
                for line in summary.lines().take(2) {
                    println!("   > {}", line.dimmed());
                }
            }
        }
    } else {
        // Missing ticket
        println!("{}. {} {}", index, "[missing]".red(), ticket_id.dimmed());
    }
}

// ============================================================================
// Plan Import Commands
// ============================================================================

/// The Plan Format Specification document.
///
/// This constant contains the full documentation for the importable plan format.
/// It is displayed by `janus plan import-spec`.
pub const PLAN_FORMAT_SPECIFICATION: &str = r#"# Plan Format Specification

This document describes the format for plan documents that can be imported
into Janus using `janus plan import`.

## Basic Structure

```markdown
# Plan Title (required)

Introductory paragraph(s) providing a description of the overall plan.

## Design

Comprehensive description of the desired end-state when the multi-phase plan
is complete. This section should contain multiple sections breaking down the
design, key technical decisions, architecture, reasoning behind the design,
and the final acceptance criteria for the entire plan.

## Acceptance Criteria (optional)

- First criterion
- Second criterion

## Implementation

### Phase 1: Phase Name

Multi-paragraph description of what should be accomplished in Phase 1.

#### The Title of the First Task in Phase One

The first task's description, implementation notes, or code examples. Required.
Must be comprehensive -- bullet points are acceptable, as are multiple paragraphs.
Must include code samples if required for clarity. Must include acceptance
criteria for the task.

#### The Title of the Second Task in Phase One

The second task's description. All task descriptions must be comprehensive.

### Phase 2: Another Phase Name

#### The Title of the First Task in Phase Two

Task description.
```

## Required Sections

The following sections are **required**:

1. **`# Plan Title`** (H1) - The plan title, must be first heading
2. **`## Design`** (H2) - Design details, architecture, and reasoning
3. **`## Implementation`** (H2) - Contains all phase definitions

## Optional Sections

- **`## Acceptance Criteria`** (H2) - If present, creates a verification ticket

## Element Reference

| Element             | Format                      | Notes                                       |
|---------------------|-----------------------------|---------------------------------------------|
| Plan title          | `# Title` (H1)              | Required, must be first heading             |
| Description         | Paragraphs after H1         | Optional, before first H2                   |
| Design              | `## Design`                 | Required, contains design details           |
| Acceptance criteria | `## Acceptance Criteria`    | Optional, creates verification ticket       |
| Implementation      | `## Implementation`         | Required, contains all phases               |
| Phase               | `### Phase N: Name`         | Under Implementation; also: Stage N, etc.   |
| Task                | `#### Task Title`           | Under a phase, becomes ticket title         |
| Completed task      | `#### Title [x]`            | Created with status: complete               |
| Task body           | Content after H4            | Becomes ticket description                  |

## Phase Numbering

Phase numbers can be:
- Numeric: `### Phase 1:`, `### Phase 2:`
- Alphanumeric: `### Phase 1a:`, `### Phase 2b:`
- Keywords: Phase, Stage, Part, Step (followed by number and optional name)

## Task Content

Content between an H4 task header and the next H4/H3 becomes the ticket body:

```markdown
#### Add Caching Support

Implement caching in the TTS service to avoid redundant synthesis.

Key changes:
- Add cache data structure
- Modify speak() method

**Acceptance Criteria:**
- Cache hits return in <5ms
- Cache invalidation works correctly

#### Next Task
```

The above creates a ticket titled "Add Caching Support" with the description
containing all the prose, bullet points, and acceptance criteria.

## Examples

See `janus plan import --dry-run <file>` to preview what would be created.
"#;

/// Show the importable plan format specification
///
/// Prints the Plan Format Specification document to stdout.
pub fn cmd_show_import_spec() -> Result<()> {
    println!("{}", PLAN_FORMAT_SPECIFICATION);
    Ok(())
}

/// Check if a plan with the given title already exists
///
/// # Arguments
/// * `title` - The title to check
///
/// # Returns
/// `Ok(())` if no duplicate exists, `Err(DuplicatePlanTitle)` if one does.
fn check_duplicate_plan_title(title: &str) -> Result<()> {
    let existing_plans = get_all_plans();

    for plan in existing_plans {
        if let Some(ref existing_title) = plan.title
            && existing_title.eq_ignore_ascii_case(title)
        {
            let plan_id = plan.id.unwrap_or_else(|| "unknown".to_string());
            return Err(JanusError::DuplicatePlanTitle(title.to_string(), plan_id));
        }
    }

    Ok(())
}

/// Format and print the dry-run import summary
///
/// # Arguments
/// * `plan` - The parsed importable plan
fn print_import_summary(plan: &ImportablePlan) {
    println!();
    println!("{}", "Import Summary".bold());
    println!("{}", "==============".bold());
    println!();

    // Title
    println!("{}: {}", "Title".bold(), plan.title);

    // Description (truncated if long)
    if let Some(ref desc) = plan.description {
        let desc_preview = if desc.len() > 200 {
            format!("{}...", &desc[..200])
        } else {
            desc.clone()
        };
        println!("{}: {}", "Description".bold(), desc_preview);
    }

    // Acceptance criteria
    if !plan.acceptance_criteria.is_empty() {
        println!();
        println!(
            "{}: {} items",
            "Acceptance Criteria".bold(),
            plan.acceptance_criteria.len()
        );
        for criterion in &plan.acceptance_criteria {
            println!("  - {}", criterion);
        }
    }

    // Plan structure
    println!();
    println!("{}: {}", "Phases".bold(), plan.phases.len());
    println!("{}: {}", "Tasks".bold(), plan.task_count());
    println!();

    for phase in &plan.phases {
        let phase_header = if phase.name.is_empty() {
            format!("Phase {}", phase.number)
        } else {
            format!("Phase {}: {}", phase.number, phase.name)
        };
        println!("{}", phase_header.cyan());

        for task in &phase.tasks {
            let marker = if task.is_complete { "[x]" } else { "[ ]" };
            println!("  {} {}", marker.dimmed(), task.title);
        }
    }

    // Summary of what would be created
    println!();
    println!("{}", "Would create:".bold());
    println!("  - 1 plan");

    let new_count = plan.all_tasks().iter().filter(|t| !t.is_complete).count();
    let complete_count = plan.all_tasks().iter().filter(|t| t.is_complete).count();

    if complete_count > 0 {
        println!(
            "  - {} tickets ({} new, {} complete)",
            plan.task_count(),
            new_count,
            complete_count
        );
    } else {
        println!("  - {} tickets (status: new)", plan.task_count());
    }

    if !plan.acceptance_criteria.is_empty() {
        println!("  - 1 verification ticket (from acceptance criteria)");
    }

    println!();
    println!("Run without --dry-run to import.");
}

/// Create a ticket from an ImportableTask
///
/// Returns (ticket_id, file_path) on success.
fn create_ticket_from_task(
    task: &crate::plan::ImportableTask,
    ticket_type: TicketType,
    prefix: Option<&str>,
) -> Result<String> {
    ensure_dir()?;

    let id = generate_id_with_custom_prefix(prefix)?;
    let uuid = generate_uuid();
    let now = iso_date();

    let status = if task.is_complete { "complete" } else { "new" };

    // Build frontmatter
    let frontmatter_lines = vec![
        "---".to_string(),
        format!("id: {}", id),
        format!("uuid: {}", uuid),
        format!("status: {}", status),
        "deps: []".to_string(),
        "links: []".to_string(),
        format!("created: {}", now),
        format!("type: {}", ticket_type),
        "priority: 2".to_string(),
        "---".to_string(),
    ];

    let frontmatter = frontmatter_lines.join("\n");

    // Build body
    let mut body = format!("# {}", task.title);
    if let Some(ref task_body) = task.body {
        body.push_str("\n\n");
        body.push_str(task_body);
    }

    let content = format!("{}\n{}\n", frontmatter, body);

    let file_path = PathBuf::from(TICKETS_ITEMS_DIR).join(format!("{}.md", id));
    fs::create_dir_all(TICKETS_ITEMS_DIR)?;
    fs::write(&file_path, content)?;

    Ok(id)
}

/// Create a verification ticket for acceptance criteria
fn create_verification_ticket(
    criteria: &[String],
    ticket_type: TicketType,
    prefix: Option<&str>,
) -> Result<String> {
    ensure_dir()?;

    let id = generate_id_with_custom_prefix(prefix)?;
    let uuid = generate_uuid();
    let now = iso_date();

    // Build frontmatter
    let frontmatter_lines = vec![
        "---".to_string(),
        format!("id: {}", id),
        format!("uuid: {}", uuid),
        "status: new".to_string(),
        "deps: []".to_string(),
        "links: []".to_string(),
        format!("created: {}", now),
        format!("type: {}", ticket_type),
        "priority: 2".to_string(),
        "---".to_string(),
    ];

    let frontmatter = frontmatter_lines.join("\n");

    // Build body with acceptance criteria checklist
    let mut body = "# Verify Acceptance Criteria\n\n".to_string();
    body.push_str("Verify that all acceptance criteria have been met:\n\n");
    for criterion in criteria {
        body.push_str(&format!("- [ ] {}\n", criterion));
    }

    let content = format!("{}\n{}\n", frontmatter, body);

    let file_path = PathBuf::from(TICKETS_ITEMS_DIR).join(format!("{}.md", id));
    fs::create_dir_all(TICKETS_ITEMS_DIR)?;
    fs::write(&file_path, content)?;

    Ok(id)
}

/// Import a plan from a markdown file
///
/// # Arguments
/// * `input` - File path or "-" for stdin
/// * `dry_run` - If true, validate and show summary without creating anything
/// * `title_override` - Override the extracted title
/// * `ticket_type` - Type for created tickets (default: task)
/// * `prefix` - Custom prefix for ticket IDs
/// * `output_json` - If true, output result as JSON
pub fn cmd_plan_import(
    input: &str,
    dry_run: bool,
    title_override: Option<&str>,
    ticket_type: TicketType,
    prefix: Option<&str>,
    output_json: bool,
) -> Result<()> {
    // 1. Read content from file or stdin
    let content = if input == "-" {
        let mut buffer = String::new();
        std::io::stdin().read_to_string(&mut buffer)?;
        buffer
    } else {
        fs::read_to_string(input)?
    };

    // 2. Parse the importable plan
    let mut plan = parse_importable_plan(&content)?;

    // 3. Apply title override if provided
    if let Some(title) = title_override {
        plan.title = title.to_string();
    }

    // 4. Check for duplicate plan title
    check_duplicate_plan_title(&plan.title)?;

    // 5. If dry-run, print summary and return
    if dry_run {
        if output_json {
            let json_output = json!({
                "dry_run": true,
                "title": plan.title,
                "description": plan.description,
                "acceptance_criteria_count": plan.acceptance_criteria.len(),
                "is_phased": plan.is_phased(),
                "phase_count": plan.phases.len(),
                "task_count": plan.task_count(),
                "would_create": {
                    "plans": 1,
                    "tickets": plan.task_count() + if !plan.acceptance_criteria.is_empty() { 1 } else { 0 },
                }
            });
            println!("{}", serde_json::to_string_pretty(&json_output)?);
        } else {
            print_import_summary(&plan);
        }
        return Ok(());
    }

    // 6. Create all tickets
    ensure_plans_dir()?;

    let mut created_ticket_ids: Vec<String> = Vec::new();

    // Create tickets for each phase
    for phase in &plan.phases {
        for task in &phase.tasks {
            let ticket_id = create_ticket_from_task(task, ticket_type, prefix)?;
            created_ticket_ids.push(ticket_id);
        }
    }

    // 7. Create verification ticket if acceptance criteria exist
    let verification_ticket_id = if !plan.acceptance_criteria.is_empty() {
        Some(create_verification_ticket(
            &plan.acceptance_criteria,
            ticket_type,
            prefix,
        )?)
    } else {
        None
    };

    // 8. Generate plan metadata
    let plan_id = generate_plan_id();
    let uuid = generate_uuid();
    let now = iso_date();

    let mut metadata = PlanMetadata {
        id: Some(plan_id.clone()),
        uuid: Some(uuid.clone()),
        created: Some(now.clone()),
        title: Some(plan.title.clone()),
        description: plan.description.clone(),
        acceptance_criteria: plan.acceptance_criteria.clone(),
        sections: Vec::new(),
        file_path: None,
    };

    // 9. Build sections with ticket IDs
    let mut ticket_idx = 0;
    for import_phase in &plan.phases {
        let mut phase = Phase::new(import_phase.number.clone(), import_phase.name.clone());
        phase.description = import_phase.description.clone();

        // Assign ticket IDs to this phase
        for _ in &import_phase.tasks {
            phase.tickets.push(created_ticket_ids[ticket_idx].clone());
            ticket_idx += 1;
        }

        // Add verification ticket to the last phase if it exists
        let is_last_phase = plan
            .phases
            .last()
            .map(|p| p.number == import_phase.number)
            .unwrap_or(false);

        if is_last_phase && let Some(ref v_id) = verification_ticket_id {
            phase.tickets.push(v_id.clone());
        }

        metadata.sections.push(PlanSection::Phase(phase));
    }

    // 10. Serialize and write plan
    let plan_content = serialize_plan(&metadata);
    let plan_handle = Plan::with_id(&plan_id);
    plan_handle.write(&plan_content)?;

    // 11. Output result
    if output_json {
        let tickets_created: Vec<serde_json::Value> = created_ticket_ids
            .iter()
            .map(|id| json!({ "id": id }))
            .collect();

        let output = json!({
            "id": plan_id,
            "uuid": uuid,
            "title": plan.title,
            "created": now,
            "is_phased": plan.is_phased(),
            "tickets_created": tickets_created,
            "verification_ticket": verification_ticket_id,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", plan_id);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_format_status_badge() {
        // Just verify it doesn't panic for all statuses
        let statuses = [
            TicketStatus::New,
            TicketStatus::Next,
            TicketStatus::InProgress,
            TicketStatus::Complete,
            TicketStatus::Cancelled,
        ];

        for status in statuses {
            let badge = format_status_badge(status);
            assert!(badge.contains(&status.to_string()));
        }
    }

    // Helper function to create test ticket metadata
    fn make_ticket(id: &str, status: TicketStatus) -> TicketMetadata {
        TicketMetadata {
            id: Some(id.to_string()),
            status: Some(status),
            title: Some(format!("Title for {}", id)),
            ..Default::default()
        }
    }

    // Helper function to create a simple plan with tickets
    fn make_simple_plan(tickets: Vec<&str>) -> PlanMetadata {
        let mut metadata = PlanMetadata::default();
        metadata.sections.push(PlanSection::Tickets(
            tickets.iter().map(|s| s.to_string()).collect(),
        ));
        metadata
    }

    // Helper function to create a phased plan
    fn make_phased_plan(phases: Vec<(&str, &str, Vec<&str>)>) -> PlanMetadata {
        let mut metadata = PlanMetadata::default();
        for (number, name, tickets) in phases {
            let phase = Phase {
                number: number.to_string(),
                name: name.to_string(),
                description: None,
                success_criteria: vec![],
                tickets: tickets.iter().map(|s| s.to_string()).collect(),
            };
            metadata.sections.push(PlanSection::Phase(phase));
        }
        metadata
    }

    #[test]
    fn test_get_next_items_simple_empty_plan() {
        let metadata = make_simple_plan(vec![]);
        let ticket_map = HashMap::new();

        let results = get_next_items_simple(&metadata, &ticket_map, 1);
        assert!(results.is_empty());
    }

    #[test]
    fn test_get_next_items_simple_one_new_ticket() {
        let metadata = make_simple_plan(vec!["t1"]);
        let mut ticket_map = HashMap::new();
        ticket_map.insert("t1".to_string(), make_ticket("t1", TicketStatus::New));

        let results = get_next_items_simple(&metadata, &ticket_map, 1);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tickets.len(), 1);
        assert_eq!(results[0].tickets[0].0, "t1");
    }

    #[test]
    fn test_get_next_items_simple_skips_complete() {
        let metadata = make_simple_plan(vec!["t1", "t2", "t3"]);
        let mut ticket_map = HashMap::new();
        ticket_map.insert("t1".to_string(), make_ticket("t1", TicketStatus::Complete));
        ticket_map.insert("t2".to_string(), make_ticket("t2", TicketStatus::New));
        ticket_map.insert("t3".to_string(), make_ticket("t3", TicketStatus::New));

        let results = get_next_items_simple(&metadata, &ticket_map, 1);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tickets.len(), 1);
        assert_eq!(results[0].tickets[0].0, "t2");
    }

    #[test]
    fn test_get_next_items_simple_respects_count() {
        let metadata = make_simple_plan(vec!["t1", "t2", "t3"]);
        let mut ticket_map = HashMap::new();
        ticket_map.insert("t1".to_string(), make_ticket("t1", TicketStatus::New));
        ticket_map.insert("t2".to_string(), make_ticket("t2", TicketStatus::New));
        ticket_map.insert("t3".to_string(), make_ticket("t3", TicketStatus::New));

        let results = get_next_items_simple(&metadata, &ticket_map, 2);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tickets.len(), 2);
    }

    #[test]
    fn test_get_next_items_simple_all_complete() {
        let metadata = make_simple_plan(vec!["t1", "t2"]);
        let mut ticket_map = HashMap::new();
        ticket_map.insert("t1".to_string(), make_ticket("t1", TicketStatus::Complete));
        ticket_map.insert("t2".to_string(), make_ticket("t2", TicketStatus::Complete));

        let results = get_next_items_simple(&metadata, &ticket_map, 1);
        assert!(results.is_empty());
    }

    #[test]
    fn test_get_next_items_phased_first_incomplete_phase() {
        let metadata = make_phased_plan(vec![
            ("1", "Phase One", vec!["t1", "t2"]),
            ("2", "Phase Two", vec!["t3", "t4"]),
        ]);
        let mut ticket_map = HashMap::new();
        ticket_map.insert("t1".to_string(), make_ticket("t1", TicketStatus::Complete));
        ticket_map.insert("t2".to_string(), make_ticket("t2", TicketStatus::Complete));
        ticket_map.insert("t3".to_string(), make_ticket("t3", TicketStatus::New));
        ticket_map.insert("t4".to_string(), make_ticket("t4", TicketStatus::New));

        let results = get_next_items_phased(&metadata, &ticket_map, false, false, 1);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].phase_number, "2");
        assert_eq!(results[0].phase_name, "Phase Two");
        assert_eq!(results[0].tickets.len(), 1);
        assert_eq!(results[0].tickets[0].0, "t3");
    }

    #[test]
    fn test_get_next_items_phased_all_phases() {
        let metadata = make_phased_plan(vec![
            ("1", "Phase One", vec!["t1", "t2"]),
            ("2", "Phase Two", vec!["t3", "t4"]),
        ]);
        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "t1".to_string(),
            make_ticket("t1", TicketStatus::InProgress),
        );
        ticket_map.insert("t2".to_string(), make_ticket("t2", TicketStatus::New));
        ticket_map.insert("t3".to_string(), make_ticket("t3", TicketStatus::New));
        ticket_map.insert("t4".to_string(), make_ticket("t4", TicketStatus::New));

        // With all=true, should get results from all incomplete phases
        let results = get_next_items_phased(&metadata, &ticket_map, false, true, 1);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].phase_number, "1");
        assert_eq!(results[1].phase_number, "2");
    }

    #[test]
    fn test_get_next_items_phased_skips_complete_phases() {
        let metadata = make_phased_plan(vec![
            ("1", "Phase One", vec!["t1"]),
            ("2", "Phase Two", vec!["t2"]),
        ]);
        let mut ticket_map = HashMap::new();
        ticket_map.insert("t1".to_string(), make_ticket("t1", TicketStatus::Complete));
        ticket_map.insert("t2".to_string(), make_ticket("t2", TicketStatus::New));

        let results = get_next_items_phased(&metadata, &ticket_map, false, false, 1);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].phase_number, "2");
    }

    #[test]
    fn test_get_next_items_phased_all_complete() {
        let metadata = make_phased_plan(vec![
            ("1", "Phase One", vec!["t1"]),
            ("2", "Phase Two", vec!["t2"]),
        ]);
        let mut ticket_map = HashMap::new();
        ticket_map.insert("t1".to_string(), make_ticket("t1", TicketStatus::Complete));
        ticket_map.insert("t2".to_string(), make_ticket("t2", TicketStatus::Complete));

        let results = get_next_items_phased(&metadata, &ticket_map, false, false, 1);
        assert!(results.is_empty());
    }

    #[test]
    fn test_compute_phase_status_for_phase_empty() {
        let phase = Phase::new("1", "Empty");
        let ticket_map = HashMap::new();

        let status = compute_phase_status_for_phase(&phase, &ticket_map);
        assert_eq!(status.status, TicketStatus::New);
        assert_eq!(status.completed_count, 0);
        assert_eq!(status.total_count, 0);
    }

    #[test]
    fn test_compute_phase_status_for_phase_mixed() {
        let mut phase = Phase::new("1", "Mixed");
        phase.tickets = vec!["t1".to_string(), "t2".to_string()];

        let mut ticket_map = HashMap::new();
        ticket_map.insert("t1".to_string(), make_ticket("t1", TicketStatus::Complete));
        ticket_map.insert("t2".to_string(), make_ticket("t2", TicketStatus::New));

        let status = compute_phase_status_for_phase(&phase, &ticket_map);
        assert_eq!(status.status, TicketStatus::InProgress);
        assert_eq!(status.completed_count, 1);
        assert_eq!(status.total_count, 2);
    }

    #[test]
    fn test_compute_aggregate_status_local() {
        // All complete
        let statuses = vec![TicketStatus::Complete, TicketStatus::Complete];
        assert_eq!(
            compute_aggregate_status_local(&statuses),
            TicketStatus::Complete
        );

        // All cancelled
        let statuses = vec![TicketStatus::Cancelled, TicketStatus::Cancelled];
        assert_eq!(
            compute_aggregate_status_local(&statuses),
            TicketStatus::Cancelled
        );

        // Mixed finished
        let statuses = vec![TicketStatus::Complete, TicketStatus::Cancelled];
        assert_eq!(
            compute_aggregate_status_local(&statuses),
            TicketStatus::Complete
        );

        // All new
        let statuses = vec![TicketStatus::New, TicketStatus::New];
        assert_eq!(compute_aggregate_status_local(&statuses), TicketStatus::New);

        // All next
        let statuses = vec![TicketStatus::Next, TicketStatus::Next];
        assert_eq!(compute_aggregate_status_local(&statuses), TicketStatus::New);

        // In progress
        let statuses = vec![TicketStatus::Complete, TicketStatus::New];
        assert_eq!(
            compute_aggregate_status_local(&statuses),
            TicketStatus::InProgress
        );

        // Empty
        assert_eq!(compute_aggregate_status_local(&[]), TicketStatus::New);
    }
}

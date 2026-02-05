//! Plan reorder command

use std::collections::HashSet;

use serde_json::json;

use super::edit_in_editor;
use crate::commands::CommandOutput;
use crate::error::{JanusError, Result};
use crate::plan::Plan;
use crate::plan::types::PlanSection;

fn parse_and_validate_ticket_order(
    new_order: &str,
    original_tickets: &[String],
) -> Result<Vec<String>> {
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

    let original_set: HashSet<_> = original_tickets.iter().collect();
    let new_set: HashSet<_> = new_ticket_order.iter().collect();
    if original_set != new_set {
        return Err(JanusError::ReorderTicketMismatch);
    }

    Ok(new_ticket_order)
}

/// Reorder tickets or phases interactively
///
/// # Arguments
/// * `plan_id` - The plan ID (can be partial)
/// * `phase` - Optional phase to reorder tickets within
/// * `reorder_phases` - If true, reorder phases instead of tickets
/// * `output_json` - If true, output result as JSON
pub async fn cmd_plan_reorder(
    plan_id: &str,
    phase: Option<&str>,
    reorder_phases: bool,
    output_json: bool,
) -> Result<()> {
    let plan = Plan::find(plan_id).await?;
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
                temp_content.push_str(&format!("{num}\n"));
            } else {
                temp_content.push_str(&format!("{num}: {name}\n"));
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
            println!("No tickets to reorder in phase '{phase_identifier}'");
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

        phase_obj.tickets = parse_and_validate_ticket_order(&new_order, &phase_obj.tickets)?;
    } else if metadata.is_simple() {
        // Reorder tickets in simple plan
        let tickets = metadata
            .tickets_section_mut()
            .ok_or_else(|| JanusError::PlanNoTicketsSection)?;

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

        *tickets = parse_and_validate_ticket_order(&new_order, tickets)?;
    } else {
        println!(
            "Use --phase to specify which phase to reorder, or --reorder-phases to reorder phases"
        );
        return Ok(());
    }

    // Write updated plan
    plan.write_metadata(&metadata)?;

    CommandOutput::new(json!({
        "plan_id": plan.id,
        "action": "reordered",
        "type": if reorder_phases { "phases" } else { "tickets" },
        "phase": phase,
    }))
    .with_text(format!("Reorder complete for plan {}", plan.id))
    .print(output_json)
}

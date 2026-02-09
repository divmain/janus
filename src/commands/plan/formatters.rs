//! Plan output formatters

use std::collections::HashMap;

use owo_colors::OwoColorize;
use serde_json::json;

use super::print_ticket_line;
use crate::commands::print_json;
use crate::commands::ticket_minimal_json_with_exists;
use crate::display::format_status_colored;
use crate::error::Result;
use crate::plan::types::{Phase, PhaseStatus, PlanMetadata, PlanSection, PlanStatus};
use crate::plan::{Plan, compute_all_phase_statuses, compute_plan_status};
use crate::types::TicketMetadata;

pub struct RawFormatter;
pub struct FullFormatter;
pub struct JsonFormatter;
pub struct TicketsOnlyFormatter;
pub struct PhasesOnlyFormatter;

impl RawFormatter {
    pub fn format(plan: &Plan) -> Result<()> {
        let content = plan.read_content()?;
        println!("{content}");
        Ok(())
    }
}

impl FullFormatter {
    pub fn format(
        _plan: &Plan,
        metadata: &PlanMetadata,
        ticket_map: &HashMap<String, TicketMetadata>,
        verbose_phases: &[String],
    ) -> Result<()> {
        let plan_status = compute_plan_status(metadata, ticket_map);

        Self::print_header(metadata, &plan_status);
        Self::print_description(metadata);
        Self::print_acceptance_criteria(metadata);
        Self::print_sections(metadata, ticket_map, verbose_phases);

        Ok(())
    }

    fn print_header(metadata: &PlanMetadata, status: &PlanStatus) {
        if let Some(ref title) = metadata.title {
            println!("{}", format!("# {title}").bold());
        }

        let status_badge = format_status_colored(status.status);
        let progress = status.progress_string();
        println!();
        println!("{status_badge} Progress: {progress} tickets");
    }

    fn print_description(metadata: &PlanMetadata) {
        if let Some(ref description) = metadata.description {
            println!();
            println!("{description}");
        }
    }

    fn print_acceptance_criteria(metadata: &PlanMetadata) {
        if !metadata.acceptance_criteria.is_empty() {
            println!();
            println!("{}", "## Acceptance Criteria".bold());
            println!();
            for criterion in &metadata.acceptance_criteria {
                println!("- [ ] {criterion}");
            }
        }
    }

    fn print_sections(
        metadata: &PlanMetadata,
        ticket_map: &HashMap<String, TicketMetadata>,
        verbose_phases: &[String],
    ) {
        let phase_statuses = compute_all_phase_statuses(metadata, ticket_map);
        let mut phase_idx = 0;

        for section in &metadata.sections {
            println!();
            match section {
                PlanSection::Phase(phase) => {
                    let phase_status = phase_statuses.get(phase_idx);
                    phase_idx += 1;
                    Self::print_phase_section(phase, phase_status, ticket_map, verbose_phases);
                }
                PlanSection::Tickets(ts) => {
                    Self::print_tickets_section(&ts.tickets, ticket_map);
                }
                PlanSection::FreeForm(freeform) => {
                    Self::print_freeform_section(freeform);
                }
            }
        }
    }

    fn print_phase_section(
        phase: &Phase,
        phase_status: Option<&PhaseStatus>,
        ticket_map: &HashMap<String, TicketMetadata>,
        verbose_phases: &[String],
    ) {
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

        if let Some(ref desc) = phase.description {
            println!();
            println!("{desc}");
        }

        if !phase.success_criteria.is_empty() {
            println!();
            println!("{}", "### Success Criteria".bold());
            println!();
            for criterion in &phase.success_criteria {
                println!("- {criterion}");
            }
        }

        if !phase.tickets.is_empty() {
            println!();
            println!("{}", "### Tickets".bold());
            println!();
            let full_summary = verbose_phases.contains(&phase.number);
            for (i, ticket_id) in phase.tickets.iter().enumerate() {
                print_ticket_line(i + 1, ticket_id, ticket_map, full_summary);
            }
        }
    }

    fn print_tickets_section(tickets: &[String], ticket_map: &HashMap<String, TicketMetadata>) {
        println!("{}", "## Tickets".bold());
        println!();
        for (i, ticket_id) in tickets.iter().enumerate() {
            print_ticket_line(i + 1, ticket_id, ticket_map, false);
        }
    }

    fn print_freeform_section(freeform: &crate::plan::types::FreeFormSection) {
        println!("{}", format!("## {}", freeform.heading).bold());
        if !freeform.content.is_empty() {
            println!();
            println!("{}", freeform.content);
        }
    }
}

impl JsonFormatter {
    pub fn format(
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
}

impl TicketsOnlyFormatter {
    pub fn format(
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
}

impl PhasesOnlyFormatter {
    pub fn format(
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
}

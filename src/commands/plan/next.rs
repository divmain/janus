//! Plan next command

use std::collections::HashMap;

use owo_colors::OwoColorize;
use serde_json::json;

use crate::commands::print_json;
use crate::display::format_status_colored;
use crate::error::Result;
use crate::plan::types::PlanMetadata;
use crate::plan::{Plan, compute_phase_status};
use crate::ticket::build_ticket_map;
use crate::types::{TicketMetadata, TicketStatus};

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
    let plan = Plan::find(id).await?;
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

        print_json(&json!({
            "plan_id": plan.id,
            "next_items": next_items_json,
        }))?;
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
            let status_badge = format_status_colored(status);
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

/// Helper struct for next item results
struct NextItemResult {
    phase_number: String,
    phase_name: String,
    tickets: Vec<(String, Option<TicketMetadata>)>,
}

/// Get next actionable items for a phased plan
fn get_next_items_phased(
    metadata: &PlanMetadata,
    ticket_map: &HashMap<String, TicketMetadata>,
    phase_only: bool,
    all: bool,
    count: usize,
) -> Vec<NextItemResult> {
    let phases = metadata.phases();
    let mut results = Vec::new();

    for phase in &phases {
        // Compute phase status
        let phase_status = compute_phase_status(phase, ticket_map);

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
    ticket_map: &HashMap<String, TicketMetadata>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::types::{Phase, PlanSection};

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
}

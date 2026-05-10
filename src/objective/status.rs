//! Objective status computation logic.
//!
//! Status is auto-computed at read time from the `satisfied_by` reference.
//! If `satisfied_by` is absent or the referenced entity isn't complete,
//! status is `Unrealized`. Otherwise, status is `Achieved`.

use std::collections::HashMap;

use crate::plan::types::PlanMetadata;
use crate::status::is_terminal;
use crate::types::{ObjectiveStatus, TicketMetadata, TicketStatus};

/// Compute the status of an objective based on its `satisfied_by` reference.
///
/// # Rules
///
/// - If `satisfied_by` is `None` → `Unrealized`
/// - If it starts with `plan-`, look it up in `plan_map`. If found, check whether
///   all its tickets (from `ticket_map`) are complete or cancelled. If so → `Achieved`,
///   otherwise → `Unrealized`.
/// - Otherwise, treat as a ticket ID. If found in `ticket_map` and status is
///   `Complete` → `Achieved`, else → `Unrealized`.
/// - If the referenced entity is not found (dangling reference) → `Unrealized`.
pub fn compute_objective_status(
    satisfied_by: Option<&str>,
    ticket_map: &HashMap<String, TicketMetadata>,
    plan_map: &HashMap<String, PlanMetadata>,
) -> ObjectiveStatus {
    let Some(ref_id) = satisfied_by else {
        return ObjectiveStatus::Unrealized;
    };

    if ref_id.starts_with("plan-") {
        // Look up plan and check if all its tickets are terminal
        if let Some(plan_meta) = plan_map.get(ref_id) {
            let all_ticket_ids = plan_meta.all_tickets();

            if all_ticket_ids.is_empty() {
                // Plan with no tickets is not considered achieved
                return ObjectiveStatus::Unrealized;
            }

            // Check that all referenced tickets are terminal (complete or cancelled)
            let all_terminal = all_ticket_ids.iter().all(|tid| {
                ticket_map
                    .get(*tid)
                    .is_some_and(|t| t.status.is_some_and(is_terminal))
            });

            if all_terminal {
                ObjectiveStatus::Achieved
            } else {
                ObjectiveStatus::Unrealized
            }
        } else {
            // Dangling plan reference
            ObjectiveStatus::Unrealized
        }
    } else {
        // Treat as ticket ID
        if let Some(ticket) = ticket_map.get(ref_id) {
            if ticket.status == Some(TicketStatus::Complete) {
                ObjectiveStatus::Achieved
            } else {
                ObjectiveStatus::Unrealized
            }
        } else {
            // Dangling ticket reference
            ObjectiveStatus::Unrealized
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::types::{PlanSection, TicketsSection};
    use crate::types::{PlanId, TicketId};

    fn make_ticket(id: &str, status: TicketStatus) -> TicketMetadata {
        TicketMetadata {
            id: Some(TicketId::new_unchecked(id)),
            status: Some(status),
            ..Default::default()
        }
    }

    fn make_plan(id: &str, ticket_ids: Vec<&str>) -> PlanMetadata {
        let mut meta = PlanMetadata {
            id: Some(PlanId::new_unchecked(id)),
            ..Default::default()
        };
        meta.sections.push(PlanSection::Tickets(TicketsSection::new(
            ticket_ids.iter().map(|s| s.to_string()).collect(),
        )));
        meta
    }

    #[test]
    fn test_no_satisfied_by() {
        let ticket_map = HashMap::new();
        let plan_map = HashMap::new();

        assert_eq!(
            compute_objective_status(None, &ticket_map, &plan_map),
            ObjectiveStatus::Unrealized
        );
    }

    #[test]
    fn test_ticket_complete() {
        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "j-done".to_string(),
            make_ticket("j-done", TicketStatus::Complete),
        );
        let plan_map = HashMap::new();

        assert_eq!(
            compute_objective_status(Some("j-done"), &ticket_map, &plan_map),
            ObjectiveStatus::Achieved
        );
    }

    #[test]
    fn test_ticket_not_complete() {
        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "j-wip".to_string(),
            make_ticket("j-wip", TicketStatus::InProgress),
        );
        let plan_map = HashMap::new();

        assert_eq!(
            compute_objective_status(Some("j-wip"), &ticket_map, &plan_map),
            ObjectiveStatus::Unrealized
        );
    }

    #[test]
    fn test_ticket_cancelled_not_achieved() {
        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "j-can".to_string(),
            make_ticket("j-can", TicketStatus::Cancelled),
        );
        let plan_map = HashMap::new();

        // Only Complete maps to Achieved for single tickets
        assert_eq!(
            compute_objective_status(Some("j-can"), &ticket_map, &plan_map),
            ObjectiveStatus::Unrealized
        );
    }

    #[test]
    fn test_dangling_ticket_reference() {
        let ticket_map = HashMap::new();
        let plan_map = HashMap::new();

        assert_eq!(
            compute_objective_status(Some("j-nonexistent"), &ticket_map, &plan_map),
            ObjectiveStatus::Unrealized
        );
    }

    #[test]
    fn test_plan_all_complete() {
        let mut ticket_map = HashMap::new();
        ticket_map.insert("t1".to_string(), make_ticket("t1", TicketStatus::Complete));
        ticket_map.insert("t2".to_string(), make_ticket("t2", TicketStatus::Complete));

        let mut plan_map = HashMap::new();
        plan_map.insert(
            "plan-done".to_string(),
            make_plan("plan-done", vec!["t1", "t2"]),
        );

        assert_eq!(
            compute_objective_status(Some("plan-done"), &ticket_map, &plan_map),
            ObjectiveStatus::Achieved
        );
    }

    #[test]
    fn test_plan_mixed_terminal() {
        // All tickets are terminal (complete or cancelled) → Achieved
        let mut ticket_map = HashMap::new();
        ticket_map.insert("t1".to_string(), make_ticket("t1", TicketStatus::Complete));
        ticket_map.insert("t2".to_string(), make_ticket("t2", TicketStatus::Cancelled));

        let mut plan_map = HashMap::new();
        plan_map.insert(
            "plan-mix".to_string(),
            make_plan("plan-mix", vec!["t1", "t2"]),
        );

        assert_eq!(
            compute_objective_status(Some("plan-mix"), &ticket_map, &plan_map),
            ObjectiveStatus::Achieved
        );
    }

    #[test]
    fn test_plan_not_all_complete() {
        let mut ticket_map = HashMap::new();
        ticket_map.insert("t1".to_string(), make_ticket("t1", TicketStatus::Complete));
        ticket_map.insert("t2".to_string(), make_ticket("t2", TicketStatus::New));

        let mut plan_map = HashMap::new();
        plan_map.insert(
            "plan-wip".to_string(),
            make_plan("plan-wip", vec!["t1", "t2"]),
        );

        assert_eq!(
            compute_objective_status(Some("plan-wip"), &ticket_map, &plan_map),
            ObjectiveStatus::Unrealized
        );
    }

    #[test]
    fn test_plan_empty_tickets() {
        let ticket_map = HashMap::new();
        let mut plan_map = HashMap::new();
        plan_map.insert("plan-empty".to_string(), make_plan("plan-empty", vec![]));

        assert_eq!(
            compute_objective_status(Some("plan-empty"), &ticket_map, &plan_map),
            ObjectiveStatus::Unrealized
        );
    }

    #[test]
    fn test_dangling_plan_reference() {
        let ticket_map = HashMap::new();
        let plan_map = HashMap::new();

        assert_eq!(
            compute_objective_status(Some("plan-nonexistent"), &ticket_map, &plan_map),
            ObjectiveStatus::Unrealized
        );
    }

    #[test]
    fn test_plan_with_missing_ticket() {
        // Plan references a ticket that doesn't exist in ticket_map
        let mut ticket_map = HashMap::new();
        ticket_map.insert("t1".to_string(), make_ticket("t1", TicketStatus::Complete));
        // t2 is missing from ticket_map

        let mut plan_map = HashMap::new();
        plan_map.insert(
            "plan-miss".to_string(),
            make_plan("plan-miss", vec!["t1", "t2"]),
        );

        // Missing ticket means not all are terminal → Unrealized
        assert_eq!(
            compute_objective_status(Some("plan-miss"), &ticket_map, &plan_map),
            ObjectiveStatus::Unrealized
        );
    }
}

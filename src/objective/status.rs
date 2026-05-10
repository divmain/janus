//! Objective status computation logic.
//!
//! Status is auto-computed at read time from the `satisfied_by` references.
//! If `satisfied_by` is empty or any referenced entity isn't complete,
//! status is `Unrealized`. Otherwise, status is `Achieved`.

use std::collections::HashMap;

use crate::plan::types::PlanMetadata;
use crate::status::is_terminal;
use crate::types::{ObjectiveStatus, TicketMetadata, TicketStatus};

/// Compute the status of an objective based on its `satisfied_by` references.
///
/// # Rules
///
/// - If `satisfied_by` is empty → `Unrealized`
/// - For each reference: if it starts with `plan-`, look it up in `plan_map`. If found, check whether
///   all its tickets (from `ticket_map`) are complete or cancelled. If so → contributes to `Achieved`,
///   otherwise → `Unrealized`.
/// - Otherwise, treat as a ticket ID. If found in `ticket_map` and status is
///   `Complete` or `Archived` → contributes to `Achieved`, else → `Unrealized`.
/// - If any referenced entity is not found (dangling reference) → `Unrealized`.
/// - Only if ALL references are achieved → `Achieved`.
pub fn compute_objective_status(
    satisfied_by: &[String],
    ticket_map: &HashMap<String, TicketMetadata>,
    plan_map: &HashMap<String, PlanMetadata>,
) -> ObjectiveStatus {
    if satisfied_by.is_empty() {
        return ObjectiveStatus::Unrealized;
    }

    let all_achieved = satisfied_by.iter().all(|ref_id| {
        is_ref_achieved(ref_id, ticket_map, plan_map)
    });

    if all_achieved {
        ObjectiveStatus::Achieved
    } else {
        ObjectiveStatus::Unrealized
    }
}

fn is_ref_achieved(
    ref_id: &str,
    ticket_map: &HashMap<String, TicketMetadata>,
    plan_map: &HashMap<String, PlanMetadata>,
) -> bool {
    if ref_id.starts_with("plan-") {
        plan_map.get(ref_id).is_some_and(|plan_meta| {
            let all_ticket_ids = plan_meta.all_tickets();
            !all_ticket_ids.is_empty()
                && all_ticket_ids.iter().all(|tid| {
                    ticket_map.get(*tid).is_some_and(|t| t.status.is_some_and(is_terminal))
                })
        })
    } else {
        ticket_map.get(ref_id).is_some_and(|ticket| {
            matches!(ticket.status, Some(TicketStatus::Complete) | Some(TicketStatus::Archived))
        })
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
            compute_objective_status(&[], &ticket_map, &plan_map),
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
            compute_objective_status(&["j-done".to_string()], &ticket_map, &plan_map),
            ObjectiveStatus::Achieved
        );
    }

    #[test]
    fn test_ticket_archived() {
        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "j-arch".to_string(),
            make_ticket("j-arch", TicketStatus::Archived),
        );
        let plan_map = HashMap::new();

        // Archived tickets have still satisfied the objective
        assert_eq!(
            compute_objective_status(&["j-arch".to_string()], &ticket_map, &plan_map),
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
            compute_objective_status(&["j-wip".to_string()], &ticket_map, &plan_map),
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
            compute_objective_status(&["j-can".to_string()], &ticket_map, &plan_map),
            ObjectiveStatus::Unrealized
        );
    }

    #[test]
    fn test_dangling_ticket_reference() {
        let ticket_map = HashMap::new();
        let plan_map = HashMap::new();

        assert_eq!(
            compute_objective_status(&["j-nonexistent".to_string()], &ticket_map, &plan_map),
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
            compute_objective_status(&["plan-done".to_string()], &ticket_map, &plan_map),
            ObjectiveStatus::Achieved
        );
    }

    #[test]
    fn test_plan_mixed_terminal() {
        // All tickets are terminal (complete or cancelled) -> Achieved
        let mut ticket_map = HashMap::new();
        ticket_map.insert("t1".to_string(), make_ticket("t1", TicketStatus::Complete));
        ticket_map.insert("t2".to_string(), make_ticket("t2", TicketStatus::Cancelled));

        let mut plan_map = HashMap::new();
        plan_map.insert(
            "plan-mix".to_string(),
            make_plan("plan-mix", vec!["t1", "t2"]),
        );

        assert_eq!(
            compute_objective_status(&["plan-mix".to_string()], &ticket_map, &plan_map),
            ObjectiveStatus::Achieved
        );
    }

    #[test]
    fn test_plan_with_archived_tickets() {
        // Plan with a mix of complete and archived tickets -> all terminal -> Achieved
        let mut ticket_map = HashMap::new();
        ticket_map.insert("t1".to_string(), make_ticket("t1", TicketStatus::Complete));
        ticket_map.insert("t2".to_string(), make_ticket("t2", TicketStatus::Archived));

        let mut plan_map = HashMap::new();
        plan_map.insert(
            "plan-arch".to_string(),
            make_plan("plan-arch", vec!["t1", "t2"]),
        );

        assert_eq!(
            compute_objective_status(&["plan-arch".to_string()], &ticket_map, &plan_map),
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
            compute_objective_status(&["plan-wip".to_string()], &ticket_map, &plan_map),
            ObjectiveStatus::Unrealized
        );
    }

    #[test]
    fn test_plan_empty_tickets() {
        let ticket_map = HashMap::new();
        let mut plan_map = HashMap::new();
        plan_map.insert("plan-empty".to_string(), make_plan("plan-empty", vec![]));

        assert_eq!(
            compute_objective_status(&["plan-empty".to_string()], &ticket_map, &plan_map),
            ObjectiveStatus::Unrealized
        );
    }

    #[test]
    fn test_dangling_plan_reference() {
        let ticket_map = HashMap::new();
        let plan_map = HashMap::new();

        assert_eq!(
            compute_objective_status(&["plan-nonexistent".to_string()], &ticket_map, &plan_map),
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

        // Missing ticket means not all are terminal -> Unrealized
        assert_eq!(
            compute_objective_status(&["plan-miss".to_string()], &ticket_map, &plan_map),
            ObjectiveStatus::Unrealized
        );
    }

    #[test]
    fn test_multiple_refs_all_complete() {
        let mut ticket_map = HashMap::new();
        ticket_map.insert("j-a".to_string(), make_ticket("j-a", TicketStatus::Complete));
        ticket_map.insert("j-b".to_string(), make_ticket("j-b", TicketStatus::Complete));
        let plan_map = HashMap::new();

        assert_eq!(
            compute_objective_status(&["j-a".to_string(), "j-b".to_string()], &ticket_map, &plan_map),
            ObjectiveStatus::Achieved
        );
    }

    #[test]
    fn test_multiple_refs_one_incomplete() {
        let mut ticket_map = HashMap::new();
        ticket_map.insert("j-a".to_string(), make_ticket("j-a", TicketStatus::Complete));
        ticket_map.insert("j-b".to_string(), make_ticket("j-b", TicketStatus::InProgress));
        let plan_map = HashMap::new();

        assert_eq!(
            compute_objective_status(&["j-a".to_string(), "j-b".to_string()], &ticket_map, &plan_map),
            ObjectiveStatus::Unrealized
        );
    }

    #[test]
    fn test_multiple_refs_mixed_ticket_and_plan() {
        let mut ticket_map = HashMap::new();
        ticket_map.insert("j-a".to_string(), make_ticket("j-a", TicketStatus::Complete));
        ticket_map.insert("t1".to_string(), make_ticket("t1", TicketStatus::Complete));

        let mut plan_map = HashMap::new();
        plan_map.insert(
            "plan-mix".to_string(),
            make_plan("plan-mix", vec!["t1"]),
        );

        assert_eq!(
            compute_objective_status(&["j-a".to_string(), "plan-mix".to_string()], &ticket_map, &plan_map),
            ObjectiveStatus::Achieved
        );
    }

    #[test]
    fn test_multiple_refs_one_cancelled() {
        let mut ticket_map = HashMap::new();
        ticket_map.insert("j-a".to_string(), make_ticket("j-a", TicketStatus::Complete));
        ticket_map.insert("j-b".to_string(), make_ticket("j-b", TicketStatus::Cancelled));
        let plan_map = HashMap::new();

        assert_eq!(
            compute_objective_status(&["j-a".to_string(), "j-b".to_string()], &ticket_map, &plan_map),
            ObjectiveStatus::Unrealized
        );
    }

    #[test]
    fn test_multiple_refs_one_dangling() {
        let mut ticket_map = HashMap::new();
        ticket_map.insert("j-a".to_string(), make_ticket("j-a", TicketStatus::Complete));
        let plan_map = HashMap::new();

        assert_eq!(
            compute_objective_status(&["j-a".to_string(), "j-missing".to_string()], &ticket_map, &plan_map),
            ObjectiveStatus::Unrealized
        );
    }
}

//! Plan status computation logic.
//!
//! This module provides functions for computing the status of plans and phases
//! based on their constituent tickets. Status is derived at runtime and never
//! stored - the markdown files remain the authoritative source.

use std::collections::HashMap;

use crate::plan::types::{Phase, PhaseStatus, PlanMetadata, PlanStatus};
use crate::status::{is_not_started, is_terminal};
use crate::types::{TicketMetadata, TicketStatus};

// ============================================================================
// Missing Ticket Policy
// ============================================================================
// When a ticket referenced in a plan does not exist in the ticket database,
// the behavior is **consistent across all code paths**:
//
// 1. **Warning**: A warning is printed to stderr (via `eprintln!`)
// 2. **Graceful degradation**: Operations continue with the ticket treated as missing
// 3. **No errors**: Missing ticket references do not cause commands to fail
//
// This policy ensures that:
// - Users are informed about missing tickets
// - Plans remain functional even with stale ticket references
// - Display commands show `[missing]` badges for visual feedback
// - Status computation skips missing tickets (they don't affect the computed status)
//
// Implementation: Use `resolve_ticket_or_warn()` to look up tickets with consistent
// behavior. For operations that modify plans (like `janus plan add-ticket`), ticket
// existence is validated before the operation proceeds.
// ============================================================================

/// Resolve a ticket from the ticket map, warning if missing
///
/// This provides consistent behavior across all code paths that need to look up tickets
/// referenced in plans. Missing tickets are logged to stderr and None is returned.
///
/// # Arguments
/// * `ticket_id` - The ID of the ticket to look up
/// * `ticket_map` - Map of ticket IDs to metadata
/// * `context` - Optional context string for the warning message (e.g., "in phase X")
///
/// # Returns
/// * `Some(ticket)` if found
/// * `None` if not found (warning printed to stderr)
pub fn resolve_ticket_or_warn<'a>(
    ticket_id: &str,
    ticket_map: &'a HashMap<String, TicketMetadata>,
    context: Option<&str>,
) -> Option<&'a TicketMetadata> {
    let ticket = ticket_map.get(ticket_id);
    if ticket.is_none() {
        let context_str = match context {
            Some(ctx) => format!(" {ctx}"),
            None => String::new(),
        };
        eprintln!("Warning: ticket '{ticket_id}' not found{context_str}");
    }
    ticket
}

/// Compute the status of a plan based on its constituent tickets.
///
/// Status computation rules:
/// 1. If all tickets are `complete` → plan status is `complete`
/// 2. If all tickets are `cancelled` → plan status is `cancelled`
/// 3. If all tickets are `complete` or `cancelled` (mixed) → plan status is `complete`
/// 4. If all tickets are `new` or `next` (not started) → plan status is `new`
/// 5. Otherwise (some started, some not started) → plan status is `in_progress`
///
/// Missing tickets are skipped with a warning printed to stderr.
pub fn compute_plan_status(
    metadata: &PlanMetadata,
    ticket_map: &HashMap<String, TicketMetadata>,
) -> PlanStatus {
    let all_ticket_ids = metadata.all_tickets();

    if all_ticket_ids.is_empty() {
        return PlanStatus {
            status: TicketStatus::New,
            completed_count: 0,
            total_count: 0,
        };
    }

    let plan_id = metadata.id.as_deref().unwrap_or("unknown");

    // Collect statuses of all referenced tickets, warning about missing ones
    let mut statuses: Vec<TicketStatus> = Vec::new();
    for id in all_ticket_ids.iter() {
        if let Some(ticket) =
            resolve_ticket_or_warn(id, ticket_map, Some(&format!("in plan '{plan_id}'")))
            && let Some(status) = ticket.status
        {
            statuses.push(status);
        }
    }

    // Use resolvable ticket count as denominator so progress is consistent
    // with status computation (which also only considers resolvable tickets)
    let total_count = statuses.len();

    let completed_count = statuses
        .iter()
        .filter(|s| **s == TicketStatus::Complete)
        .count();

    let status = compute_aggregate_status(&statuses);

    PlanStatus {
        status,
        completed_count,
        total_count,
    }
}

/// Compute the status of a single phase (internal implementation)
///
/// # Arguments
/// * `phase` - The phase to compute status for
/// * `ticket_map` - Map of ticket IDs to metadata
/// * `warn_missing` - If true, print warnings for missing tickets to stderr
fn compute_phase_status_impl(
    phase: &Phase,
    ticket_map: &HashMap<String, TicketMetadata>,
    warn_missing: bool,
) -> PhaseStatus {
    if phase.ticket_list.tickets.is_empty() {
        return PhaseStatus {
            phase_number: phase.number.clone(),
            phase_name: phase.name.clone(),
            status: TicketStatus::New,
            completed_count: 0,
            total_count: 0,
        };
    }

    // Collect statuses of all referenced tickets, warning about missing ones
    let mut statuses: Vec<TicketStatus> = Vec::new();
    for id in &phase.ticket_list.tickets {
        if warn_missing {
            if let Some(ticket) =
                resolve_ticket_or_warn(id, ticket_map, Some(&format!("in phase '{}'", phase.name)))
                && let Some(status) = ticket.status
            {
                statuses.push(status);
            }
        } else if let Some(ticket) = ticket_map.get(id)
            && let Some(status) = ticket.status
        {
            statuses.push(status);
        }
    }

    // Use resolvable ticket count as denominator so progress is consistent
    // with status computation (which also only considers resolvable tickets)
    let total_count = statuses.len();

    let completed_count = statuses
        .iter()
        .filter(|s| **s == TicketStatus::Complete)
        .count();

    let status = compute_aggregate_status(&statuses);

    PhaseStatus {
        phase_number: phase.number.clone(),
        phase_name: phase.name.clone(),
        status,
        completed_count,
        total_count,
    }
}

/// Compute the status of all phases in a plan.
///
/// Returns a vector of `PhaseStatus` for each phase in document order.
/// For simple plans (no phases), returns an empty vector.
/// Missing tickets are warned about to stderr.
pub fn compute_all_phase_statuses(
    metadata: &PlanMetadata,
    ticket_map: &HashMap<String, TicketMetadata>,
) -> Vec<PhaseStatus> {
    // Use the impl function without warnings since compute_plan_status
    // already warns about missing tickets at the plan level
    metadata
        .phases()
        .iter()
        .map(|phase| compute_phase_status_impl(phase, ticket_map, false))
        .collect()
}

/// Compute the status of a single phase
///
/// Missing tickets are skipped with a warning printed to stderr.
pub fn compute_phase_status(
    phase: &Phase,
    ticket_map: &HashMap<String, TicketMetadata>,
) -> PhaseStatus {
    compute_phase_status_impl(phase, ticket_map, true)
}

/// Compute aggregate status from a list of ticket statuses.
///
/// # Truth Table
///
/// | Condition                              | Result        |
/// |----------------------------------------|---------------|
/// | Empty list                             | `new`         |
/// | All `complete`                         | `complete`    |
/// | All `cancelled`                        | `cancelled`   |
/// | All terminal (mixed complete/cancelled)| `complete`    |
/// | All not started (new/next)             | `new`         |
/// | Any other combination                  | `in_progress` |
///
/// Note: "Terminal" means `complete` or `cancelled` (see `is_terminal()`).
/// "Not started" means `new` or `next` (see `is_not_started()`).
pub fn compute_aggregate_status(statuses: &[TicketStatus]) -> TicketStatus {
    if statuses.is_empty() {
        return TicketStatus::New;
    }

    let all_terminal = statuses.iter().all(|&s| is_terminal(s));
    let all_not_started = statuses.iter().all(|&s| is_not_started(s));
    let has_complete = statuses.contains(&TicketStatus::Complete);
    let has_cancelled = statuses.contains(&TicketStatus::Cancelled);

    match (all_terminal, all_not_started, has_complete, has_cancelled) {
        // All tickets finished: determine if pure complete, pure cancelled, or mixed
        (true, _, true, false) => TicketStatus::Complete,
        (true, _, false, true) => TicketStatus::Cancelled,
        (true, _, true, true) => TicketStatus::Complete, // Mixed terminal → complete
        // All tickets not yet started
        (_, true, _, _) => TicketStatus::New,
        // Some work has started but not all finished
        _ => TicketStatus::InProgress,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::types::{PlanSection, TicketList, TicketsSection};
    use crate::types::TicketId;

    #[test]
    fn test_compute_aggregate_status_all_complete() {
        let statuses = vec![TicketStatus::Complete, TicketStatus::Complete];
        assert_eq!(compute_aggregate_status(&statuses), TicketStatus::Complete);
    }

    #[test]
    fn test_compute_aggregate_status_all_cancelled() {
        let statuses = vec![TicketStatus::Cancelled, TicketStatus::Cancelled];
        assert_eq!(compute_aggregate_status(&statuses), TicketStatus::Cancelled);
    }

    #[test]
    fn test_compute_aggregate_status_mixed_finished() {
        let statuses = vec![TicketStatus::Complete, TicketStatus::Cancelled];
        assert_eq!(compute_aggregate_status(&statuses), TicketStatus::Complete);
    }

    #[test]
    fn test_compute_aggregate_status_all_not_started() {
        let statuses = vec![TicketStatus::New, TicketStatus::Next];
        assert_eq!(compute_aggregate_status(&statuses), TicketStatus::New);
    }

    #[test]
    fn test_compute_aggregate_status_in_progress() {
        // Some started, some not
        let statuses = vec![TicketStatus::Complete, TicketStatus::New];
        assert_eq!(
            compute_aggregate_status(&statuses),
            TicketStatus::InProgress
        );

        let statuses = vec![TicketStatus::InProgress, TicketStatus::New];
        assert_eq!(
            compute_aggregate_status(&statuses),
            TicketStatus::InProgress
        );

        let statuses = vec![
            TicketStatus::Complete,
            TicketStatus::InProgress,
            TicketStatus::New,
        ];
        assert_eq!(
            compute_aggregate_status(&statuses),
            TicketStatus::InProgress
        );
    }

    #[test]
    fn test_compute_aggregate_status_empty() {
        let statuses: Vec<TicketStatus> = vec![];
        assert_eq!(compute_aggregate_status(&statuses), TicketStatus::New);
    }

    #[test]
    fn test_compute_plan_status_empty_plan() {
        let metadata = PlanMetadata::default();
        let ticket_map = HashMap::new();

        let status = compute_plan_status(&metadata, &ticket_map);
        assert_eq!(status.status, TicketStatus::New);
        assert_eq!(status.completed_count, 0);
        assert_eq!(status.total_count, 0);
    }

    #[test]
    fn test_compute_plan_status_with_tickets() {
        let mut metadata = PlanMetadata::default();
        metadata
            .sections
            .push(PlanSection::Tickets(TicketsSection::new(vec![
                "j-a1b2".to_string(),
                "j-c3d4".to_string(),
                "j-e5f6".to_string(),
            ])));

        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "j-a1b2".to_string(),
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-a1b2")),
                status: Some(TicketStatus::Complete),
                ..Default::default()
            },
        );
        ticket_map.insert(
            "j-c3d4".to_string(),
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-c3d4")),
                status: Some(TicketStatus::InProgress),
                ..Default::default()
            },
        );
        ticket_map.insert(
            "j-e5f6".to_string(),
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-e5f6")),
                status: Some(TicketStatus::New),
                ..Default::default()
            },
        );

        let status = compute_plan_status(&metadata, &ticket_map);
        assert_eq!(status.status, TicketStatus::InProgress);
        assert_eq!(status.completed_count, 1);
        assert_eq!(status.total_count, 3);
    }

    #[test]
    fn test_compute_phase_status() {
        let phase = Phase {
            number: "1".to_string(),
            name: "Infrastructure".to_string(),
            description: None,
            success_criteria: vec![],
            ticket_list: TicketList {
                tickets: vec!["j-a1b2".to_string(), "j-c3d4".to_string()],
                tickets_raw: None,
            },
            ..Default::default()
        };

        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "j-a1b2".to_string(),
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-a1b2")),
                status: Some(TicketStatus::Complete),
                ..Default::default()
            },
        );
        ticket_map.insert(
            "j-c3d4".to_string(),
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-c3d4")),
                status: Some(TicketStatus::Complete),
                ..Default::default()
            },
        );

        let status = compute_phase_status(&phase, &ticket_map);
        assert_eq!(status.phase_number, "1");
        assert_eq!(status.phase_name, "Infrastructure");
        assert_eq!(status.status, TicketStatus::Complete);
        assert_eq!(status.completed_count, 2);
        assert_eq!(status.total_count, 2);
    }

    #[test]
    fn test_compute_phase_status_missing_tickets() {
        let phase = Phase {
            number: "1".to_string(),
            name: "Test".to_string(),
            description: None,
            success_criteria: vec![],
            ticket_list: TicketList {
                tickets: vec![
                    "j-exists".to_string(),
                    "j-missing".to_string(), // Not in ticket_map
                ],
                tickets_raw: None,
            },
            ..Default::default()
        };

        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "j-exists".to_string(),
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-exists")),
                status: Some(TicketStatus::Complete),
                ..Default::default()
            },
        );
        // j-missing is not added

        let status = compute_phase_status(&phase, &ticket_map);
        // Missing tickets are skipped, so we only see the one that exists
        assert_eq!(status.status, TicketStatus::Complete);
        assert_eq!(status.completed_count, 1);
        assert_eq!(status.total_count, 1); // Total only counts resolvable tickets
    }

    /// Helper to create a ticket metadata with a given status
    fn make_ticket(id: &str, status: TicketStatus) -> TicketMetadata {
        TicketMetadata {
            id: Some(TicketId::new_unchecked(id)),
            status: Some(status),
            ..Default::default()
        }
    }

    /// Helper to create a phased plan with given phase tickets
    fn make_phased_plan(phases: Vec<(&str, &str, Vec<&str>)>) -> PlanMetadata {
        let mut metadata = PlanMetadata::default();
        for (number, name, tickets) in phases {
            let phase = Phase {
                number: number.to_string(),
                name: name.to_string(),
                description: None,
                success_criteria: vec![],
                ticket_list: TicketList {
                    tickets: tickets.iter().map(|s| s.to_string()).collect(),
                    tickets_raw: None,
                },
                ..Default::default()
            };
            metadata.sections.push(PlanSection::Phase(phase));
        }
        metadata
    }

    #[test]
    fn test_compute_phased_plan_status_all_phases_complete() {
        let metadata = make_phased_plan(vec![
            ("1", "Phase One", vec!["t1", "t2"]),
            ("2", "Phase Two", vec!["t3", "t4"]),
        ]);

        let mut ticket_map = HashMap::new();
        ticket_map.insert("t1".to_string(), make_ticket("t1", TicketStatus::Complete));
        ticket_map.insert("t2".to_string(), make_ticket("t2", TicketStatus::Complete));
        ticket_map.insert("t3".to_string(), make_ticket("t3", TicketStatus::Complete));
        ticket_map.insert("t4".to_string(), make_ticket("t4", TicketStatus::Complete));

        let status = compute_plan_status(&metadata, &ticket_map);
        assert_eq!(status.status, TicketStatus::Complete);
        assert_eq!(status.completed_count, 4);
        assert_eq!(status.total_count, 4);
    }

    #[test]
    fn test_compute_phased_plan_status_mixed_phases() {
        // Phase 1: complete, Phase 2: new
        let metadata = make_phased_plan(vec![
            ("1", "Phase One", vec!["t1", "t2"]),
            ("2", "Phase Two", vec!["t3", "t4"]),
        ]);

        let mut ticket_map = HashMap::new();
        ticket_map.insert("t1".to_string(), make_ticket("t1", TicketStatus::Complete));
        ticket_map.insert("t2".to_string(), make_ticket("t2", TicketStatus::Complete));
        ticket_map.insert("t3".to_string(), make_ticket("t3", TicketStatus::New));
        ticket_map.insert("t4".to_string(), make_ticket("t4", TicketStatus::New));

        let status = compute_plan_status(&metadata, &ticket_map);
        assert_eq!(status.status, TicketStatus::InProgress);
        assert_eq!(status.completed_count, 2);
        assert_eq!(status.total_count, 4);
    }

    #[test]
    fn test_compute_phased_plan_status_all_new() {
        let metadata = make_phased_plan(vec![
            ("1", "Phase One", vec!["t1"]),
            ("2", "Phase Two", vec!["t2"]),
        ]);

        let mut ticket_map = HashMap::new();
        ticket_map.insert("t1".to_string(), make_ticket("t1", TicketStatus::New));
        ticket_map.insert("t2".to_string(), make_ticket("t2", TicketStatus::Next));

        let status = compute_plan_status(&metadata, &ticket_map);
        assert_eq!(status.status, TicketStatus::New);
        assert_eq!(status.completed_count, 0);
        assert_eq!(status.total_count, 2);
    }

    #[test]
    fn test_compute_phased_plan_status_all_cancelled() {
        let metadata = make_phased_plan(vec![
            ("1", "Phase One", vec!["t1"]),
            ("2", "Phase Two", vec!["t2"]),
        ]);

        let mut ticket_map = HashMap::new();
        ticket_map.insert("t1".to_string(), make_ticket("t1", TicketStatus::Cancelled));
        ticket_map.insert("t2".to_string(), make_ticket("t2", TicketStatus::Cancelled));

        let status = compute_plan_status(&metadata, &ticket_map);
        assert_eq!(status.status, TicketStatus::Cancelled);
        assert_eq!(status.completed_count, 0);
        assert_eq!(status.total_count, 2);
    }

    #[test]
    fn test_compute_phased_plan_status_mixed_complete_cancelled() {
        // All finished but mixed complete/cancelled should be "complete"
        let metadata = make_phased_plan(vec![("1", "Phase One", vec!["t1", "t2"])]);

        let mut ticket_map = HashMap::new();
        ticket_map.insert("t1".to_string(), make_ticket("t1", TicketStatus::Complete));
        ticket_map.insert("t2".to_string(), make_ticket("t2", TicketStatus::Cancelled));

        let status = compute_plan_status(&metadata, &ticket_map);
        assert_eq!(status.status, TicketStatus::Complete);
        assert_eq!(status.completed_count, 1); // Only "complete" counts toward completed_count
        assert_eq!(status.total_count, 2);
    }

    #[test]
    fn test_compute_phased_plan_status_in_progress_ticket() {
        let metadata = make_phased_plan(vec![("1", "Phase One", vec!["t1", "t2"])]);

        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "t1".to_string(),
            make_ticket("t1", TicketStatus::InProgress),
        );
        ticket_map.insert("t2".to_string(), make_ticket("t2", TicketStatus::New));

        let status = compute_plan_status(&metadata, &ticket_map);
        assert_eq!(status.status, TicketStatus::InProgress);
        assert_eq!(status.completed_count, 0);
        assert_eq!(status.total_count, 2);
    }

    #[test]
    fn test_compute_phased_plan_status_empty_phases() {
        // Plan with phases but no tickets
        let metadata = make_phased_plan(vec![
            ("1", "Empty Phase", vec![]),
            ("2", "Also Empty", vec![]),
        ]);

        let ticket_map = HashMap::new();

        let status = compute_plan_status(&metadata, &ticket_map);
        assert_eq!(status.status, TicketStatus::New);
        assert_eq!(status.completed_count, 0);
        assert_eq!(status.total_count, 0);
    }

    #[test]
    fn test_compute_all_phase_statuses() {
        let metadata = make_phased_plan(vec![
            ("1", "Phase One", vec!["t1", "t2"]),
            ("2", "Phase Two", vec!["t3"]),
            ("3", "Phase Three", vec!["t4", "t5"]),
        ]);

        let mut ticket_map = HashMap::new();
        // Phase 1: all complete
        ticket_map.insert("t1".to_string(), make_ticket("t1", TicketStatus::Complete));
        ticket_map.insert("t2".to_string(), make_ticket("t2", TicketStatus::Complete));
        // Phase 2: in progress
        ticket_map.insert(
            "t3".to_string(),
            make_ticket("t3", TicketStatus::InProgress),
        );
        // Phase 3: all new
        ticket_map.insert("t4".to_string(), make_ticket("t4", TicketStatus::New));
        ticket_map.insert("t5".to_string(), make_ticket("t5", TicketStatus::New));

        let phase_statuses = compute_all_phase_statuses(&metadata, &ticket_map);

        assert_eq!(phase_statuses.len(), 3);

        // Phase 1
        assert_eq!(phase_statuses[0].phase_number, "1");
        assert_eq!(phase_statuses[0].phase_name, "Phase One");
        assert_eq!(phase_statuses[0].status, TicketStatus::Complete);
        assert_eq!(phase_statuses[0].completed_count, 2);
        assert_eq!(phase_statuses[0].total_count, 2);

        // Phase 2
        assert_eq!(phase_statuses[1].phase_number, "2");
        assert_eq!(phase_statuses[1].phase_name, "Phase Two");
        assert_eq!(phase_statuses[1].status, TicketStatus::InProgress);
        assert_eq!(phase_statuses[1].completed_count, 0);
        assert_eq!(phase_statuses[1].total_count, 1);

        // Phase 3
        assert_eq!(phase_statuses[2].phase_number, "3");
        assert_eq!(phase_statuses[2].phase_name, "Phase Three");
        assert_eq!(phase_statuses[2].status, TicketStatus::New);
        assert_eq!(phase_statuses[2].completed_count, 0);
        assert_eq!(phase_statuses[2].total_count, 2);
    }

    #[test]
    fn test_compute_all_phase_statuses_simple_plan() {
        // Simple plan (no phases) should return empty vec
        let mut metadata = PlanMetadata::default();
        metadata
            .sections
            .push(PlanSection::Tickets(TicketsSection::new(vec![
                "t1".to_string(),
                "t2".to_string(),
            ])));

        let ticket_map = HashMap::new();
        let phase_statuses = compute_all_phase_statuses(&metadata, &ticket_map);

        assert!(phase_statuses.is_empty());
    }

    #[test]
    fn test_compute_phase_status_empty_phase() {
        let phase = Phase {
            number: "1".to_string(),
            name: "Empty".to_string(),
            description: None,
            success_criteria: vec![],
            ticket_list: TicketList {
                tickets: vec![],
                tickets_raw: None,
            },
            ..Default::default()
        };

        let ticket_map = HashMap::new();
        let status = compute_phase_status(&phase, &ticket_map);

        assert_eq!(status.status, TicketStatus::New);
        assert_eq!(status.completed_count, 0);
        assert_eq!(status.total_count, 0);
    }

    #[test]
    fn test_compute_phased_plan_with_missing_tickets() {
        let metadata = make_phased_plan(vec![("1", "Phase One", vec!["t1", "t2", "t-missing"])]);

        let mut ticket_map = HashMap::new();
        ticket_map.insert("t1".to_string(), make_ticket("t1", TicketStatus::Complete));
        ticket_map.insert("t2".to_string(), make_ticket("t2", TicketStatus::Complete));
        // t-missing is not in ticket_map

        let status = compute_plan_status(&metadata, &ticket_map);
        // Missing tickets are skipped for status computation
        // Both existing tickets are complete, so status should be complete
        assert_eq!(status.status, TicketStatus::Complete);
        assert_eq!(status.completed_count, 2);
        // total_count only counts resolvable tickets, consistent with status
        assert_eq!(status.total_count, 2);
    }

    #[test]
    fn test_compute_aggregate_status_with_next() {
        // Test that Next status is treated as "not started"
        let statuses = vec![TicketStatus::Next, TicketStatus::New];
        assert_eq!(compute_aggregate_status(&statuses), TicketStatus::New);

        // Next mixed with in_progress should be in_progress
        let statuses = vec![TicketStatus::Next, TicketStatus::InProgress];
        assert_eq!(
            compute_aggregate_status(&statuses),
            TicketStatus::InProgress
        );

        // Next mixed with complete should be in_progress
        let statuses = vec![TicketStatus::Next, TicketStatus::Complete];
        assert_eq!(
            compute_aggregate_status(&statuses),
            TicketStatus::InProgress
        );
    }

    #[test]
    fn test_phase_status_progress_percent() {
        let status = PhaseStatus {
            phase_number: "1".to_string(),
            phase_name: "Test".to_string(),
            status: TicketStatus::InProgress,
            completed_count: 1,
            total_count: 4,
        };

        assert_eq!(status.progress_percent(), 25.0);
        assert_eq!(status.progress_string(), "1/4");
    }

    #[test]
    fn test_plan_status_progress_percent() {
        let status = PlanStatus {
            status: TicketStatus::InProgress,
            completed_count: 3,
            total_count: 10,
        };

        assert_eq!(status.progress_percent(), 30.0);
        assert_eq!(status.progress_string(), "3/10 (30%)");
    }

    #[test]
    fn test_compute_phased_plan_three_phases_progressive() {
        // Realistic scenario: first phase done, second in progress, third not started
        let metadata = make_phased_plan(vec![
            ("1", "Infrastructure", vec!["t1", "t2"]),
            ("2", "Implementation", vec!["t3", "t4", "t5"]),
            ("3", "Testing", vec!["t6", "t7"]),
        ]);

        let mut ticket_map = HashMap::new();
        // Phase 1: all complete
        ticket_map.insert("t1".to_string(), make_ticket("t1", TicketStatus::Complete));
        ticket_map.insert("t2".to_string(), make_ticket("t2", TicketStatus::Complete));
        // Phase 2: in progress (one done, one in progress, one new)
        ticket_map.insert("t3".to_string(), make_ticket("t3", TicketStatus::Complete));
        ticket_map.insert(
            "t4".to_string(),
            make_ticket("t4", TicketStatus::InProgress),
        );
        ticket_map.insert("t5".to_string(), make_ticket("t5", TicketStatus::New));
        // Phase 3: not started
        ticket_map.insert("t6".to_string(), make_ticket("t6", TicketStatus::New));
        ticket_map.insert("t7".to_string(), make_ticket("t7", TicketStatus::New));

        let status = compute_plan_status(&metadata, &ticket_map);
        assert_eq!(status.status, TicketStatus::InProgress);
        assert_eq!(status.completed_count, 3); // t1, t2, t3
        assert_eq!(status.total_count, 7);

        // Verify individual phase statuses
        let phase_statuses = compute_all_phase_statuses(&metadata, &ticket_map);
        assert_eq!(phase_statuses[0].status, TicketStatus::Complete);
        assert_eq!(phase_statuses[1].status, TicketStatus::InProgress);
        assert_eq!(phase_statuses[2].status, TicketStatus::New);
    }
}

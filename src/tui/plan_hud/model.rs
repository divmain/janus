//! Data model and state computation for the Plan HUD
//!
//! This module contains the pure data types and logic for computing HUD state
//! from plan metadata and ticket data. No UI/iocraft dependencies here — this
//! is testable independently.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::events::{self, types::EventType};
use crate::plan::types::{PhaseStatus, PlanMetadata, PlanSection, PlanStatus};
use crate::status::plan::{compute_all_phase_statuses, compute_plan_status};
use crate::store::get_or_init_store;
use crate::types::{TicketMetadata, TicketStatus};

// ============================================================================
// HUD State
// ============================================================================

/// Complete computed state for a single HUD render
#[derive(Debug, Clone)]
pub struct HudState {
    /// Plan metadata
    pub plan: PlanMetadata,
    /// Computed overall plan status
    pub plan_status: PlanStatus,
    /// Per-phase computed statuses (empty for simple plans)
    pub phase_statuses: Vec<PhaseStatus>,
    /// All plan tickets with their metadata, in plan order
    pub tickets: Vec<HudTicket>,
    /// Tickets grouped by phase (phase_index -> ticket indices into `tickets`)
    pub phase_tickets: Vec<Vec<usize>>,
    /// Tickets currently in_progress
    pub active_ticket_ids: Vec<String>,
    /// Activity events relevant to this plan
    pub activity_events: Vec<ActivityEvent>,
    /// Timing information
    pub timing: TimingState,
    /// Whether this is a simple (non-phased) plan
    pub is_simple: bool,
}

/// A ticket within the HUD, enriched with display annotations
#[derive(Debug, Clone)]
pub struct HudTicket {
    /// The ticket ID
    pub id: String,
    /// Ticket metadata (may be None if ticket is missing from store)
    pub metadata: Option<TicketMetadata>,
    /// Which phase this ticket belongs to (None for simple plans)
    pub phase_index: Option<usize>,
    /// Whether this ticket is currently active (in_progress)
    pub is_active: bool,
    /// Duration this ticket took to complete (if complete, from event log)
    pub completion_duration: Option<std::time::Duration>,
}

// ============================================================================
// Scroll Row Model
// ============================================================================

/// A single row in the flat scrollable list. This flattens phase headers and
/// ticket rows into a single ordered sequence so that scroll_offset can
/// uniformly address any visible row.
#[derive(Debug, Clone)]
pub enum ScrollRow {
    /// A phase header row
    PhaseHeader {
        /// Index into HudState::phase_statuses / plan.phases()
        phase_idx: usize,
    },
    /// A ticket row
    Ticket {
        /// Index into HudState::tickets
        ticket_idx: usize,
    },
}

impl ScrollRow {
    /// If this is a Ticket row, return its ticket index
    pub fn ticket_idx(&self) -> Option<usize> {
        match self {
            ScrollRow::Ticket { ticket_idx } => Some(*ticket_idx),
            _ => None,
        }
    }
}

/// Build a flat list of scroll rows from the HUD state.
///
/// For phased plans: [PhaseHeader(0), Ticket, Ticket, ..., PhaseHeader(1), Ticket, ...]
/// For simple plans: [Ticket, Ticket, ...]
///
/// `is_compact` and phase statuses control whether completed phase tickets are
/// hidden (compact mode hides tickets under completed non-active phases).
pub fn build_scroll_rows(state: &HudState, is_compact: bool) -> Vec<ScrollRow> {
    let mut rows = Vec::new();

    if state.is_simple {
        // Simple plan: just tickets, no headers
        for idx in 0..state.tickets.len() {
            rows.push(ScrollRow::Ticket { ticket_idx: idx });
        }
    } else {
        // Phased plan: interleave headers and tickets
        let phases = state.plan.phases();
        for (phase_idx, _phase) in phases.iter().enumerate() {
            let ps = state.phase_statuses.get(phase_idx);
            let phase_status = ps.map(|p| p.status).unwrap_or(TicketStatus::New);
            let is_active_phase =
                state.phase_statuses.iter().take(phase_idx).all(|p| {
                    p.status == TicketStatus::Complete || p.status == TicketStatus::Cancelled
                }) && phase_status != TicketStatus::Complete
                    && phase_status != TicketStatus::Cancelled;

            rows.push(ScrollRow::PhaseHeader { phase_idx });

            // In compact mode, hide tickets for completed non-active phases
            let show_tickets =
                !is_compact || is_active_phase || phase_status != TicketStatus::Complete;

            if show_tickets && let Some(indices) = state.phase_tickets.get(phase_idx) {
                for &ticket_idx in indices {
                    rows.push(ScrollRow::Ticket { ticket_idx });
                }
            }
        }
    }

    rows
}

// ============================================================================
// Activity Events
// ============================================================================

/// A formatted event for the activity log
#[derive(Debug, Clone)]
pub struct ActivityEvent {
    /// Formatted timestamp (HH:MM:SS)
    pub time: String,
    /// The ticket or entity ID
    pub entity_id: String,
    /// Human-readable description of the event
    pub description: String,
    /// Event type for styling
    pub event_type: EventType,
    /// The raw ISO timestamp for ordering
    pub raw_timestamp: String,
}

// ============================================================================
// Timing
// ============================================================================

/// Time tracking state
#[derive(Debug, Clone)]
pub struct TimingState {
    /// When the active ticket entered in_progress (ISO timestamp from events)
    pub active_ticket_start: Option<String>,
    /// Average cycle time for completed tickets in this plan
    pub avg_cycle_time: Option<std::time::Duration>,
    /// Number of remaining non-terminal tickets
    pub remaining_count: usize,
}

impl TimingState {
    /// Estimate remaining time based on average cycle time
    pub fn estimated_remaining(&self) -> Option<std::time::Duration> {
        self.avg_cycle_time
            .map(|avg| avg * self.remaining_count as u32)
    }
}

// ============================================================================
// Flash / Transition Tracking
// ============================================================================

/// Types of visual transitions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlashType {
    /// A ticket just completed
    Completed,
    /// A ticket just became active
    Started,
    /// An entire phase just completed
    PhaseCompleted,
}

/// Tracks recent state transitions for flash animations
#[derive(Debug, Clone)]
pub struct FlashEntry {
    pub flash_type: FlashType,
    pub created: Instant,
}

/// Duration for which flash effects are visible
pub const FLASH_DURATION_SECS: u64 = 3;

// ============================================================================
// State Loading
// ============================================================================

/// Load the complete HUD state for a plan
pub async fn load_hud_state(plan_id: &str) -> crate::error::Result<HudState> {
    let store = get_or_init_store().await?;

    // Get plan metadata from store
    let plan = store.get_plan(plan_id).ok_or_else(|| {
        crate::error::JanusError::PlanNotFound(crate::types::PlanId::new_unchecked(plan_id))
    })?;

    // Build ticket map for status computation
    let ticket_map = store.build_ticket_map();

    // Compute statuses
    let plan_status = compute_plan_status(&plan, &ticket_map);
    let phase_statuses = compute_all_phase_statuses(&plan, &ticket_map);

    // Collect all plan ticket IDs
    let all_ticket_ids: Vec<&str> = plan.all_tickets();
    let plan_ticket_set: HashSet<String> = all_ticket_ids.iter().map(|s| s.to_string()).collect();

    // Build a scoped ticket map (only plan tickets) for timing/events
    let scoped_ticket_map: HashMap<String, TicketMetadata> = ticket_map
        .iter()
        .filter(|(id, _)| plan_ticket_set.contains(id.as_str()))
        .map(|(id, meta)| (id.clone(), meta.clone()))
        .collect();

    // Find active tickets
    let active_ticket_ids: Vec<String> = all_ticket_ids
        .iter()
        .filter(|id| {
            ticket_map
                .get(**id)
                .and_then(|m| m.status)
                .map(|s| s == TicketStatus::InProgress)
                .unwrap_or(false)
        })
        .map(|s| s.to_string())
        .collect();

    // Determine if simple plan
    let is_simple = plan.is_simple();

    // Build HudTicket list, grouped by phase
    let mut tickets = Vec::new();
    let mut phase_tickets: Vec<Vec<usize>> = Vec::new();

    if is_simple {
        // Simple plan: one flat list
        let mut indices = Vec::new();
        for id in &all_ticket_ids {
            let idx = tickets.len();
            tickets.push(HudTicket {
                id: id.to_string(),
                metadata: ticket_map.get(*id).cloned(),
                phase_index: None,
                is_active: active_ticket_ids.contains(&id.to_string()),
                completion_duration: None, // filled in by timing
            });
            indices.push(idx);
        }
        phase_tickets.push(indices);
    } else {
        // Phased plan: group by phase
        for (phase_idx, section) in plan.sections.iter().enumerate() {
            let phase_ticket_ids = match section {
                PlanSection::Phase(phase) => &phase.ticket_list.tickets,
                _ => continue,
            };
            let mut indices = Vec::new();
            for id in phase_ticket_ids {
                let idx = tickets.len();
                tickets.push(HudTicket {
                    id: id.to_string(),
                    metadata: ticket_map.get(id).cloned(),
                    phase_index: Some(phase_idx),
                    is_active: active_ticket_ids.contains(id),
                    completion_duration: None,
                });
                indices.push(idx);
            }
            phase_tickets.push(indices);
        }
    }

    // Load timing data and activity events from event log
    let (timing, activity_events, completion_durations) =
        load_events_data(&plan_ticket_set, &active_ticket_ids, &scoped_ticket_map);

    // Apply completion durations to tickets
    for ticket in &mut tickets {
        if let Some(dur) = completion_durations.get(&ticket.id) {
            ticket.completion_duration = Some(*dur);
        }
    }

    Ok(HudState {
        plan,
        plan_status,
        phase_statuses,
        tickets,
        phase_tickets,
        active_ticket_ids,
        activity_events,
        timing,
        is_simple,
    })
}

/// Load events data, timing information, and completion durations
fn load_events_data(
    plan_ticket_ids: &HashSet<String>,
    active_ticket_ids: &[String],
    ticket_map: &HashMap<String, TicketMetadata>,
) -> (
    TimingState,
    Vec<ActivityEvent>,
    HashMap<String, std::time::Duration>,
) {
    let events = events::read_events().unwrap_or_default();

    // Filter to plan-relevant events
    let plan_events: Vec<_> = events
        .iter()
        .filter(|e| plan_ticket_ids.contains(&e.entity_id))
        .collect();

    // Build activity log (last 50 events, newest first)
    let activity_events: Vec<ActivityEvent> = plan_events
        .iter()
        .rev()
        .take(50)
        .map(|e| {
            let description = format_event_description(e);
            let time = format_event_time(&e.timestamp);
            ActivityEvent {
                time,
                entity_id: e.entity_id.clone(),
                description,
                event_type: e.event_type.clone(),
                raw_timestamp: e.timestamp.clone(),
            }
        })
        .collect();

    // Find when the active ticket started (last status_changed to in_progress)
    let active_ticket_start = active_ticket_ids.first().and_then(|active_id| {
        plan_events
            .iter()
            .rev()
            .find(|e| {
                e.entity_id == *active_id
                    && e.event_type == EventType::StatusChanged
                    && e.data.get("to").and_then(|v| v.as_str()) == Some("in_progress")
            })
            .map(|e| e.timestamp.clone())
    });

    // Compute completion durations: for each completed ticket, find the time from
    // last in_progress start to completion
    let mut completion_durations: HashMap<String, std::time::Duration> = HashMap::new();
    let mut cycle_times: Vec<std::time::Duration> = Vec::new();

    for ticket_id in plan_ticket_ids {
        let status = ticket_map.get(ticket_id).and_then(|m| m.status);
        if status != Some(TicketStatus::Complete) {
            continue;
        }

        // Find last status_changed to in_progress
        let start_ts = plan_events
            .iter()
            .rev()
            .find(|e| {
                e.entity_id == *ticket_id
                    && e.event_type == EventType::StatusChanged
                    && e.data.get("to").and_then(|v| v.as_str()) == Some("in_progress")
            })
            .map(|e| &e.timestamp);

        // Find last status_changed to complete
        let end_ts = plan_events
            .iter()
            .rev()
            .find(|e| {
                e.entity_id == *ticket_id
                    && e.event_type == EventType::StatusChanged
                    && e.data.get("to").and_then(|v| v.as_str()) == Some("complete")
            })
            .map(|e| &e.timestamp);

        if let (Some(start), Some(end)) = (start_ts, end_ts)
            && let Some(duration) = parse_duration_between(start, end)
        {
            completion_durations.insert(ticket_id.clone(), duration);
            cycle_times.push(duration);
        }
    }

    // Compute average cycle time
    let avg_cycle_time = if cycle_times.is_empty() {
        None
    } else {
        let total: std::time::Duration = cycle_times.iter().sum();
        Some(total / cycle_times.len() as u32)
    };

    // Count remaining (non-terminal) tickets
    let remaining_count = ticket_map
        .values()
        .filter(|m| {
            plan_ticket_ids.contains(m.id.as_deref().unwrap_or(""))
                && !matches!(
                    m.status,
                    Some(TicketStatus::Complete) | Some(TicketStatus::Cancelled)
                )
        })
        .count();

    let timing = TimingState {
        active_ticket_start,
        avg_cycle_time,
        remaining_count,
    };

    (timing, activity_events, completion_durations)
}

/// Detect state transitions between two HUD states for flash animations
pub fn diff_states(old: &HudState, new: &HudState) -> Vec<(String, FlashType)> {
    let mut transitions = Vec::new();

    // Build maps for quick lookup
    let old_statuses: HashMap<&str, TicketStatus> = old
        .tickets
        .iter()
        .filter_map(|t| {
            t.metadata
                .as_ref()
                .and_then(|m| m.status)
                .map(|s| (t.id.as_str(), s))
        })
        .collect();

    for ticket in &new.tickets {
        let new_status = ticket.metadata.as_ref().and_then(|m| m.status);
        let old_status = old_statuses.get(ticket.id.as_str()).copied();

        if new_status != old_status {
            if new_status == Some(TicketStatus::Complete) {
                transitions.push((ticket.id.clone(), FlashType::Completed));
            } else if new_status == Some(TicketStatus::InProgress)
                && old_status != Some(TicketStatus::InProgress)
            {
                transitions.push((ticket.id.clone(), FlashType::Started));
            }
        }
    }

    // Check for phase completions
    for (i, new_ps) in new.phase_statuses.iter().enumerate() {
        let old_ps = old.phase_statuses.get(i);
        if new_ps.status == TicketStatus::Complete
            && old_ps.is_some_and(|o| o.status != TicketStatus::Complete)
        {
            // Use phase number as key
            transitions.push((
                format!("phase-{}", new_ps.phase_number),
                FlashType::PhaseCompleted,
            ));
        }
    }

    transitions
}

// ============================================================================
// Formatting Helpers
// ============================================================================

/// Format an event into a human-readable description
fn format_event_description(event: &crate::events::types::Event) -> String {
    match event.event_type {
        EventType::StatusChanged => {
            let from = event
                .data
                .get("from")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let to = event.data.get("to").and_then(|v| v.as_str()).unwrap_or("?");
            format!("status: {from} -> {to}")
        }
        EventType::FieldUpdated => {
            let field = event
                .data
                .get("field")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let to = event
                .data
                .get("new_value")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("{field} -> {to}")
        }
        EventType::NoteAdded => "note added".to_string(),
        EventType::DependencyAdded => {
            let dep = event
                .data
                .get("depends_on")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("dep added: {dep}")
        }
        EventType::DependencyRemoved => {
            let dep = event
                .data
                .get("depends_on")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("dep removed: {dep}")
        }
        EventType::LabelAdded => {
            let label = event
                .data
                .get("label")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("label +{label}")
        }
        EventType::LabelRemoved => {
            let label = event
                .data
                .get("label")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("label -{label}")
        }
        EventType::LinkAdded => "link added".to_string(),
        EventType::LinkRemoved => "link removed".to_string(),
        EventType::TicketCreated => "created".to_string(),
        _ => format!("{}", event.event_type),
    }
}

/// Format an ISO timestamp to HH:MM:SS for display
fn format_event_time(timestamp: &str) -> String {
    // Timestamp format: "2024-01-15T10:30:00.123Z"
    // Extract HH:MM:SS
    if let Some(t_pos) = timestamp.find('T') {
        let time_part = &timestamp[t_pos + 1..];
        if time_part.len() >= 8 {
            return time_part[..8].to_string();
        }
    }
    timestamp.to_string()
}

/// Parse duration between two ISO timestamps
fn parse_duration_between(start: &str, end: &str) -> Option<std::time::Duration> {
    use jiff::Timestamp;
    let start_ts = start.parse::<Timestamp>().ok()?;
    let end_ts = end.parse::<Timestamp>().ok()?;
    let signed_dur = end_ts.duration_since(start_ts);
    if signed_dur.is_negative() {
        return None;
    }
    Some(signed_dur.unsigned_abs())
}

/// Format a duration for human display
pub fn format_duration(duration: std::time::Duration) -> String {
    let total_secs = duration.as_secs();
    if total_secs < 60 {
        format!("{total_secs}s")
    } else if total_secs < 3600 {
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        if secs == 0 {
            format!("{mins}m")
        } else {
            format!("{mins}m {secs}s")
        }
    } else {
        let hours = total_secs / 3600;
        let mins = (total_secs % 3600) / 60;
        if mins == 0 {
            format!("{hours}h")
        } else {
            format!("{hours}h {mins}m")
        }
    }
}

/// Format a duration since an ISO timestamp
pub fn duration_since_timestamp(timestamp: &str) -> Option<std::time::Duration> {
    use jiff::Timestamp;
    let ts = timestamp.parse::<Timestamp>().ok()?;
    let now = Timestamp::now();
    let signed_dur = now.duration_since(ts);
    if signed_dur.is_negative() {
        return None;
    }
    Some(signed_dur.unsigned_abs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration_seconds() {
        assert_eq!(format_duration(std::time::Duration::from_secs(30)), "30s");
        assert_eq!(format_duration(std::time::Duration::from_secs(0)), "0s");
    }

    #[test]
    fn test_format_duration_minutes() {
        assert_eq!(format_duration(std::time::Duration::from_secs(120)), "2m");
        assert_eq!(
            format_duration(std::time::Duration::from_secs(150)),
            "2m 30s"
        );
    }

    #[test]
    fn test_format_duration_hours() {
        assert_eq!(format_duration(std::time::Duration::from_secs(3600)), "1h");
        assert_eq!(
            format_duration(std::time::Duration::from_secs(5400)),
            "1h 30m"
        );
    }

    #[test]
    fn test_format_event_time() {
        assert_eq!(format_event_time("2024-01-15T10:30:00.123Z"), "10:30:00");
        assert_eq!(format_event_time("2024-01-15T23:59:59.000Z"), "23:59:59");
    }

    #[test]
    fn test_parse_duration_between() {
        let dur = parse_duration_between("2024-01-15T10:00:00.000Z", "2024-01-15T10:30:00.000Z");
        assert!(dur.is_some());
        let d = dur.unwrap();
        assert_eq!(d.as_secs(), 1800); // 30 minutes
    }

    #[test]
    fn test_parse_duration_between_reverse_returns_none() {
        let dur = parse_duration_between("2024-01-15T10:30:00.000Z", "2024-01-15T10:00:00.000Z");
        assert!(dur.is_none());
    }

    #[test]
    fn test_timing_estimated_remaining() {
        let timing = TimingState {
            active_ticket_start: None,
            avg_cycle_time: Some(std::time::Duration::from_secs(600)), // 10min avg
            remaining_count: 5,
        };
        let est = timing.estimated_remaining().unwrap();
        assert_eq!(est.as_secs(), 3000); // 50min
    }

    #[test]
    fn test_timing_estimated_remaining_no_data() {
        let timing = TimingState {
            active_ticket_start: None,
            avg_cycle_time: None,
            remaining_count: 5,
        };
        assert!(timing.estimated_remaining().is_none());
    }

    /// Helper to build a minimal HudState for scroll row tests
    fn make_test_state(
        is_simple: bool,
        ticket_count: usize,
        phase_tickets: Vec<Vec<usize>>,
        phase_statuses: Vec<PhaseStatus>,
    ) -> HudState {
        let tickets: Vec<HudTicket> = (0..ticket_count)
            .map(|i| HudTicket {
                id: format!("t-{i}"),
                metadata: Some(TicketMetadata {
                    status: Some(TicketStatus::New),
                    ..Default::default()
                }),
                phase_index: None,
                is_active: false,
                completion_duration: None,
            })
            .collect();

        // Build a plan with phases if not simple
        let mut plan = PlanMetadata::default();
        if !is_simple {
            for (i, _) in phase_statuses.iter().enumerate() {
                let phase_ticket_ids: Vec<String> = phase_tickets
                    .get(i)
                    .map(|indices| indices.iter().map(|&idx| format!("t-{idx}")).collect())
                    .unwrap_or_default();
                let mut phase = crate::plan::types::Phase::new(
                    format!("{}", i + 1),
                    format!("Phase {}", i + 1),
                );
                phase.ticket_list = crate::plan::types::TicketList::new(phase_ticket_ids);
                plan.sections.push(PlanSection::Phase(phase));
            }
        }

        HudState {
            plan,
            plan_status: PlanStatus {
                status: TicketStatus::New,
                completed_count: 0,
                total_count: ticket_count,
            },
            phase_statuses,
            tickets,
            phase_tickets,
            active_ticket_ids: vec![],
            activity_events: vec![],
            timing: TimingState {
                active_ticket_start: None,
                avg_cycle_time: None,
                remaining_count: ticket_count,
            },
            is_simple,
        }
    }

    #[test]
    fn test_build_scroll_rows_simple_plan() {
        let state = make_test_state(true, 3, vec![vec![0, 1, 2]], vec![]);
        let rows = build_scroll_rows(&state, false);
        assert_eq!(rows.len(), 3);
        assert!(matches!(rows[0], ScrollRow::Ticket { ticket_idx: 0 }));
        assert!(matches!(rows[1], ScrollRow::Ticket { ticket_idx: 1 }));
        assert!(matches!(rows[2], ScrollRow::Ticket { ticket_idx: 2 }));
    }

    #[test]
    fn test_build_scroll_rows_phased_plan() {
        let phase_statuses = vec![
            PhaseStatus {
                phase_number: "1".to_string(),
                phase_name: "Phase 1".to_string(),
                status: TicketStatus::New,
                completed_count: 0,
                total_count: 2,
            },
            PhaseStatus {
                phase_number: "2".to_string(),
                phase_name: "Phase 2".to_string(),
                status: TicketStatus::New,
                completed_count: 0,
                total_count: 1,
            },
        ];
        let state = make_test_state(false, 3, vec![vec![0, 1], vec![2]], phase_statuses);
        let rows = build_scroll_rows(&state, false);
        // PhaseHeader(0), Ticket(0), Ticket(1), PhaseHeader(1), Ticket(2)
        assert_eq!(rows.len(), 5);
        assert!(matches!(rows[0], ScrollRow::PhaseHeader { phase_idx: 0 }));
        assert!(matches!(rows[1], ScrollRow::Ticket { ticket_idx: 0 }));
        assert!(matches!(rows[2], ScrollRow::Ticket { ticket_idx: 1 }));
        assert!(matches!(rows[3], ScrollRow::PhaseHeader { phase_idx: 1 }));
        assert!(matches!(rows[4], ScrollRow::Ticket { ticket_idx: 2 }));
    }

    #[test]
    fn test_build_scroll_rows_compact_hides_completed_phase_tickets() {
        let phase_statuses = vec![
            PhaseStatus {
                phase_number: "1".to_string(),
                phase_name: "Phase 1".to_string(),
                status: TicketStatus::Complete,
                completed_count: 2,
                total_count: 2,
            },
            PhaseStatus {
                phase_number: "2".to_string(),
                phase_name: "Phase 2".to_string(),
                status: TicketStatus::New,
                completed_count: 0,
                total_count: 1,
            },
        ];
        let state = make_test_state(false, 3, vec![vec![0, 1], vec![2]], phase_statuses);
        let rows = build_scroll_rows(&state, true); // compact mode
        // Phase 1 is complete+non-active, so tickets hidden:
        // PhaseHeader(0), PhaseHeader(1), Ticket(2)
        assert_eq!(rows.len(), 3);
        assert!(matches!(rows[0], ScrollRow::PhaseHeader { phase_idx: 0 }));
        assert!(matches!(rows[1], ScrollRow::PhaseHeader { phase_idx: 1 }));
        assert!(matches!(rows[2], ScrollRow::Ticket { ticket_idx: 2 }));
    }

    #[test]
    fn test_scroll_row_ticket_idx() {
        let header = ScrollRow::PhaseHeader { phase_idx: 0 };
        let ticket = ScrollRow::Ticket { ticket_idx: 5 };
        assert_eq!(header.ticket_idx(), None);
        assert_eq!(ticket.ticket_idx(), Some(5));
    }
}

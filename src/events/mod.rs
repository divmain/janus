//! Event logging module for tracking mutations in Janus
//!
//! This module provides functionality for logging mutation events to
//! `.janus/events.ndjson`. Events are appended atomically to support
//! concurrent access from multiple processes.
//!
//! # Event Log Format
//!
//! Events are stored as Newline Delimited JSON (NDJSON), where each line
//! is a complete JSON object representing a single event. This format is
//! efficient for append operations and easy to process with standard tools.
//!
//! # Usage
//!
//! ```ignore
//! use janus::events::{log_event, Event, EventType, EntityType};
//! use serde_json::json;
//!
//! // Log a ticket creation event
//! log_event(Event::new(
//!     EventType::TicketCreated,
//!     EntityType::Ticket,
//!     "j-a1b2",
//!     json!({"title": "New feature", "priority": 1}),
//! ));
//! ```

pub mod types;

pub use types::{Actor, EntityType, Event, EventType};

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use crate::types::janus_root;

/// The name of the events log file
const EVENTS_FILE: &str = "events.ndjson";

/// Get the path to the events log file
pub fn events_file_path() -> PathBuf {
    janus_root().join(EVENTS_FILE)
}

/// Log an event to the events log file
///
/// This function appends the event as a JSON line to `.janus/events.ndjson`.
/// The append is performed with `O_APPEND`, which is atomic for writes up to
/// `PIPE_BUF` bytes on POSIX systems. Concurrent appenders follow
/// last-writer-wins ordering — no advisory locking is performed.
///
/// # Errors
///
/// This function logs errors to stderr rather than returning them, since
/// event logging is a secondary concern and should not fail the primary
/// operation. A warning is printed if:
/// - The events file cannot be created or opened
/// - The event cannot be serialized
/// - The write operation fails
pub fn log_event(event: Event) {
    if let Err(e) = log_event_impl(event) {
        eprintln!("Warning: failed to log event: {e}");
    }
}

/// Internal implementation that returns errors for testing
fn log_event_impl(event: Event) -> std::io::Result<()> {
    let path = events_file_path();

    // Ensure the .janus directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to create directory for events at {}: {}",
                    parent.display(),
                    e
                ),
            )
        })?;
    }

    // Open file in append mode, creating if necessary
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| {
            std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to open events file at {}: {}",
                    crate::utils::format_relative_path(&path),
                    e
                ),
            )
        })?;

    // Serialize event to JSON (single line, no pretty printing)
    let json = serde_json::to_string(&event)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    // Write the event followed by newline
    writeln!(file, "{json}")?;

    // Ensure data is flushed to disk
    file.flush()?;

    Ok(())
}

/// Read all events from the event log
///
/// Returns a vector of events in chronological order (oldest first).
/// Invalid JSON lines are skipped with a warning.
///
/// # Errors
///
/// Returns an error if the events file cannot be read. Returns an empty
/// vector if the file doesn't exist.
pub fn read_events() -> std::io::Result<Vec<Event>> {
    let path = events_file_path();

    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(&path).map_err(|e| {
        std::io::Error::new(
            e.kind(),
            format!(
                "Failed to read events file at {}: {}",
                crate::utils::format_relative_path(&path),
                e
            ),
        )
    })?;
    let mut events = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }

        match serde_json::from_str::<Event>(line) {
            Ok(event) => events.push(event),
            Err(e) => {
                eprintln!(
                    "Warning: failed to parse event on line {}: {}",
                    line_num + 1,
                    e
                );
            }
        }
    }

    Ok(events)
}

/// Clear the events log
///
/// Removes the events log file if it exists.
///
/// # Errors
///
/// Returns an error if the file exists but cannot be removed.
pub fn clear_events() -> std::io::Result<()> {
    let path = events_file_path();
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| {
            std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to remove events file at {}: {}",
                    crate::utils::format_relative_path(&path),
                    e
                ),
            )
        })?;
    }
    Ok(())
}

// ============================================================================
// Helper functions for creating common events
// ============================================================================

/// Log a ticket creation event
pub fn log_ticket_created(
    ticket_id: &str,
    title: &str,
    ticket_type: &str,
    priority: u8,
    spawned_from: Option<&str>,
) {
    let mut data = serde_json::json!({
        "title": title,
        "type": ticket_type,
        "priority": priority,
    });

    if let Some(parent_id) = spawned_from {
        data["spawned_from"] = serde_json::Value::String(parent_id.to_string());
    }

    log_event(Event::new(
        EventType::TicketCreated,
        EntityType::Ticket,
        ticket_id,
        data,
    ));
}

/// Log a status change event
pub fn log_status_changed(ticket_id: &str, from: &str, to: &str, summary: Option<&str>) {
    let mut data = serde_json::json!({
        "from": from,
        "to": to,
    });

    if let Some(s) = summary {
        data["summary"] = serde_json::Value::String(s.to_string());
    }

    log_event(Event::new(
        EventType::StatusChanged,
        EntityType::Ticket,
        ticket_id,
        data,
    ));
}

/// Log a note added event
pub fn log_note_added(ticket_id: &str, note_preview: &str) {
    // Truncate note to first 100 chars for preview
    let preview = if note_preview.len() > 100 {
        let end = note_preview
            .char_indices()
            .nth(97)
            .map(|(i, _)| i)
            .unwrap_or(note_preview.len());
        format!("{}...", &note_preview[..end])
    } else {
        note_preview.to_string()
    };

    log_event(Event::new(
        EventType::NoteAdded,
        EntityType::Ticket,
        ticket_id,
        serde_json::json!({
            "content_preview": preview,
        }),
    ));
}

/// Log a field update event
pub fn log_field_updated(ticket_id: &str, field: &str, old_value: Option<&str>, new_value: &str) {
    log_event(Event::new(
        EventType::FieldUpdated,
        EntityType::Ticket,
        ticket_id,
        serde_json::json!({
            "field": field,
            "old_value": old_value,
            "new_value": new_value,
        }),
    ));
}

/// Log a dependency added event
pub fn log_dependency_added(ticket_id: &str, dep_id: &str) {
    log_event(Event::new(
        EventType::DependencyAdded,
        EntityType::Ticket,
        ticket_id,
        serde_json::json!({
            "dependency_id": dep_id,
        }),
    ));
}

/// Log a dependency removed event
pub fn log_dependency_removed(ticket_id: &str, dep_id: &str) {
    log_event(Event::new(
        EventType::DependencyRemoved,
        EntityType::Ticket,
        ticket_id,
        serde_json::json!({
            "dependency_id": dep_id,
        }),
    ));
}

/// Log a link added event
pub fn log_link_added(ticket_id: &str, linked_id: &str) {
    log_event(Event::new(
        EventType::LinkAdded,
        EntityType::Ticket,
        ticket_id,
        serde_json::json!({
            "linked_id": linked_id,
        }),
    ));
}

/// Log a link removed event
pub fn log_link_removed(ticket_id: &str, linked_id: &str) {
    log_event(Event::new(
        EventType::LinkRemoved,
        EntityType::Ticket,
        ticket_id,
        serde_json::json!({
            "linked_id": linked_id,
        }),
    ));
}

/// Log a plan creation event
pub fn log_plan_created(plan_id: &str, title: &str, is_phased: bool, phases: &[String]) {
    log_event(Event::new(
        EventType::PlanCreated,
        EntityType::Plan,
        plan_id,
        serde_json::json!({
            "title": title,
            "is_phased": is_phased,
            "phases": phases,
        }),
    ));
}

/// Log a ticket added to plan event
pub fn log_ticket_added_to_plan(plan_id: &str, ticket_id: &str, phase: Option<&str>) {
    let mut data = serde_json::json!({
        "ticket_id": ticket_id,
    });

    if let Some(p) = phase {
        data["phase"] = serde_json::Value::String(p.to_string());
    }

    log_event(Event::new(
        EventType::TicketAddedToPlan,
        EntityType::Plan,
        plan_id,
        data,
    ));
}

/// Log a ticket removed from plan event
pub fn log_ticket_removed_from_plan(plan_id: &str, ticket_id: &str, phase: Option<&str>) {
    let mut data = serde_json::json!({
        "ticket_id": ticket_id,
    });

    if let Some(p) = phase {
        data["phase"] = serde_json::Value::String(p.to_string());
    }

    log_event(Event::new(
        EventType::TicketRemovedFromPlan,
        EntityType::Plan,
        plan_id,
        data,
    ));
}

/// Log a phase added event
pub fn log_phase_added(plan_id: &str, phase_number: &str, phase_name: &str) {
    log_event(Event::new(
        EventType::PhaseAdded,
        EntityType::Plan,
        plan_id,
        serde_json::json!({
            "phase_number": phase_number,
            "phase_name": phase_name,
        }),
    ));
}

/// Log a phase removed event
pub fn log_phase_removed(
    plan_id: &str,
    phase_number: &str,
    phase_name: &str,
    migrated_tickets: usize,
) {
    log_event(Event::new(
        EventType::PhaseRemoved,
        EntityType::Plan,
        plan_id,
        serde_json::json!({
            "phase_number": phase_number,
            "phase_name": phase_name,
            "migrated_tickets": migrated_tickets,
        }),
    ));
}

/// Log a ticket moved event (between phases)
pub fn log_ticket_moved(plan_id: &str, ticket_id: &str, from_phase: &str, to_phase: &str) {
    log_event(Event::new(
        EventType::TicketMoved,
        EntityType::Plan,
        plan_id,
        serde_json::json!({
            "ticket_id": ticket_id,
            "from_phase": from_phase,
            "to_phase": to_phase,
        }),
    ));
}

/// Log a store rebuilt event
///
/// This function logs detailed information about why the store was rebuilt,
/// including the reason, duration, and additional context to help debug
/// issues where store regeneration happens unexpectedly.
pub fn log_cache_rebuilt(
    reason: &str,
    trigger: &str,
    duration_ms: Option<u64>,
    ticket_count: Option<usize>,
    details: Option<serde_json::Value>,
) {
    let mut data = serde_json::json!({
        "reason": reason,
        "trigger": trigger,
    });

    if let Some(ms) = duration_ms {
        data["duration_ms"] = serde_json::Value::Number(serde_json::Number::from(ms));
    }

    if let Some(count) = ticket_count {
        data["ticket_count"] = serde_json::Value::Number(serde_json::Number::from(count));
    }

    if let Some(d) = details {
        data["details"] = d;
    }

    log_event(Event::new(
        EventType::CacheRebuilt,
        EntityType::Cache,
        "cache",
        data,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use crate::paths::JanusRootGuard;

    /// Helper to create a temporary test directory.
    /// Returns `(TempDir, JanusRootGuard)` — both must be held for the test's lifetime.
    fn setup_test_dir() -> (tempfile::TempDir, JanusRootGuard) {
        let temp_dir = tempfile::tempdir().unwrap();
        let janus_dir = temp_dir.path().join(".janus");
        fs::create_dir_all(&janus_dir).unwrap();
        let guard = JanusRootGuard::new(&janus_dir);
        (temp_dir, guard)
    }

    #[test]
    fn test_log_and_read_events() {
        let (_temp, _guard) = setup_test_dir();

        // Log some events
        log_event(Event::new(
            EventType::TicketCreated,
            EntityType::Ticket,
            "j-test1",
            serde_json::json!({"title": "Test 1"}),
        ));

        log_event(Event::new(
            EventType::StatusChanged,
            EntityType::Ticket,
            "j-test1",
            serde_json::json!({"from": "new", "to": "in_progress"}),
        ));

        // Read events back
        let events = read_events().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_type, EventType::TicketCreated);
        assert_eq!(events[0].entity_id, "j-test1");
        assert_eq!(events[1].event_type, EventType::StatusChanged);
    }

    #[test]
    fn test_clear_events() {
        let (_temp, _guard) = setup_test_dir();

        // Log an event
        log_event(Event::new(
            EventType::TicketCreated,
            EntityType::Ticket,
            "j-test",
            serde_json::json!({}),
        ));

        assert!(events_file_path().exists());

        // Clear events
        clear_events().unwrap();

        assert!(!events_file_path().exists());
        assert!(read_events().unwrap().is_empty());
    }

    #[test]
    fn test_ndjson_format() {
        let (_temp, _guard) = setup_test_dir();

        // Log two events
        log_event(Event::new(
            EventType::TicketCreated,
            EntityType::Ticket,
            "j-1",
            serde_json::json!({}),
        ));
        log_event(Event::new(
            EventType::PlanCreated,
            EntityType::Plan,
            "plan-1",
            serde_json::json!({}),
        ));

        // Read raw content
        let content = fs::read_to_string(events_file_path()).unwrap();
        let lines: Vec<&str> = content.lines().collect();

        // Should be exactly 2 lines
        assert_eq!(lines.len(), 2);

        // Each line should be valid JSON
        for line in lines {
            assert!(serde_json::from_str::<Event>(line).is_ok());
        }
    }

    #[test]
    fn test_helper_functions() {
        let (_temp, _guard) = setup_test_dir();

        log_ticket_created("j-test", "Test ticket", "task", 2, None);
        log_status_changed("j-test", "new", "complete", Some("Done!"));
        log_note_added("j-test", "This is a note");
        log_field_updated("j-test", "priority", Some("2"), "1");
        log_dependency_added("j-test", "j-other");
        log_dependency_removed("j-test", "j-other");
        log_link_added("j-test", "j-linked");
        log_link_removed("j-test", "j-linked");
        log_plan_created("plan-1", "Test Plan", true, &["Phase 1".to_string()]);
        log_ticket_added_to_plan("plan-1", "j-test", Some("Phase 1"));
        log_ticket_removed_from_plan("plan-1", "j-test", Some("Phase 1"));
        log_phase_added("plan-1", "2", "Phase 2");
        log_phase_removed("plan-1", "1", "Phase 1", 2);
        log_ticket_moved("plan-1", "j-test", "Phase 1", "Phase 2");

        let events = read_events().unwrap();
        assert_eq!(events.len(), 14);
    }

    #[test]
    fn test_log_cache_rebuilt() {
        let (_temp, _guard) = setup_test_dir();

        // Log a store rebuilt event with all fields
        log_cache_rebuilt(
            "version_mismatch",
            "automatic_schema_update",
            Some(150),
            Some(42),
            Some(serde_json::json!({
                "old_version": "12",
                "new_version": "13",
            })),
        );

        // Log a minimal store rebuilt event
        log_cache_rebuilt(
            "corruption_recovery",
            "automatic_recovery",
            None,
            None,
            None,
        );

        let events = read_events().unwrap();
        assert_eq!(events.len(), 2);

        // Verify first event
        assert_eq!(events[0].event_type, EventType::CacheRebuilt);
        assert_eq!(events[0].entity_type, EntityType::Cache);
        assert_eq!(events[0].entity_id, "cache");
        assert_eq!(events[0].data["reason"], "version_mismatch");
        assert_eq!(events[0].data["trigger"], "automatic_schema_update");
        assert_eq!(events[0].data["duration_ms"], 150);
        assert_eq!(events[0].data["ticket_count"], 42);
        assert_eq!(events[0].data["details"]["old_version"], "12");
        assert_eq!(events[0].data["details"]["new_version"], "13");

        // Verify second event
        assert_eq!(events[1].event_type, EventType::CacheRebuilt);
        assert_eq!(events[1].data["reason"], "corruption_recovery");
        assert_eq!(events[1].data["trigger"], "automatic_recovery");
        assert!(
            !events[1]
                .data
                .as_object()
                .unwrap()
                .contains_key("duration_ms")
        );
        assert!(
            !events[1]
                .data
                .as_object()
                .unwrap()
                .contains_key("ticket_count")
        );
        assert!(!events[1].data.as_object().unwrap().contains_key("details"));
    }

    #[test]
    fn test_spawned_from_included() {
        let (_temp, _guard) = setup_test_dir();

        log_ticket_created("j-child", "Child ticket", "task", 2, Some("j-parent"));

        let events = read_events().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data["spawned_from"], "j-parent");
    }

    #[test]
    fn test_note_truncation() {
        let (_temp, _guard) = setup_test_dir();

        let long_note = "a".repeat(200);
        log_note_added("j-test", &long_note);

        let events = read_events().unwrap();
        let preview = events[0].data["content_preview"].as_str().unwrap();
        assert!(preview.len() <= 100);
        assert!(preview.ends_with("..."));
    }
}

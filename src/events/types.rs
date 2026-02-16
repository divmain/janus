//! Event logging types for tracking mutations in Janus
//!
//! This module defines the types used for the mutation event log stored at
//! `.janus/events.ndjson`. Events are logged for all write operations to
//! enable audit trails and external integrations.

use serde::{Deserialize, Serialize};

pub use crate::types::EntityType;

/// The type of event being logged
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    // Ticket events
    TicketCreated,
    StatusChanged,
    NoteAdded,
    FieldUpdated,
    DependencyAdded,
    DependencyRemoved,
    LinkAdded,
    LinkRemoved,

    // Plan events
    PlanCreated,
    TicketAddedToPlan,
    TicketRemovedFromPlan,
    PhaseAdded,
    PhaseRemoved,
    TicketMoved,

    // Cache events
    CacheRebuilt,
}

enum_display_fromstr!(
    EventType,
    crate::error::JanusError::invalid_event_type,
    ["ticket_created", "status_changed", "note_added", "field_updated", "dependency_added", "dependency_removed", "link_added", "link_removed", "plan_created", "ticket_added_to_plan", "ticket_removed_from_plan", "phase_added", "phase_removed", "ticket_moved", "cache_rebuilt"],
    {
        TicketCreated => "ticket_created",
        StatusChanged => "status_changed",
        NoteAdded => "note_added",
        FieldUpdated => "field_updated",
        DependencyAdded => "dependency_added",
        DependencyRemoved => "dependency_removed",
        LinkAdded => "link_added",
        LinkRemoved => "link_removed",
        PlanCreated => "plan_created",
        TicketAddedToPlan => "ticket_added_to_plan",
        TicketRemovedFromPlan => "ticket_removed_from_plan",
        PhaseAdded => "phase_added",
        PhaseRemoved => "phase_removed",
        TicketMoved => "ticket_moved",
        CacheRebuilt => "cache_rebuilt",
    }
);

/// The actor that triggered the event
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Actor {
    #[default]
    Cli,
    Mcp,
    Hook,
}

enum_display_fromstr!(
    Actor,
    crate::error::JanusError::invalid_actor,
    ["cli", "mcp", "hook"],
    {
        Cli => "cli",
        Mcp => "mcp",
        Hook => "hook",
    }
);

/// A mutation event record
///
/// Events are serialized as NDJSON (Newline Delimited JSON) and appended
/// to `.janus/events.ndjson`. Each event captures a single mutation operation
/// with full context for audit and integration purposes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// ISO 8601 timestamp with milliseconds
    pub timestamp: String,

    /// The type of event
    pub event_type: EventType,

    /// The type of entity being mutated
    pub entity_type: EntityType,

    /// The ID of the entity being mutated
    pub entity_id: String,

    /// The actor that triggered this event
    pub actor: Actor,

    /// Event-specific payload data
    pub data: serde_json::Value,
}

impl Event {
    /// Create a new event with the current timestamp
    pub fn new(
        event_type: EventType,
        entity_type: EntityType,
        entity_id: impl Into<String>,
        data: serde_json::Value,
    ) -> Self {
        Self {
            timestamp: iso_timestamp_millis(),
            event_type,
            entity_type,
            entity_id: entity_id.into(),
            actor: Actor::default(),
            data,
        }
    }

    /// Set the actor for this event
    pub fn with_actor(mut self, actor: Actor) -> Self {
        self.actor = actor;
        self
    }
}

/// Get the current timestamp in ISO 8601 format with milliseconds
fn iso_timestamp_millis() -> String {
    use jiff::Timestamp;
    let now = Timestamp::now();
    // Format as ISO 8601 with milliseconds
    now.strftime("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_event_type_serialization() {
        assert_eq!(
            serde_json::to_string(&EventType::TicketCreated).unwrap(),
            "\"ticket_created\""
        );
        assert_eq!(
            serde_json::to_string(&EventType::StatusChanged).unwrap(),
            "\"status_changed\""
        );
        assert_eq!(
            serde_json::to_string(&EventType::TicketAddedToPlan).unwrap(),
            "\"ticket_added_to_plan\""
        );
    }

    #[test]
    fn test_entity_type_serialization() {
        assert_eq!(
            serde_json::to_string(&EntityType::Ticket).unwrap(),
            "\"ticket\""
        );
        assert_eq!(
            serde_json::to_string(&EntityType::Plan).unwrap(),
            "\"plan\""
        );
    }

    #[test]
    fn test_actor_serialization() {
        assert_eq!(serde_json::to_string(&Actor::Cli).unwrap(), "\"cli\"");
        assert_eq!(serde_json::to_string(&Actor::Mcp).unwrap(), "\"mcp\"");
        assert_eq!(serde_json::to_string(&Actor::Hook).unwrap(), "\"hook\"");
    }

    #[test]
    fn test_event_creation() {
        let event = Event::new(
            EventType::TicketCreated,
            EntityType::Ticket,
            "j-a1b2",
            json!({"title": "Test ticket", "priority": 2}),
        );

        assert_eq!(event.event_type, EventType::TicketCreated);
        assert_eq!(event.entity_type, EntityType::Ticket);
        assert_eq!(event.entity_id, "j-a1b2");
        assert_eq!(event.actor, Actor::Cli); // Default actor
        assert!(event.timestamp.contains('T'));
        assert!(event.timestamp.ends_with('Z'));
    }

    #[test]
    fn test_event_with_actor() {
        let event = Event::new(
            EventType::TicketCreated,
            EntityType::Ticket,
            "j-a1b2",
            json!({}),
        )
        .with_actor(Actor::Mcp);

        assert_eq!(event.actor, Actor::Mcp);
    }

    #[test]
    fn test_event_json_serialization() {
        let event = Event::new(
            EventType::StatusChanged,
            EntityType::Ticket,
            "j-a1b2",
            json!({"from": "new", "to": "in_progress"}),
        );

        let json_str = serde_json::to_string(&event).unwrap();
        assert!(json_str.contains("\"event_type\":\"status_changed\""));
        assert!(json_str.contains("\"entity_type\":\"ticket\""));
        assert!(json_str.contains("\"entity_id\":\"j-a1b2\""));
        assert!(json_str.contains("\"actor\":\"cli\""));
    }

    #[test]
    fn test_event_json_deserialization() {
        let json_str = r#"{
            "timestamp": "2024-01-15T10:30:00.000Z",
            "event_type": "ticket_created",
            "entity_type": "ticket",
            "entity_id": "j-test",
            "actor": "cli",
            "data": {"title": "Test"}
        }"#;

        let event: Event = serde_json::from_str(json_str).unwrap();
        assert_eq!(event.event_type, EventType::TicketCreated);
        assert_eq!(event.entity_type, EntityType::Ticket);
        assert_eq!(event.entity_id, "j-test");
        assert_eq!(event.actor, Actor::Cli);
        assert_eq!(event.data["title"], "Test");
    }

    #[test]
    fn test_timestamp_has_milliseconds() {
        let ts = iso_timestamp_millis();
        // Should match pattern like "2024-01-15T10:30:00.123Z"
        // The milliseconds part should have 3 digits
        let parts: Vec<&str> = ts.split('.').collect();
        assert_eq!(parts.len(), 2);
        assert!(parts[1].len() >= 4); // 3 digits + "Z"
        assert!(parts[1].ends_with('Z'));
    }
}

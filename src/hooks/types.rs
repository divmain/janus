//! Hook types for the Janus hooks system.
//!
//! This module defines the core types used for hook events and contexts.

use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{JanusError, Result};

/// Events that can trigger hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEvent {
    /// Fired after a new ticket is created
    TicketCreated,
    /// Fired after an existing ticket is updated
    TicketUpdated,
    /// Fired after a ticket is deleted
    TicketDeleted,
    /// Fired after a new plan is created
    PlanCreated,
    /// Fired after an existing plan is updated
    PlanUpdated,
    /// Fired after a plan is deleted
    PlanDeleted,
    /// Fired before writing any item to disk (can abort)
    PreWrite,
    /// Fired after writing any item to disk
    PostWrite,
    /// Fired before deleting any item (can abort)
    PreDelete,
    /// Fired after deleting any item
    PostDelete,
}

impl HookEvent {
    /// Returns true if this is a pre-operation hook that can abort the operation.
    pub fn is_pre_hook(&self) -> bool {
        matches!(self, HookEvent::PreWrite | HookEvent::PreDelete)
    }

    /// Returns the event name as used in configuration and environment variables.
    pub fn as_str(&self) -> &'static str {
        match self {
            HookEvent::TicketCreated => "ticket_created",
            HookEvent::TicketUpdated => "ticket_updated",
            HookEvent::TicketDeleted => "ticket_deleted",
            HookEvent::PlanCreated => "plan_created",
            HookEvent::PlanUpdated => "plan_updated",
            HookEvent::PlanDeleted => "plan_deleted",
            HookEvent::PreWrite => "pre_write",
            HookEvent::PostWrite => "post_write",
            HookEvent::PreDelete => "pre_delete",
            HookEvent::PostDelete => "post_delete",
        }
    }

    /// Returns all possible hook events.
    pub fn all() -> &'static [HookEvent] {
        &[
            HookEvent::TicketCreated,
            HookEvent::TicketUpdated,
            HookEvent::TicketDeleted,
            HookEvent::PlanCreated,
            HookEvent::PlanUpdated,
            HookEvent::PlanDeleted,
            HookEvent::PreWrite,
            HookEvent::PostWrite,
            HookEvent::PreDelete,
            HookEvent::PostDelete,
        ]
    }
}

impl fmt::Display for HookEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for HookEvent {
    type Err = JanusError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "ticket_created" => Ok(HookEvent::TicketCreated),
            "ticket_updated" => Ok(HookEvent::TicketUpdated),
            "ticket_deleted" => Ok(HookEvent::TicketDeleted),
            "plan_created" => Ok(HookEvent::PlanCreated),
            "plan_updated" => Ok(HookEvent::PlanUpdated),
            "plan_deleted" => Ok(HookEvent::PlanDeleted),
            "pre_write" => Ok(HookEvent::PreWrite),
            "post_write" => Ok(HookEvent::PostWrite),
            "pre_delete" => Ok(HookEvent::PreDelete),
            "post_delete" => Ok(HookEvent::PostDelete),
            _ => Err(JanusError::InvalidHookEvent(s.to_string())),
        }
    }
}

/// The type of item being operated on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemType {
    Ticket,
    Plan,
}

impl fmt::Display for ItemType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ItemType::Ticket => write!(f, "ticket"),
            ItemType::Plan => write!(f, "plan"),
        }
    }
}

/// Context passed to hook scripts via environment variables.
#[derive(Debug, Clone, Default)]
pub struct HookContext {
    /// The event that triggered this hook
    pub event: Option<HookEvent>,
    /// The type of item being operated on
    pub item_type: Option<ItemType>,
    /// The ID of the item being operated on
    pub item_id: Option<String>,
    /// The file path of the item
    pub file_path: Option<PathBuf>,
    /// The field name being modified (for updates)
    pub field_name: Option<String>,
    /// The old value (for updates)
    pub old_value: Option<String>,
    /// The new value (for updates)
    pub new_value: Option<String>,
}

impl HookContext {
    /// Create a new empty hook context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the event.
    pub fn with_event(mut self, event: HookEvent) -> Self {
        self.event = Some(event);
        self
    }

    /// Set the item type.
    pub fn with_item_type(mut self, item_type: ItemType) -> Self {
        self.item_type = Some(item_type);
        self
    }

    /// Set the item ID.
    pub fn with_item_id(mut self, item_id: impl Into<String>) -> Self {
        self.item_id = Some(item_id.into());
        self
    }

    /// Set the file path.
    pub fn with_file_path(mut self, file_path: impl Into<PathBuf>) -> Self {
        self.file_path = Some(file_path.into());
        self
    }

    /// Set the field name.
    pub fn with_field_name(mut self, field_name: impl Into<String>) -> Self {
        self.field_name = Some(field_name.into());
        self
    }

    /// Set the old value.
    pub fn with_old_value(mut self, old_value: impl Into<String>) -> Self {
        self.old_value = Some(old_value.into());
        self
    }

    /// Set the new value.
    pub fn with_new_value(mut self, new_value: impl Into<String>) -> Self {
        self.new_value = Some(new_value.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_event_display() {
        assert_eq!(HookEvent::TicketCreated.to_string(), "ticket_created");
        assert_eq!(HookEvent::PreWrite.to_string(), "pre_write");
        assert_eq!(HookEvent::PostDelete.to_string(), "post_delete");
    }

    #[test]
    fn test_hook_event_from_str() {
        assert_eq!(
            "ticket_created".parse::<HookEvent>().unwrap(),
            HookEvent::TicketCreated
        );
        assert_eq!(
            "PRE_WRITE".parse::<HookEvent>().unwrap(),
            HookEvent::PreWrite
        );
        assert!("invalid".parse::<HookEvent>().is_err());
    }

    #[test]
    fn test_hook_event_is_pre_hook() {
        assert!(HookEvent::PreWrite.is_pre_hook());
        assert!(HookEvent::PreDelete.is_pre_hook());
        assert!(!HookEvent::PostWrite.is_pre_hook());
        assert!(!HookEvent::TicketCreated.is_pre_hook());
    }

    #[test]
    fn test_hook_event_all() {
        let all = HookEvent::all();
        assert_eq!(all.len(), 10);
        assert!(all.contains(&HookEvent::TicketCreated));
        assert!(all.contains(&HookEvent::PostDelete));
    }

    #[test]
    fn test_item_type_display() {
        assert_eq!(ItemType::Ticket.to_string(), "ticket");
        assert_eq!(ItemType::Plan.to_string(), "plan");
    }

    #[test]
    fn test_hook_context_builder() {
        let ctx = HookContext::new()
            .with_event(HookEvent::TicketCreated)
            .with_item_type(ItemType::Ticket)
            .with_item_id("j-1234")
            .with_file_path("/path/to/file.md")
            .with_field_name("status")
            .with_old_value("new")
            .with_new_value("complete");

        assert_eq!(ctx.event, Some(HookEvent::TicketCreated));
        assert_eq!(ctx.item_type, Some(ItemType::Ticket));
        assert_eq!(ctx.item_id, Some("j-1234".to_string()));
        assert_eq!(ctx.file_path, Some(PathBuf::from("/path/to/file.md")));
        assert_eq!(ctx.field_name, Some("status".to_string()));
        assert_eq!(ctx.old_value, Some("new".to_string()));
        assert_eq!(ctx.new_value, Some("complete".to_string()));
    }

    #[test]
    fn test_invalid_hook_event_error() {
        let result = "not_a_valid_event".parse::<HookEvent>();
        match result {
            Err(JanusError::InvalidHookEvent(event)) => {
                assert_eq!(event, "not_a_valid_event");
            }
            other => panic!("Expected InvalidHookEvent, got: {:?}", other),
        }
    }

    #[test]
    fn test_hook_event_from_str_all_variants() {
        // Test all valid event strings
        let events = [
            ("ticket_created", HookEvent::TicketCreated),
            ("ticket_updated", HookEvent::TicketUpdated),
            ("ticket_deleted", HookEvent::TicketDeleted),
            ("plan_created", HookEvent::PlanCreated),
            ("plan_updated", HookEvent::PlanUpdated),
            ("plan_deleted", HookEvent::PlanDeleted),
            ("pre_write", HookEvent::PreWrite),
            ("post_write", HookEvent::PostWrite),
            ("pre_delete", HookEvent::PreDelete),
            ("post_delete", HookEvent::PostDelete),
        ];

        for (s, expected) in events {
            let result: HookEvent = s.parse().unwrap();
            assert_eq!(result, expected);

            // Also test uppercase
            let upper_result: HookEvent = s.to_uppercase().parse().unwrap();
            assert_eq!(upper_result, expected);
        }
    }

    #[test]
    fn test_invalid_hook_event_variants() {
        let invalid_events = [
            "",
            "invalid",
            "ticket",
            "pre",
            "post",
            "created",
            "ticket-created", // Wrong separator
            "ticketcreated",  // No separator
        ];

        for invalid in invalid_events {
            let result = invalid.parse::<HookEvent>();
            assert!(
                matches!(result, Err(JanusError::InvalidHookEvent(_))),
                "Expected InvalidHookEvent for '{}'",
                invalid
            );
        }
    }
}

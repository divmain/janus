//! Shared edit form state management
//!
//! Provides types and utilities for managing the edit form state
//! used across IssueBrowser and KanbanBoard components.

use iocraft::prelude::State;

use crate::types::TicketMetadata;

use super::edit::EditResult;

/// The editing mode state machine
///
/// This enum represents the three possible states of the edit form:
/// - `None`: No editing in progress
/// - `Creating`: Creating a new ticket
/// - `Editing`: Editing an existing ticket
///
/// Note: `TicketMetadata` is boxed to reduce enum size since the `Editing` variant
/// is much larger than `None` and `Creating`.
#[derive(Clone, Default)]
pub enum EditMode {
    #[default]
    None,
    Creating {
        body: String,
    },
    Editing {
        ticket: Box<TicketMetadata>,
        body: String,
    },
}

/// Holds all the state needed for the edit form
pub struct EditFormState<'a> {
    pub mode: &'a mut State<EditMode>,
    pub result: &'a mut State<EditResult>,
}

impl EditFormState<'_> {
    /// Check if the edit form is currently open
    pub fn is_editing(&self) -> bool {
        !matches!(*self.mode.read(), EditMode::None)
    }

    /// Check if we're creating a new ticket (vs editing existing)
    pub fn is_creating_new(&self) -> bool {
        matches!(*self.mode.read(), EditMode::Creating { .. })
    }

    /// Reset all edit state to defaults
    pub fn reset(&mut self) {
        self.mode.set(EditMode::None);
    }

    /// Handle the edit result, returning the ticket ID if a granular refresh is needed.
    ///
    /// Returns:
    /// - `Some(ticket_id)` if an existing ticket was saved (needs granular UI update)
    /// - `Some(special marker)` if a new ticket was created (needs full reload)
    /// - `None` if no refresh is needed (cancelled or still editing)
    pub fn handle_result(&mut self) -> Option<String> {
        match self.result.get() {
            EditResult::Saved => {
                // Check if we're creating a new ticket or editing existing
                let is_creating = self.is_creating_new();

                // Get the ticket ID before resetting the state
                let ticket_id = self
                    .get_edit_ticket()
                    .and_then(|t| t.id.map(|id| id.to_string()));

                self.result.set(EditResult::Editing);
                self.reset();

                // For existing tickets, return the ID for granular refresh
                // For new tickets, return a special marker to trigger full reload
                if is_creating {
                    Some("__NEW_TICKET__".to_string())
                } else {
                    ticket_id
                }
            }
            EditResult::Cancelled => {
                self.result.set(EditResult::Editing);
                self.reset();
                None
            }
            EditResult::Editing => None,
        }
    }

    /// Start editing an existing ticket
    pub fn start_edit(&mut self, ticket: TicketMetadata, body: String) {
        self.mode.set(EditMode::Editing {
            ticket: Box::new(ticket),
            body,
        });
    }

    /// Start creating a new ticket
    pub fn start_create(&mut self) {
        self.mode.set(EditMode::Creating {
            body: String::new(),
        });
    }

    /// Get the ticket being edited (if editing existing)
    pub fn get_edit_ticket(&self) -> Option<TicketMetadata> {
        match &*self.mode.read() {
            EditMode::Editing { ticket, .. } => Some((**ticket).clone()),
            _ => None,
        }
    }

    /// Get the body for the edit form
    pub fn get_edit_body(&self) -> Option<String> {
        match &*self.mode.read() {
            EditMode::Editing { body, .. } | EditMode::Creating { body } => Some(body.clone()),
            EditMode::None => None,
        }
    }
}

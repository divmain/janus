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

    /// Handle the edit result, returning true if reload is needed
    pub fn handle_result(&mut self) -> bool {
        match self.result.get() {
            EditResult::Saved => {
                self.result.set(EditResult::Editing);
                self.reset();
                true // needs reload
            }
            EditResult::Cancelled => {
                self.result.set(EditResult::Editing);
                self.reset();
                false
            }
            EditResult::Editing => false,
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

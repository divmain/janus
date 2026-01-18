//! Shared edit form state management
//!
//! Provides types and utilities for managing the edit form state
//! used across IssueBrowser and KanbanBoard components.

use iocraft::prelude::State;

use crate::types::TicketMetadata;

use super::edit::EditResult;

/// Holds all the state needed for the edit form
pub struct EditFormState<'a> {
    pub result: &'a mut State<EditResult>,
    pub is_editing_existing: &'a mut State<bool>,
    pub is_creating_new: &'a mut State<bool>,
    pub editing_ticket: &'a mut State<TicketMetadata>,
    pub editing_body: &'a mut State<String>,
}

impl EditFormState<'_> {
    /// Check if the edit form is currently open
    pub fn is_editing(&self) -> bool {
        self.is_editing_existing.get() || self.is_creating_new.get()
    }

    /// Reset all edit state to defaults
    pub fn reset(&mut self) {
        self.is_editing_existing.set(false);
        self.is_creating_new.set(false);
        self.editing_ticket.set(TicketMetadata::default());
        self.editing_body.set(String::new());
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
        self.editing_ticket.set(ticket);
        self.editing_body.set(body);
        self.is_editing_existing.set(true);
        self.is_creating_new.set(false);
    }

    /// Start creating a new ticket
    pub fn start_create(&mut self) {
        self.editing_ticket.set(TicketMetadata::default());
        self.editing_body.set(String::new());
        self.is_editing_existing.set(false);
        self.is_creating_new.set(true);
    }

    /// Get the ticket being edited (if editing existing)
    pub fn get_edit_ticket(&self) -> Option<TicketMetadata> {
        if self.is_editing_existing.get() {
            Some(self.editing_ticket.read().clone())
        } else {
            None
        }
    }

    /// Get the body for the edit form
    pub fn get_edit_body(&self) -> Option<String> {
        if self.is_editing() {
            Some(self.editing_body.to_string())
        } else {
            None
        }
    }
}

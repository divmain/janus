//! Triage mode modal trigger handlers
//!
//! This module handles triage mode specific key events that trigger modals
//! for ticket operations (notes and cancellation).

use iocraft::prelude::{KeyCode, KeyModifiers};

use crate::tui::components::{ModalState, NoteModalData, TicketModalData};
use crate::tui::search::FilteredTicket;

/// Handle triage mode modal triggers ('n' for note, 'c' for cancel)
///
/// Returns true if the event was handled (modal was opened), false otherwise.
///
/// This function is called before delegating to other handlers when in triage mode.
/// It checks for specific keys that should open confirmation modals rather than
/// executing actions immediately.
pub fn handle_triage_modal_triggers(
    code: KeyCode,
    modifiers: KeyModifiers,
    is_triage_mode: bool,
    selected_index: usize,
    filtered_tickets: &[FilteredTicket],
    note_modal: &mut ModalState<NoteModalData>,
    cancel_confirm_modal: &mut ModalState<TicketModalData>,
) -> bool {
    if !is_triage_mode {
        return false;
    }

    if modifiers != KeyModifiers::NONE {
        return false;
    }

    match code {
        KeyCode::Char('n') => {
            // Open note input modal
            if let Some(ft) = filtered_tickets.get(selected_index)
                && let Some(id) = &ft.ticket.id
            {
                note_modal.open(NoteModalData::new(id.clone()));
            }
            true
        }
        KeyCode::Char('c') => {
            // Open cancel confirmation modal
            if let Some(ft) = filtered_tickets.get(selected_index)
                && let Some(id) = &ft.ticket.id
            {
                cancel_confirm_modal.open(TicketModalData::new(
                    id.clone(),
                    ft.ticket.title.clone().unwrap_or_default(),
                ));
            }
            true
        }
        _ => false,
    }
}

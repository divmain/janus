//! Generic modal state management
//!
//! This module provides a generic wrapper for managing modal visibility and associated data.
//! It reduces boilerplate when modals need both an open/close state and associated data.
//!
//! ## When to use
//!
//! - **Use `ModalState<M, D>`**: When a modal carries data that's set when opening
//!   (e.g., note modal needs ticket_id and initial text)
//! - **Use `State<Option<T>>`**: When the data IS the visibility (e.g., `Option<SyncPreviewState>`)
//! - **Use `State<bool>`**: For simple stateless modals (e.g., help modal)

use iocraft::prelude::*;
use std::marker::PhantomData;

/// Generic state for managing a single modal with associated data
///
/// The type parameter `M` acts as a phantom marker to distinguish different modals
/// at the type level, even if they share the same data type.
///
/// # Example
///
/// ```ignore
/// // Define modal marker types
/// struct NoteModal;
/// struct ConfirmModal;
///
/// // Data types for each modal
/// #[derive(Default, Clone)]
/// struct NoteData { ticket_id: String, text: String }
///
/// // Initialize in component
/// let note_modal = ModalState::<NoteModal, NoteData>::use_state(&mut hooks);
/// let confirm_modal = ModalState::<ConfirmModal, String>::use_state(&mut hooks);
///
/// // Open with data
/// note_modal.open(NoteData { ticket_id: "j-1234".into(), text: String::new() });
///
/// // Check and use
/// if note_modal.is_open() {
///     let data = note_modal.data();
///     // render modal with data.ticket_id, data.text
/// }
///
/// // Close
/// note_modal.close();
/// ```
/// The `Copy` impl requires `D: Send + Sync + 'static + Unpin` because `State<D>` is only `Copy`
/// when `D` meets those bounds. This is fine since all our modal data types are
/// plain data structs.
pub struct ModalState<M, D: Send + Sync + 'static + Unpin = ()> {
    is_open: State<bool>,
    data: State<D>,
    _marker: PhantomData<M>,
}

// Manual Clone/Copy impls to properly bound them
impl<M, D: Send + Sync + 'static + Unpin> Clone for ModalState<M, D> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<M, D: Send + Sync + 'static + Unpin> Copy for ModalState<M, D> {}

impl<M, D> ModalState<M, D>
where
    D: Default + Clone + Send + Sync + 'static + Unpin,
{
    /// Initialize modal state using hooks
    ///
    /// This must be called inside a component function with access to `Hooks`.
    pub fn use_state(hooks: &mut Hooks) -> Self {
        ModalState {
            is_open: hooks.use_state(|| false),
            data: hooks.use_state(D::default),
            _marker: PhantomData,
        }
    }

    /// Open the modal with the given data
    ///
    /// Sets the data first, then marks the modal as open.
    /// Note: This copies the internal State handles and mutates them.
    pub fn open(&self, data: D) {
        let mut data_state = self.data;
        let mut open_state = self.is_open;
        data_state.set(data);
        open_state.set(true);
    }

    /// Close the modal
    ///
    /// Marks the modal as closed. Data is preserved until the next `open()` call.
    /// Note: This copies the internal State handle and mutates it.
    pub fn close(&self) {
        let mut open_state = self.is_open;
        open_state.set(false);
    }

    /// Check if the modal is currently open
    pub fn is_open(&self) -> bool {
        self.is_open.get()
    }

    /// Get a clone of the current data
    ///
    /// This can be called whether or not the modal is open.
    pub fn data(&self) -> D {
        self.data.read().clone()
    }

    /// Get a read guard for the data
    ///
    /// Useful when you need to borrow the data without cloning.
    pub fn read_data(&self) -> impl std::ops::Deref<Target = D> + '_ {
        self.data.read()
    }

    /// Update the data while the modal is open
    ///
    /// Useful for forms where the data changes during input.
    /// Note: This copies the internal State handle and mutates it.
    pub fn set_data(&self, data: D) {
        let mut data_state = self.data;
        data_state.set(data);
    }

    /// Get the underlying open state for direct manipulation
    ///
    /// This is useful when you need to pass the state to a child component
    /// or when working with event handlers that need mutable access.
    pub fn open_state(&self) -> State<bool> {
        self.is_open
    }

    /// Get the underlying data state for direct manipulation
    ///
    /// This is useful when you need to pass the state to a child component
    /// or when working with event handlers that need mutable access.
    pub fn data_state(&self) -> State<D> {
        self.data
    }
}

/// Data for modals that show information about a specific ticket
#[derive(Default, Clone)]
pub struct TicketModalData {
    /// The ticket ID (e.g., "j-1234")
    pub ticket_id: String,
    /// The ticket title for display
    pub ticket_title: String,
}

impl TicketModalData {
    /// Create new ticket modal data
    pub fn new(ticket_id: impl Into<String>, ticket_title: impl Into<String>) -> Self {
        Self {
            ticket_id: ticket_id.into(),
            ticket_title: ticket_title.into(),
        }
    }
}

/// Data for the note input modal
#[derive(Default, Clone)]
pub struct NoteModalData {
    /// The ticket ID to add the note to
    pub ticket_id: String,
    /// The current note text (editable)
    pub text: String,
}

impl NoteModalData {
    /// Create new note modal data
    pub fn new(ticket_id: impl Into<String>) -> Self {
        Self {
            ticket_id: ticket_id.into(),
            text: String::new(),
        }
    }

    /// Create with initial text
    pub fn with_text(ticket_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            ticket_id: ticket_id.into(),
            text: text.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ticket_modal_data() {
        let data = TicketModalData::new("j-1234", "Fix the bug");
        assert_eq!(data.ticket_id, "j-1234");
        assert_eq!(data.ticket_title, "Fix the bug");
    }

    #[test]
    fn test_note_modal_data() {
        let data = NoteModalData::new("j-1234");
        assert_eq!(data.ticket_id, "j-1234");
        assert!(data.text.is_empty());

        let data_with_text = NoteModalData::with_text("j-5678", "Initial note");
        assert_eq!(data_with_text.ticket_id, "j-5678");
        assert_eq!(data_with_text.text, "Initial note");
    }
}

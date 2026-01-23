//! Confirmation dialog for remote TUI operations

use iocraft::prelude::*;

use crate::tui::components::{
    ModalBorderColor, ModalContainer, ModalOverlay, ModalWidth, TextViewer,
};

/// The action to perform when confirmation is accepted
#[derive(Debug, Clone)]
pub enum ConfirmAction {
    /// Unlink the specified ticket IDs
    Unlink(Vec<String>),
}

/// State for the confirmation dialog
#[derive(Debug, Clone)]
pub struct ConfirmDialogState {
    /// Message to display to the user
    pub message: String,
    /// Whether "yes" is the default action
    #[allow(dead_code)]
    pub default_yes: bool,
    /// The action to perform if confirmed
    pub action: ConfirmAction,
}

impl ConfirmDialogState {
    /// Create a new confirmation dialog state
    pub fn new(message: String, default_yes: bool, action: ConfirmAction) -> Self {
        Self {
            message,
            default_yes,
            action,
        }
    }

    /// Create a confirmation for unlinking tickets
    pub fn for_unlink(ticket_ids: Vec<String>) -> Self {
        let message = if ticket_ids.len() == 1 {
            format!("Unlink ticket '{}' from its remote issue?", ticket_ids[0])
        } else {
            format!(
                "Unlink {} tickets from their remote issues?",
                ticket_ids.len()
            )
        };
        Self::new(message, false, ConfirmAction::Unlink(ticket_ids))
    }
}

/// Props for the ConfirmDialog component
#[derive(Default, Props)]
pub struct ConfirmDialogProps {
    /// The message to display
    pub message: String,
}

/// Confirmation dialog component using shared modal components
#[component]
pub fn ConfirmDialog(props: &ConfirmDialogProps) -> impl Into<AnyElement<'static>> {
    element! {
        ModalOverlay(show_backdrop: true) {
            ModalContainer(
                width: Some(ModalWidth::Fixed(60)),
                border_color: Some(ModalBorderColor::Warning),
                title: Some("Confirm".to_string()),
                footer_text: Some("[Y]es / [n]o / [c]ancel".to_string()),
            ) {
                TextViewer(
                    text: props.message.clone(),
                    scroll_offset: 0usize,
                    has_focus: false,
                    placeholder: Some("No message".to_string()),
                )
            }
        }
    }
}

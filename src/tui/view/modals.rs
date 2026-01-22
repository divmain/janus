//! Modal dialogs for the issue browser view
//!
//! This module contains modal components used in triage mode:
//! - NoteInputModal: For adding notes to tickets
//! - CancelConfirmModal: For confirming ticket cancellation

use iocraft::prelude::*;

use crate::tui::theme::theme;

// =============================================================================
// Note Input Modal
// =============================================================================

/// Props for the NoteInputModal component
#[derive(Default, Props)]
pub struct NoteInputModalProps {
    /// The ticket ID being annotated
    pub ticket_id: String,
    /// Current note text state
    pub note_text: Option<State<String>>,
}

/// Modal dialog for inputting a note to add to a ticket
///
/// Displays a text input area where the user can type a note.
/// - Submit with Enter (when note is not empty)
/// - Cancel with Escape
#[component]
pub fn NoteInputModal<'a>(
    props: &NoteInputModalProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'a>> {
    let theme = theme();

    // Local state for TextInput
    let initial_value = props.note_text.map(|s| s.to_string()).unwrap_or_default();
    let mut local_value = hooks.use_state(move || initial_value);

    // Handle for imperative cursor control
    let mut handle = hooks.use_ref_default::<TextInputHandle>();

    // Set cursor to beginning on initial render
    hooks.use_effect(move || handle.write().set_cursor_offset(0), ());

    // Get current value for TextInput
    let text_input_value = local_value.to_string();

    // Update external state when local changes
    let external_value = props.note_text;

    element! {
        View(
            width: 100pct,
            height: 100pct,
            position: Position::Absolute,
            top: 0,
            left: 0,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
        ) {
            View(
                width: 60,
                height: 18,
                border_style: BorderStyle::Double,
                border_color: theme.border_focused,
                padding: 1,
                flex_direction: FlexDirection::Column,
                background_color: theme.background,
            ) {
                // Header
                View(
                    width: 100pct,
                    flex_direction: FlexDirection::Row,
                    padding_bottom: 1,
                    border_edges: Edges::Bottom,
                    border_style: BorderStyle::Single,
                    border_color: theme.border,
                ) {
                    Text(
                        content: format!("Add Note to {}", props.ticket_id),
                        color: theme.border_focused,
                        weight: Weight::Bold,
                    )
                }

                // Note input area
                View(
                    width: 100pct,
                    height: 10,
                    margin_top: 1,
                    border_style: BorderStyle::Round,
                    border_color: theme.border_focused,
                    padding_left: 1,
                    padding_right: 1,
                    overflow: Overflow::Hidden,
                ) {
                    TextInput(
                        has_focus: true,
                        value: text_input_value,
                        on_change: move |new_value: String| {
                            local_value.set(new_value.clone());
                            if let Some(mut ext) = external_value {
                                ext.set(new_value);
                            }
                        },
                        multiline: true,
                        cursor_color: Some(theme.highlight),
                        color: Some(theme.text),
                        handle,
                    )
                }

                // Footer with instructions
                View(
                    width: 100pct,
                    margin_top: 1,
                ) {
                    Text(
                        content: "[Enter] Submit  [Esc] Cancel",
                        color: theme.text_dimmed,
                    )
                }
            }
        }
    }
}

// =============================================================================
// Cancel Confirmation Modal
// =============================================================================

/// Props for the CancelConfirmModal component
#[derive(Default, Props)]
pub struct CancelConfirmModalProps {
    /// The ticket ID being cancelled
    pub ticket_id: String,
    /// The ticket title (for display)
    pub ticket_title: String,
}

/// Modal dialog for confirming ticket cancellation
///
/// Displays a confirmation prompt asking the user to press `c` again to confirm.
/// - Press `c` again to confirm cancellation
/// - Escape or any other key cancels the action
#[component]
pub fn CancelConfirmModal<'a>(
    props: &CancelConfirmModalProps,
    _hooks: Hooks,
) -> impl Into<AnyElement<'a>> {
    let theme = theme();

    // Truncate title if too long
    let display_title = if props.ticket_title.len() > 40 {
        format!("{}...", &props.ticket_title[..37])
    } else {
        props.ticket_title.clone()
    };

    element! {
        View(
            width: 100pct,
            height: 100pct,
            position: Position::Absolute,
            top: 0,
            left: 0,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
        ) {
            View(
                width: 50,
                border_style: BorderStyle::Double,
                border_color: Color::Yellow,
                padding: 1,
                flex_direction: FlexDirection::Column,
                background_color: theme.background,
            ) {
                // Header
                View(
                    width: 100pct,
                    padding_bottom: 1,
                    border_edges: Edges::Bottom,
                    border_style: BorderStyle::Single,
                    border_color: theme.border,
                ) {
                    Text(
                        content: "Confirm Cancellation",
                        color: Color::Yellow,
                        weight: Weight::Bold,
                    )
                }

                // Message
                View(
                    width: 100pct,
                    margin_top: 1,
                    flex_direction: FlexDirection::Column,
                ) {
                    Text(
                        content: format!("Cancel ticket {}?", props.ticket_id),
                        color: theme.text,
                    )
                    Text(
                        content: display_title,
                        color: theme.text_dimmed,
                    )
                }

                // Instructions
                View(
                    width: 100pct,
                    margin_top: 2,
                ) {
                    Text(
                        content: "Press [c] again to confirm, [Esc] to cancel",
                        color: theme.text_dimmed,
                    )
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_title_truncation() {
        let long_title = "This is a very long ticket title that should be truncated for display";
        let truncated = if long_title.len() > 40 {
            format!("{}...", &long_title[..37])
        } else {
            long_title.to_string()
        };
        assert!(truncated.len() <= 43); // 40 chars + "..."
        assert!(truncated.ends_with("..."));
    }
}

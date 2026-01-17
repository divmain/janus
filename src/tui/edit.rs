//! Edit form modal for creating and editing tickets
//!
//! Provides a full-featured form for editing all ticket fields including
//! title, status, type, priority, and body content.

use iocraft::prelude::*;

use crate::formatting::extract_ticket_body;
use crate::tui::components::{Footer, Selectable, edit_shortcuts, options_for};
use crate::tui::services::{TicketEditService, TicketFormValidator};
use crate::tui::theme::theme;
use crate::types::{TicketMetadata, TicketPriority, TicketStatus, TicketType};

/// Which field is currently focused in the edit form
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditField {
    #[default]
    Title,
    Status,
    Type,
    Priority,
    Body,
}

impl EditField {
    /// Get the next field (wrapping)
    pub fn next(self) -> Self {
        match self {
            EditField::Title => EditField::Status,
            EditField::Status => EditField::Type,
            EditField::Type => EditField::Priority,
            EditField::Priority => EditField::Body,
            EditField::Body => EditField::Title,
        }
    }

    /// Get the previous field (wrapping)
    pub fn prev(self) -> Self {
        match self {
            EditField::Title => EditField::Body,
            EditField::Status => EditField::Title,
            EditField::Type => EditField::Status,
            EditField::Priority => EditField::Type,
            EditField::Body => EditField::Priority,
        }
    }
}

/// Result of the edit form
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditResult {
    /// User saved changes
    Saved,
    /// User cancelled without saving
    Cancelled,
    /// Still editing
    #[default]
    Editing,
}

/// Props for the EditForm component
#[derive(Default, Props)]
pub struct EditFormProps {
    /// Initial ticket metadata (for editing existing - None = creating new ticket)
    /// If ticket.id is Some, we're editing; if None, we're creating
    pub ticket: Option<TicketMetadata>,
    /// Initial body content
    pub initial_body: Option<String>,
    /// Callback when form is closed
    pub on_close: Option<State<EditResult>>,
}

/// Full edit form modal component
#[component]
pub fn EditForm<'a>(props: &EditFormProps, mut hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let theme = theme();
    let (width, height) = hooks.use_terminal_size();

    // Get initial values from props
    let initial_ticket = props.ticket.clone().unwrap_or_default();
    let ticket_id = initial_ticket.id.clone();
    let is_new = ticket_id.is_none();

    // State for form fields
    let mut title = hooks.use_state(|| initial_ticket.title.clone().unwrap_or_default());
    let mut status = hooks.use_state(|| initial_ticket.status.unwrap_or(TicketStatus::New));
    let mut ticket_type =
        hooks.use_state(|| initial_ticket.ticket_type.unwrap_or(TicketType::Task));
    let mut priority = hooks.use_state(|| initial_ticket.priority.unwrap_or(TicketPriority::P2));
    let mut body = hooks.use_state(|| props.initial_body.clone().unwrap_or_default());

    // UI state
    let mut focused_field = hooks.use_state(EditField::default);
    let mut should_save = hooks.use_state(|| false);
    let mut should_cancel = hooks.use_state(|| false);
    let mut has_error = hooks.use_state(|| false);
    let mut error_text = hooks.use_state(String::new);

    // Handle save logic
    if should_save.get() {
        should_save.set(false);

        // Validate form using validator
        let title_val = title.to_string();
        let validation_result = TicketFormValidator::validate(
            &title_val,
            status.get(),
            ticket_type.get(),
            priority.get(),
            &body.to_string(),
        );

        if !validation_result.is_valid {
            has_error.set(true);
            error_text.set(validation_result.error.unwrap_or_default());
        } else {
            // Save the ticket via edit service
            let save_result = TicketEditService::save(
                ticket_id.as_deref(),
                &title_val,
                status.get(),
                ticket_type.get(),
                priority.get(),
                &body.to_string(),
            );

            match save_result {
                Ok(()) => {
                    if let Some(mut on_close) = props.on_close {
                        on_close.set(EditResult::Saved);
                    }
                }
                Err(e) => {
                    has_error.set(true);
                    error_text.set(format!("Save failed: {}", e));
                }
            }
        }
    }

    // Handle cancel
    if should_cancel.get() {
        should_cancel.set(false);
        if let Some(mut on_close) = props.on_close {
            on_close.set(EditResult::Cancelled);
        }
    }

    // Keyboard handling
    hooks.use_terminal_events({
        move |event| {
            if let TerminalEvent::Key(KeyEvent {
                code,
                kind,
                modifiers,
                ..
            }) = event
            {
                if kind == KeyEventKind::Release {
                    return;
                }

                // Global shortcuts (work in any field)
                if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('s') {
                    should_save.set(true);
                    return;
                }

                match code {
                    KeyCode::Esc => {
                        should_cancel.set(true);
                        return;
                    }
                    KeyCode::Tab if modifiers.contains(KeyModifiers::SHIFT) => {
                        focused_field.set(focused_field.get().prev());
                        return;
                    }
                    KeyCode::Tab => {
                        focused_field.set(focused_field.get().next());
                        return;
                    }
                    KeyCode::BackTab => {
                        focused_field.set(focused_field.get().prev());
                        return;
                    }
                    _ => {}
                }

                // Field-specific handling
                match focused_field.get() {
                    EditField::Title => handle_text_input(&mut title, code),
                    EditField::Body => handle_multiline_input(&mut body, code),
                    EditField::Status => handle_select_input(&mut status, code),
                    EditField::Type => handle_select_input(&mut ticket_type, code),
                    EditField::Priority => handle_select_input(&mut priority, code),
                }
            }
        }
    });

    // Calculate modal size
    let modal_width = width.saturating_sub(8).min(80);
    let modal_height = height.saturating_sub(4).min(30);
    let left_padding = (width.saturating_sub(modal_width)) / 2;
    let top_padding = (height.saturating_sub(modal_height)) / 2;

    // Header title
    let header_title = if is_new {
        "New Ticket".to_string()
    } else {
        format!("Edit: {}", ticket_id.as_deref().unwrap_or(""))
    };

    // Get options for selects
    let status_options = options_for::<TicketStatus>();
    let type_options = options_for::<TicketType>();
    let priority_options = options_for::<TicketPriority>();

    element! {
        // Modal backdrop
        View(
            width,
            height,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Start,
            align_items: AlignItems::Start,
        ) {
            // Top padding
            View(height: top_padding)

            View(flex_direction: FlexDirection::Row) {
                // Left padding
                View(width: left_padding)

                // Modal content
                View(
                    width: modal_width,
                    height: modal_height,
                    flex_direction: FlexDirection::Column,
                    border_style: BorderStyle::Round,
                    border_color: theme.border_focused,
                    background_color: theme.background,
                ) {
                    // Header
                    View(
                        width: 100pct,
                        height: 1,
                        padding_left: 1,
                        border_edges: Edges::Bottom,
                        border_style: BorderStyle::Single,
                        border_color: theme.border,
                        background_color: theme.border,
                    ) {
                        Text(
                            content: header_title,
                            color: theme.text,
                            weight: Weight::Bold,
                        )
                    }

                    // Error message (if any)
                    #(if has_error.get() {
                        Some(element! {
                            View(
                                width: 100pct,
                                padding_left: 1,
                                padding_right: 1,
                                margin_top: 1,
                            ) {
                                Text(
                                    content: error_text.to_string(),
                                    color: Color::Red,
                                )
                            }
                        })
                    } else {
                        None
                    })

                    // Form content
                    View(
                        flex_grow: 1.0,
                        width: 100pct,
                        padding: 1,
                        flex_direction: FlexDirection::Column,
                        gap: 1,
                    ) {
                        // Title field
                        View(flex_direction: FlexDirection::Column) {
                            Text(
                                content: "Title:",
                                color: if focused_field.get() == EditField::Title {
                                    theme.border_focused
                                } else {
                                    theme.text_dimmed
                                },
                            )
                            View(
                                border_style: BorderStyle::Round,
                                border_color: if focused_field.get() == EditField::Title {
                                    theme.border_focused
                                } else {
                                    theme.border
                                },
                                padding_left: 1,
                                padding_right: 1,
                                width: 100pct,
                            ) {
                                Text(
                                    content: format!("{}_", title.to_string()),
                                    color: theme.text,
                                )
                            }
                        }

                        // Row: Status and Type
                        View(flex_direction: FlexDirection::Row, gap: 2) {
                            // Status selector
                            View(flex_direction: FlexDirection::Row, gap: 1) {
                                Text(
                                    content: "Status:",
                                    color: if focused_field.get() == EditField::Status {
                                        theme.border_focused
                                    } else {
                                        theme.text_dimmed
                                    },
                                )
                                View(
                                    border_style: BorderStyle::Round,
                                    border_color: if focused_field.get() == EditField::Status {
                                        theme.border_focused
                                    } else {
                                        theme.border
                                    },
                                    padding_left: 1,
                                    padding_right: 1,
                                    min_width: 14,
                                ) {
                                    View(flex_direction: FlexDirection::Row, gap: 1) {
                                        Text(
                                            content: status_options.get(status.get().index()).cloned().unwrap_or_default(),
                                            color: theme.status_color(status.get()),
                                        )
                                        Text(content: "v", color: theme.text_dimmed)
                                    }
                                }
                            }

                            // Type selector
                            View(flex_direction: FlexDirection::Row, gap: 1) {
                                Text(
                                    content: "Type:",
                                    color: if focused_field.get() == EditField::Type {
                                        theme.border_focused
                                    } else {
                                        theme.text_dimmed
                                    },
                                )
                                View(
                                    border_style: BorderStyle::Round,
                                    border_color: if focused_field.get() == EditField::Type {
                                        theme.border_focused
                                    } else {
                                        theme.border
                                    },
                                    padding_left: 1,
                                    padding_right: 1,
                                    min_width: 12,
                                ) {
                                    View(flex_direction: FlexDirection::Row, gap: 1) {
                                        Text(
                                            content: type_options.get(ticket_type.get().index()).cloned().unwrap_or_default(),
                                            color: theme.type_color(ticket_type.get()),
                                        )
                                        Text(content: "v", color: theme.text_dimmed)
                                    }
                                }
                            }
                        }

                        // Row: Priority
                        View(flex_direction: FlexDirection::Row, gap: 2) {
                            // Priority selector
                            View(flex_direction: FlexDirection::Row, gap: 1) {
                                Text(
                                    content: "Priority:",
                                    color: if focused_field.get() == EditField::Priority {
                                        theme.border_focused
                                    } else {
                                        theme.text_dimmed
                                    },
                                )
                                View(
                                    border_style: BorderStyle::Round,
                                    border_color: if focused_field.get() == EditField::Priority {
                                        theme.border_focused
                                    } else {
                                        theme.border
                                    },
                                    padding_left: 1,
                                    padding_right: 1,
                                    min_width: 6,
                                ) {
                                    View(flex_direction: FlexDirection::Row, gap: 1) {
                                        Text(
                                            content: priority_options.get(priority.get().index()).cloned().unwrap_or_default(),
                                            color: theme.priority_color(priority.get()),
                                        )
                                        Text(content: "v", color: theme.text_dimmed)
                                    }
                                }
                            }
                        }

                        // Separator
                        View(
                            width: 100pct,
                            margin_top: 1,
                            border_edges: Edges::Bottom,
                            border_style: BorderStyle::Single,
                            border_color: theme.border,
                        )

                        // Body field label
                        Text(
                            content: "Description:",
                            color: if focused_field.get() == EditField::Body {
                                theme.border_focused
                            } else {
                                theme.text_dimmed
                            },
                        )

                        // Body text area
                        View(
                            flex_grow: 1.0,
                            width: 100pct,
                            border_style: BorderStyle::Round,
                            border_color: if focused_field.get() == EditField::Body {
                                theme.border_focused
                            } else {
                                theme.border
                            },
                            padding: 1,
                            overflow: Overflow::Hidden,
                        ) {
                            View(flex_direction: FlexDirection::Column) {
                                #(body.to_string().lines().take(10).map(|line| {
                                    let line_owned = line.to_string();
                                    element! {
                                        Text(content: line_owned, color: theme.text)
                                    }
                                }))
                                #(if focused_field.get() == EditField::Body {
                                    Some(element! {
                                        Text(content: "_", color: theme.highlight)
                                    })
                                } else {
                                    None
                                })
                            }
                        }
                    }

                    // Footer
                    Footer(shortcuts: edit_shortcuts())
                }
            }
        }
    }
}

/// Handle text input for single-line fields
fn handle_text_input(state: &mut State<String>, code: KeyCode) {
    match code {
        KeyCode::Char(c) => {
            let mut val = state.to_string();
            val.push(c);
            state.set(val);
        }
        KeyCode::Backspace => {
            let mut val = state.to_string();
            val.pop();
            state.set(val);
        }
        _ => {}
    }
}

/// Handle text input for multiline fields
fn handle_multiline_input(state: &mut State<String>, code: KeyCode) {
    match code {
        KeyCode::Char(c) => {
            let mut val = state.to_string();
            val.push(c);
            state.set(val);
        }
        KeyCode::Backspace => {
            let mut val = state.to_string();
            val.pop();
            state.set(val);
        }
        KeyCode::Enter => {
            let mut val = state.to_string();
            val.push('\n');
            state.set(val);
        }
        _ => {}
    }
}

/// Handle select input for enum fields
fn handle_select_input<T: Selectable + Send + Sync + 'static>(state: &mut State<T>, code: KeyCode) {
    match code {
        KeyCode::Left | KeyCode::Char('h') => {
            state.set(state.get().prev());
        }
        KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter | KeyCode::Char(' ') => {
            state.set(state.get().next());
        }
        _ => {}
    }
}

/// Extract body content from ticket file (everything after title)
pub fn extract_body_for_edit(content: &str) -> String {
    extract_ticket_body(content).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edit_field_navigation() {
        assert_eq!(EditField::Title.next(), EditField::Status);
        assert_eq!(EditField::Body.next(), EditField::Title);
        assert_eq!(EditField::Title.prev(), EditField::Body);
        assert_eq!(EditField::Status.prev(), EditField::Title);
    }

    #[test]
    fn test_extract_body_for_edit() {
        let content = r#"---
id: test
status: new
---
# Test Title

This is the body.
With multiple lines.
"#;
        let body = extract_body_for_edit(content);
        assert!(body.contains("This is the body"));
        assert!(body.contains("With multiple lines"));
        assert!(!body.contains("Test Title"));
    }
}

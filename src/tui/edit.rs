//! Edit form modal for creating and editing tickets
//!
//! Provides a full-featured form for editing all ticket fields including
//! title, status, type, priority, and body content.

use iocraft::prelude::*;

use crate::display::extract_ticket_body;
use crate::tui::components::{Clickable, Select, Selectable, TextEditor, options_for};
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

/// Request data for async save operation
struct SaveRequest {
    ticket_id: Option<String>,
    title: String,
    status: TicketStatus,
    ticket_type: TicketType,
    priority: TicketPriority,
    body: String,
}

/// Full edit form modal component
#[component]
pub fn EditForm<'a>(props: &EditFormProps, mut hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let theme = theme();
    let (_width, _height) = hooks.use_terminal_size();

    // Get initial values from props
    let initial_ticket = props.ticket.clone().unwrap_or_default();
    let ticket_id = initial_ticket.id.as_ref().map(|id| id.to_string());
    let is_new = ticket_id.is_none();

    // State for form fields
    let mut title = hooks.use_state(|| initial_ticket.title.clone().unwrap_or_default());
    let mut status = hooks.use_state(|| initial_ticket.status.unwrap_or(TicketStatus::New));
    let mut ticket_type =
        hooks.use_state(|| initial_ticket.ticket_type.unwrap_or(TicketType::Task));
    let mut priority = hooks.use_state(|| initial_ticket.priority.unwrap_or(TicketPriority::P2));
    let body = hooks.use_state(|| props.initial_body.clone().unwrap_or_default());

    // UI state
    let mut focused_field = hooks.use_state(EditField::default);
    let mut should_save = hooks.use_state(|| false);
    let mut should_cancel = hooks.use_state(|| false);
    let mut has_error = hooks.use_state(|| false);
    let mut error_text = hooks.use_state(String::new);
    let mut is_saving = hooks.use_state(|| false);

    // Click handlers for form fields (using async_handler pattern to allow state mutation)
    let focus_title_handler: Handler<()> = hooks.use_async_handler({
        let focused_field_setter = focused_field;
        move |_| {
            let mut focused_field_setter = focused_field_setter;
            async move {
                focused_field_setter.set(EditField::Title);
            }
        }
    });

    let focus_body_handler: Handler<()> = hooks.use_async_handler({
        let focused_field_setter = focused_field;
        move |_| {
            let mut focused_field_setter = focused_field_setter;
            async move {
                focused_field_setter.set(EditField::Body);
            }
        }
    });

    // Handlers for Select component arrows
    let status_prev_handler: Handler<()> = hooks.use_async_handler({
        let status_setter = status;
        move |_| {
            let mut status_setter = status_setter;
            async move {
                status_setter.set(status_setter.get().prev());
            }
        }
    });

    let status_next_handler: Handler<()> = hooks.use_async_handler({
        let status_setter = status;
        move |_| {
            let mut status_setter = status_setter;
            async move {
                status_setter.set(status_setter.get().next());
            }
        }
    });

    let type_prev_handler: Handler<()> = hooks.use_async_handler({
        let type_setter = ticket_type;
        move |_| {
            let mut type_setter = type_setter;
            async move {
                type_setter.set(type_setter.get().prev());
            }
        }
    });

    let type_next_handler: Handler<()> = hooks.use_async_handler({
        let type_setter = ticket_type;
        move |_| {
            let mut type_setter = type_setter;
            async move {
                type_setter.set(type_setter.get().next());
            }
        }
    });

    let priority_prev_handler: Handler<()> = hooks.use_async_handler({
        let priority_setter = priority;
        move |_| {
            let mut priority_setter = priority_setter;
            async move {
                priority_setter.set(priority_setter.get().prev());
            }
        }
    });

    let priority_next_handler: Handler<()> = hooks.use_async_handler({
        let priority_setter = priority;
        move |_| {
            let mut priority_setter = priority_setter;
            async move {
                priority_setter.set(priority_setter.get().next());
            }
        }
    });

    // Async save handler
    let save_handler: Handler<SaveRequest> = hooks.use_async_handler({
        let has_error_setter = has_error;
        let error_text_setter = error_text;
        let is_saving_setter = is_saving;
        let on_close = props.on_close;

        move |request: SaveRequest| {
            let mut has_error_setter = has_error_setter;
            let mut error_text_setter = error_text_setter;
            let mut is_saving_setter = is_saving_setter;
            let on_close = on_close;

            async move {
                let result = TicketEditService::save(
                    request.ticket_id.as_deref(),
                    &request.title,
                    request.status,
                    request.ticket_type,
                    request.priority,
                    &request.body,
                )
                .await;

                is_saving_setter.set(false);

                match result {
                    Ok(()) => {
                        if let Some(mut on_close) = on_close {
                            on_close.set(EditResult::Saved);
                        }
                    }
                    Err(e) => {
                        has_error_setter.set(true);
                        error_text_setter.set(format!("Save failed: {e}"));
                    }
                }
            }
        }
    });

    // Handle save logic
    if should_save.get() && !is_saving.get() {
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
            // Save the ticket via async handler
            is_saving.set(true);
            save_handler(SaveRequest {
                ticket_id: ticket_id.clone(),
                title: title_val,
                status: status.get(),
                ticket_type: ticket_type.get(),
                priority: priority.get(),
                body: body.to_string(),
            });
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

                // When Title or Body field is focused, let TextInput handle all keys except
                // navigation (Tab/Esc) and global shortcuts (Ctrl+S).
                // This prevents double-handling of key events that causes cursor issues.
                let is_text_input_focused =
                    matches!(focused_field.get(), EditField::Title | EditField::Body);
                let is_navigation_key =
                    matches!(code, KeyCode::Esc | KeyCode::Tab | KeyCode::BackTab);
                let is_global_shortcut =
                    modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('s');

                if is_text_input_focused && !is_navigation_key && !is_global_shortcut {
                    // Let TextInput handle this key exclusively
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
                    EditField::Title | EditField::Body => {} // TextInput handles these
                    EditField::Status => handle_select_input(&mut status, code),
                    EditField::Type => handle_select_input(&mut ticket_type, code),
                    EditField::Priority => handle_select_input(&mut priority, code),
                }
            }
        }
    });

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
        // Modal content
        View(
            width: 90pct,
            height: 90pct,
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
                                    color: theme.error,
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
                        overflow: Overflow::Hidden,
                    ) {
                        // Title field
                        Clickable(
                            on_click: Some(focus_title_handler.clone()),
                        ) {
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
                                    TextInput(
                                        value: title.to_string(),
                                        has_focus: focused_field.get() == EditField::Title,
                                        on_change: move |new_value: String| title.set(new_value),
                                        color: theme.text,
                                        cursor_color: Some(theme.highlight),
                                    )
                                }
                            }
                        }

                        // Row: Status, Type, and Priority (compact inline selectors)
                        View(flex_direction: FlexDirection::Row, gap: 3) {
                            Select(
                                label: Some("Status"),
                                options: status_options.clone(),
                                selected_index: status.get().index(),
                                has_focus: focused_field.get() == EditField::Status,
                                value_color: Some(theme.status_color(status.get())),
                                on_prev: Some(status_prev_handler.clone()),
                                on_next: Some(status_next_handler.clone()),
                            )
                            Select(
                                label: Some("Type"),
                                options: type_options.clone(),
                                selected_index: ticket_type.get().index(),
                                has_focus: focused_field.get() == EditField::Type,
                                value_color: Some(theme.type_color(ticket_type.get())),
                                on_prev: Some(type_prev_handler.clone()),
                                on_next: Some(type_next_handler.clone()),
                            )
                            Select(
                                label: Some("Priority"),
                                options: priority_options.clone(),
                                selected_index: priority.get().index(),
                                has_focus: focused_field.get() == EditField::Priority,
                                value_color: Some(theme.priority_color(priority.get())),
                                on_prev: Some(priority_prev_handler.clone()),
                                on_next: Some(priority_next_handler.clone()),
                            )
                        }

                        // Separator
                        View(
                            width: 100pct,
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
                        Clickable(
                            on_click: Some(focus_body_handler.clone()),
                        ) {
                            View(
                                width: 100pct,
                                flex_grow: 1.0,
                                overflow: Overflow::Hidden,
                            ) {
                                TextEditor(
                                    value: Some(body),
                                    has_focus: focused_field.get() == EditField::Body,
                                    cursor_color: None,
                                )
                            }
                        }
                    }

                    // Footer with Save and Cancel buttons
                    View(
                        width: 100pct,
                        height: 3,
                        padding: 1,
                        border_edges: Edges::Top,
                        border_style: BorderStyle::Single,
                        border_color: theme.border,
                        flex_direction: FlexDirection::Row,
                        gap: 2,
                        justify_content: JustifyContent::Center,
                    ) {
                        Button(
                            handler: move |_| should_save.set(true),
                            has_focus: false,
                        ) {
                            View(
                                border_style: BorderStyle::Round,
                                border_color: theme.status_complete,
                                padding_left: 2,
                                padding_right: 2,
                                background_color: theme.status_complete,
                            ) {
                                Text(
                                    content: "Save (Ctrl+S)",
                                    color: Color::Black,
                                    weight: Weight::Bold,
                                )
                            }
                        }
                        Button(
                            handler: move |_| should_cancel.set(true),
                            has_focus: false,
                        ) {
                            View(
                                border_style: BorderStyle::Round,
                                border_color: theme.border,
                                padding_left: 2,
                                padding_right: 2,
                            ) {
                                Text(
                                    content: "Cancel (Esc)",
                                    color: theme.text,
                                )
                            }
                        }
                    }
                }
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

/// Edit form overlay with backdrop
///
/// Renders the EditForm component as an overlay on top of existing content.
/// The backdrop is transparent (no background_color) so underlying content
/// remains visible around the modal.
#[component]
pub fn EditFormOverlay<'a>(props: &EditFormProps, _hooks: Hooks) -> impl Into<AnyElement<'a>> {
    element! {
        // Modal container - no background color so underlying content shows through
        View(
            width: 100pct,
            height: 100pct,
            position: Position::Absolute,
            top: 0,
            left: 0,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
        ) {
            EditForm(
                ticket: props.ticket.clone(),
                initial_body: props.initial_body.clone(),
                on_close: props.on_close.clone(),
            )
        }
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

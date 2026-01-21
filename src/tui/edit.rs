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
    let (_width, height) = hooks.use_terminal_size();

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
    let mut body_scroll_offset = hooks.use_state(|| 0usize);
    let mut should_save = hooks.use_state(|| false);
    let mut should_cancel = hooks.use_state(|| false);
    let mut has_error = hooks.use_state(|| false);
    let mut error_text = hooks.use_state(String::new);
    let mut is_saving = hooks.use_state(|| false);

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
                        error_text_setter.set(format!("Save failed: {}", e));
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
                    EditField::Body => {
                        handle_body_input(&mut body, &mut body_scroll_offset, code, height)
                    }
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
        // Modal backdrop
        View(
            width: 100pct,
            height: 100pct,
            position: Position::Absolute,
            top: 0,
            left: 0,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            background_color: Color::Rgb { r: 80, g: 80, b: 80 },
        ) {
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
                        overflow: Overflow::Hidden,
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
                            View(flex_direction: FlexDirection::Column, height: 100pct) {
                                #({
                                    let body_text = body.to_string();
                                    let lines: Vec<&str> = body_text.lines().collect();
                                    let total_lines = lines.len();
                                    let scroll_offset_val = body_scroll_offset.get().min(total_lines.saturating_sub(1));
                                    let is_body_focused = focused_field.get() == EditField::Body;

                                    if body_text.is_empty() {
                                        vec![
                                            element! {
                                                Text(content: "_", color: theme.text)
                                            }.into()
                                        ]
                                    } else {
                                        let mut elements: Vec<AnyElement<'static>> = Vec::new();

                                        let body_scroll = scroll_offset_val;
                                        let has_more_above = body_scroll > 0;

                                        if has_more_above {
                                            elements.push(element! {
                                                Text(content: "↑", color: theme.text_dimmed)
                                            }.into());
                                        }

                                        // Render all lines from scroll offset - flexbox overflow handles clipping
                                        for line in lines.iter().skip(body_scroll) {
                                            let line_owned = line.to_string();
                                            elements.push(element! {
                                                Text(content: line_owned, color: theme.text)
                                            }.into());
                                        }

                                        if is_body_focused {
                                            elements.push(element! {
                                                Text(content: "_", color: theme.highlight)
                                            }.into());
                                        }

                                        elements
                                    }
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

/// Handle text input for body field with scrolling support
fn handle_body_input(
    state: &mut State<String>,
    scroll_offset: &mut State<usize>,
    code: KeyCode,
    terminal_height: u16,
) {
    let body_text = state.to_string();
    let lines: Vec<&str> = body_text.lines().collect();
    let total_lines = lines.len();

    // Estimate visible lines in body area: ~30% of terminal height after modal chrome
    // This is approximate since flexbox handles actual sizing
    let effective_visible = ((terminal_height as usize) * 90 / 100 / 3).max(3);

    match code {
        KeyCode::Char(c) if c != 'j' && c != 'k' => {
            let mut val = body_text;
            val.push(c);
            state.set(val);
        }
        KeyCode::Backspace => {
            let mut val = body_text;
            val.pop();
            state.set(val);
        }
        KeyCode::Enter => {
            let mut val = body_text;
            val.push('\n');
            state.set(val);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let current = scroll_offset.get();
            let max_scroll = total_lines.saturating_sub(effective_visible);
            if current < max_scroll {
                scroll_offset.set(current.saturating_add(1));
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            let current = scroll_offset.get();
            if current > 0 {
                scroll_offset.set(current.saturating_sub(1));
            }
        }
        KeyCode::PageDown => {
            let current = scroll_offset.get();
            let page_size = effective_visible.saturating_sub(1).max(1);
            let max_scroll = total_lines.saturating_sub(effective_visible);
            let new_offset = current.saturating_add(page_size).min(max_scroll);
            scroll_offset.set(new_offset);
        }
        KeyCode::PageUp => {
            let current = scroll_offset.get();
            let page_size = effective_visible.saturating_sub(1).max(1);
            scroll_offset.set(current.saturating_sub(page_size));
        }
        KeyCode::Home => {
            scroll_offset.set(0);
        }
        KeyCode::End => {
            let max_scroll = total_lines.saturating_sub(effective_visible);
            scroll_offset.set(max_scroll);
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

//! Remote TUI detail pane component
//!
//! Displays detailed information about the selected issue or ticket.
//! Supports mouse wheel scrolling.

use iocraft::prelude::*;

use crate::display::extract_ticket_body;
use crate::remote::{RemoteIssue, RemoteStatus};
use crate::ticket::Ticket;
use crate::tui::components::{Clickable, TextViewer};
use crate::tui::remote::state::ViewMode;
use crate::tui::theme::theme;
use crate::types::TicketMetadata;

/// Props for the DetailPane component
#[derive(Default, Props)]
pub struct DetailPaneProps {
    /// Current view mode (Local or Remote)
    pub view_mode: ViewMode,
    /// Selected remote issue (if any)
    pub selected_remote: Option<RemoteIssue>,
    /// Selected local ticket (if any)
    pub selected_local: Option<TicketMetadata>,
    /// Whether the detail pane should be visible
    pub visible: bool,
    /// Scroll offset for remote detail body
    pub remote_scroll_offset: usize,
    /// Scroll offset for local detail body
    pub local_scroll_offset: usize,
    /// All local tickets (for checking link status of remote issues)
    pub all_local_tickets: Vec<TicketMetadata>,
    /// Handler invoked when scroll up is requested (mouse wheel)
    pub on_scroll_up: Option<Handler<()>>,
    /// Handler invoked when scroll down is requested (mouse wheel)
    pub on_scroll_down: Option<Handler<()>>,
}

/// Detail pane showing issue/ticket details
#[component]
pub fn DetailPane(props: &DetailPaneProps) -> impl Into<AnyElement<'static>> {
    let theme = theme();

    if !props.visible {
        return element! {
            View()
        };
    }

    element! {
        View(
            flex_grow: 1.0,
            height: 100pct,
            flex_direction: FlexDirection::Column,
            border_style: BorderStyle::Round,
            border_color: theme.border,
        ) {
            #(if props.view_mode == ViewMode::Remote {
                render_remote_detail(
                    &props.selected_remote,
                    props.remote_scroll_offset,
                    &props.all_local_tickets,
                    props.on_scroll_up.clone(),
                    props.on_scroll_down.clone(),
                )
            } else {
                render_local_detail(
                    &props.selected_local,
                    props.local_scroll_offset,
                    props.on_scroll_up.clone(),
                    props.on_scroll_down.clone(),
                )
            })
        }
    }
}

/// Render remote issue detail
fn render_remote_detail(
    selected_remote: &Option<RemoteIssue>,
    remote_scroll_offset: usize,
    all_local_tickets: &[TicketMetadata],
    on_scroll_up: Option<Handler<()>>,
    on_scroll_down: Option<Handler<()>>,
) -> Option<AnyElement<'static>> {
    let theme = theme();

    if let Some(issue) = selected_remote {
        let status_str = match &issue.status {
            RemoteStatus::Open => "open".to_string(),
            RemoteStatus::Closed => "closed".to_string(),
            RemoteStatus::Custom(s) => s.clone(),
        };

        // Clone data for rendering
        let issue_id = issue.id.clone();
        let issue_title = issue.title.clone();
        let issue_priority = issue.priority;
        let issue_assignee = issue.assignee.clone();
        let issue_updated = issue.updated_at.clone();
        let issue_body = issue.body.clone();

        // Find linked local ticket
        let linked_ticket_id = all_local_tickets.iter().find_map(|ticket| {
            ticket.remote.as_ref().and_then(|remote_ref| {
                let remote_issue_id =
                    crate::tui::remote::operations::extract_issue_id_from_remote_ref(remote_ref)?;
                if remote_issue_id == issue.id {
                    ticket.id.clone()
                } else {
                    None
                }
            })
        });

        Some(
            element! {
                View(
                    width: 100pct,
                    height: 100pct,
                    flex_direction: FlexDirection::Column,
                    overflow: Overflow::Hidden,
                ) {
                    // Header
                    View(
                        width: 100pct,
                        padding: 1,
                        border_edges: Edges::Bottom,
                        border_style: BorderStyle::Single,
                        border_color: theme.border,
                    ) {
                        View(flex_direction: FlexDirection::Column) {
                            Text(content: issue_id, color: theme.id_color, weight: Weight::Bold)
                            Text(content: issue_title, color: theme.text, weight: Weight::Bold)
                        }
                    }

                    // Metadata
                    View(
                        width: 100pct,
                        padding: 1,
                        flex_direction: FlexDirection::Column,
                    ) {
                        Text(content: format!("Status: {}", status_str), color: Color::Green)
                        Text(content: format!("Priority: {:?}", issue_priority), color: theme.text)
                        Text(content: format!("Assignee: {:?}", issue_assignee), color: theme.text)
                        Text(content: format!("Updated: {}", &issue_updated[..10.min(issue_updated.len())]), color: theme.text)
                        #(linked_ticket_id.as_ref().map(|linked_id| element! {
                            Text(content: format!("Linked: {}", linked_id), color: Color::Cyan)
                        }))
                    }

                    // Body (with mouse wheel scrolling)
                    View(
                        flex_grow: 1.0,
                        width: 100pct,
                        padding: 1,
                        overflow: Overflow::Hidden,
                    ) {
                        Clickable(
                            on_scroll_up: on_scroll_up.clone(),
                            on_scroll_down: on_scroll_down.clone(),
                        ) {
                            TextViewer(
                                text: issue_body,
                                scroll_offset: remote_scroll_offset,
                                has_focus: false,
                                placeholder: Some("No description".to_string()),
                            )
                        }
                    }
                }
            }
            .into_any(),
        )
    } else {
        Some(
            element! {
                View(
                    flex_grow: 1.0,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                ) {
                    Text(content: "No issue selected", color: theme.text_dimmed)
                }
            }
            .into_any(),
        )
    }
}

/// Render local ticket detail
fn render_local_detail(
    selected_local: &Option<TicketMetadata>,
    scroll_offset: usize,
    on_scroll_up: Option<Handler<()>>,
    on_scroll_down: Option<Handler<()>>,
) -> Option<AnyElement<'static>> {
    let theme = theme();

    if let Some(ticket) = selected_local {
        let status = ticket.status.unwrap_or_default();

        // Clone data for rendering
        let ticket_id = ticket.id.clone().unwrap_or_default();
        let ticket_title = ticket.title.clone().unwrap_or_default();
        let ticket_type = ticket.ticket_type;
        let ticket_priority = ticket.priority;

        // Try to read the body content
        let body = if let Some(file_path) = &ticket.file_path {
            match Ticket::new(file_path.clone()) {
                Ok(ticket_handle) => match ticket_handle.read_content() {
                    Ok(content) => extract_ticket_body(&content).unwrap_or_default(),
                    Err(_) => "(error: could not read file)".to_string(),
                },
                Err(_) => "(error: invalid ticket path)".to_string(),
            }
        } else {
            "(file_path is None)".to_string()
        };

        Some(
            element! {
                View(
                    width: 100pct,
                    height: 100pct,
                    flex_direction: FlexDirection::Column,
                    overflow: Overflow::Hidden,
                ) {
                    View(
                        width: 100pct,
                        padding: 1,
                        border_edges: Edges::Bottom,
                        border_style: BorderStyle::Single,
                        border_color: theme.border,
                    ) {
                        View(flex_direction: FlexDirection::Column) {
                            Text(
                                content: ticket_id,
                                color: theme.id_color,
                                weight: Weight::Bold,
                            )
                            Text(
                                content: ticket_title,
                                color: theme.text,
                                weight: Weight::Bold,
                            )
                        }
                    }

                    View(
                        width: 100pct,
                        padding: 1,
                        flex_direction: FlexDirection::Column,
                    ) {
                        Text(content: format!("Status: {}", status), color: theme.status_color(status))
                        Text(content: format!("Type: {:?}", ticket_type), color: theme.text)
                        Text(content: format!("Priority: {:?}", ticket_priority), color: theme.text)
                    }

                    // Body (with mouse wheel scrolling)
                    View(
                        flex_grow: 1.0,
                        width: 100pct,
                        padding: 1,
                        overflow: Overflow::Hidden,
                    ) {
                        Clickable(
                            on_scroll_up: on_scroll_up.clone(),
                            on_scroll_down: on_scroll_down.clone(),
                        ) {
                            TextViewer(
                                text: body,
                                scroll_offset: scroll_offset,
                                has_focus: false,
                                placeholder: Some("No description".to_string()),
                            )
                        }
                    }
                }
            }
            .into_any(),
        )
    } else {
        Some(
            element! {
                View(
                    flex_grow: 1.0,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                ) {
                    Text(content: "No ticket selected", color: theme.text_dimmed)
                }
            }
            .into_any(),
        )
    }
}

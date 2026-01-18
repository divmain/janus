//! Remote TUI detail pane component
//!
//! Displays detailed information about the selected issue or ticket.

use iocraft::prelude::*;

use crate::remote::{RemoteIssue, RemoteStatus};
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
                render_remote_detail(&props.selected_remote)
            } else {
                render_local_detail(&props.selected_local)
            })
        }
    }
}

/// Render remote issue detail
fn render_remote_detail(selected_remote: &Option<RemoteIssue>) -> Option<AnyElement<'static>> {
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
                    }

                    // Body
                    View(
                        flex_grow: 1.0,
                        width: 100pct,
                        padding: 1,
                        overflow: Overflow::Hidden,
                        flex_direction: FlexDirection::Column,
                    ) {
                        #(issue_body.lines().take(15).map(|line| {
                            element! {
                                Text(content: line.to_string(), color: theme.text)
                            }
                        }))
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
fn render_local_detail(selected_local: &Option<TicketMetadata>) -> Option<AnyElement<'static>> {
    let theme = theme();

    if let Some(ticket) = selected_local {
        let status = ticket.status.unwrap_or_default();

        // Clone data for rendering
        let ticket_id = ticket.id.clone().unwrap_or_default();
        let ticket_title = ticket.title.clone().unwrap_or_default();
        let ticket_type = ticket.ticket_type;
        let ticket_priority = ticket.priority;

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

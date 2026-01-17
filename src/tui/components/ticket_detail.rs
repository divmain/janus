//! Ticket detail pane component
//!
//! Displays detailed information about a selected ticket including
//! metadata, dependencies, links, and body content.

use iocraft::prelude::*;

use crate::formatting::{extract_ticket_body, format_date_for_display};
use crate::ticket::Ticket;
use crate::tui::theme::theme;
use crate::types::TicketMetadata;

/// Props for the TicketDetail component
#[derive(Default, Props)]
pub struct TicketDetailProps {
    /// The ticket to display (None shows empty state)
    pub ticket: Option<TicketMetadata>,
    /// Whether the detail pane has focus
    pub has_focus: bool,
}

/// Ticket detail view showing metadata and body
#[component]
pub fn TicketDetail(props: &TicketDetailProps) -> impl Into<AnyElement<'static>> {
    let theme = theme();
    let border_color = if props.has_focus {
        theme.border_focused
    } else {
        theme.border
    };

    let Some(ticket) = props.ticket.clone() else {
        return element! {
            View(
                width: 100pct,
                height: 100pct,
                border_style: BorderStyle::Round,
                border_color: border_color,
                flex_direction: FlexDirection::Column,
                padding: 1,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
            ) {
                Text(
                    content: "No ticket selected",
                    color: theme.text_dimmed,
                )
            }
        };
    };

    // Extract ticket properties
    let id = ticket.id.clone().unwrap_or_else(|| "???".to_string());
    let title = ticket
        .title
        .clone()
        .unwrap_or_else(|| "(no title)".to_string());
    let status = ticket.status.unwrap_or_default();
    let ticket_type = ticket.ticket_type;
    let priority = ticket.priority;
    let created = ticket.created.clone();
    let deps = ticket.deps.clone();
    let links = ticket.links.clone();
    let parent = ticket.parent.clone();

    // Get status and type colors
    let status_color = theme.status_color(status);
    let type_color = ticket_type
        .map(|t| theme.type_color(t))
        .unwrap_or(theme.text);
    let priority_color = priority
        .map(|p| theme.priority_color(p))
        .unwrap_or(theme.text);

    // Format values
    let status_str = status.to_string();
    let type_str = ticket_type
        .map(|t| t.to_string())
        .unwrap_or_else(|| "-".to_string());
    let priority_str = priority
        .map(|p| format!("P{}", p.as_num()))
        .unwrap_or_else(|| "-".to_string());
    let created_str = created
        .map(|c| format_date_for_display(&c))
        .unwrap_or_else(|| "-".to_string());
    let deps_str = if deps.is_empty() {
        "-".to_string()
    } else {
        deps.join(", ")
    };
    let links_str = if links.is_empty() {
        "-".to_string()
    } else {
        links.join(", ")
    };
    let parent_str = parent.unwrap_or_else(|| "-".to_string());

    // Try to read the body content
    let body = if let Some(file_path) = &ticket.file_path {
        let ticket_handle = Ticket::new(file_path.clone());
        ticket_handle
            .read_content()
            .ok()
            .and_then(|content| extract_ticket_body(&content))
            .unwrap_or_default()
    } else {
        String::new()
    };

    element! {
        View(
            width: 100pct,
            height: 100pct,
            border_style: BorderStyle::Round,
            border_color: border_color,
            flex_direction: FlexDirection::Column,
            overflow: Overflow::Hidden,
        ) {
            // Header with ID and Title
            View(
                width: 100pct,
                padding: 1,
                border_edges: Edges::Bottom,
                border_style: BorderStyle::Single,
                border_color: theme.border,
            ) {
                View(flex_direction: FlexDirection::Column) {
                    // ID
                    Text(
                        content: id,
                        color: theme.id_color,
                        weight: Weight::Bold,
                    )
                    // Title (may be long)
                    Text(
                        content: title,
                        color: theme.text,
                        weight: Weight::Bold,
                    )
                }
            }

            // Metadata section
            View(
                width: 100pct,
                padding_left: 1,
                padding_right: 1,
                padding_top: 1,
                flex_direction: FlexDirection::Column,
                gap: 0,
            ) {
                // Row 1: Status and Type
                View(flex_direction: FlexDirection::Row, height: 1) {
                    View(width: 50pct, flex_direction: FlexDirection::Row) {
                        Text(content: "Status: ", color: theme.text_dimmed)
                        Text(content: status_str, color: status_color)
                    }
                    View(width: 50pct, flex_direction: FlexDirection::Row) {
                        Text(content: "Type: ", color: theme.text_dimmed)
                        Text(content: type_str, color: type_color)
                    }
                }

                // Row 2: Priority and Created
                View(flex_direction: FlexDirection::Row, height: 1) {
                    View(width: 50pct, flex_direction: FlexDirection::Row) {
                        Text(content: "Priority: ", color: theme.text_dimmed)
                        Text(content: priority_str, color: priority_color)
                    }
                    View(width: 50pct, flex_direction: FlexDirection::Row) {
                        Text(content: "Created: ", color: theme.text_dimmed)
                        Text(content: created_str.clone(), color: theme.text)
                    }
                }

                // Row 3: Parent
                View(flex_direction: FlexDirection::Row, height: 1) {
                    View(width: 50pct, flex_direction: FlexDirection::Row) {
                        Text(content: "Parent: ", color: theme.text_dimmed)
                        Text(content: parent_str, color: theme.id_color)
                    }
                }

                // Row 4: Dependencies
                View(flex_direction: FlexDirection::Row, height: 1) {
                    Text(content: "Deps: ", color: theme.text_dimmed)
                    Text(content: deps_str, color: theme.id_color)
                }

                // Row 5: Links
                View(flex_direction: FlexDirection::Row, height: 1) {
                    Text(content: "Links: ", color: theme.text_dimmed)
                    Text(content: links_str, color: theme.id_color)
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

            // Body content (scrollable)
            View(
                flex_grow: 1.0,
                width: 100pct,
                padding: 1,
                overflow: Overflow::Hidden,
                flex_direction: FlexDirection::Column,
            ) {
                #(body.lines().take(20).map(|line| {
                    let line_owned = line.to_string();
                    element! {
                        Text(content: line_owned, color: theme.text)
                    }
                }))
            }
        }
    }
}

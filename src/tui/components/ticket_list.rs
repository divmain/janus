//! Scrollable ticket list component
//!
//! Displays a list of tickets with selection highlighting, fuzzy match
//! highlighting, and scrolling support.

use iocraft::prelude::*;

use crate::tui::search::FilteredTicket;
use crate::tui::theme::theme;
use crate::types::TicketStatus;
use crate::utils::truncate_string;

/// Props for the TicketList component
#[derive(Default, Props)]
pub struct TicketListProps {
    /// List of filtered tickets to display
    pub tickets: Vec<FilteredTicket>,
    /// Index of the currently selected ticket
    pub selected_index: usize,
    /// Current scroll offset (first visible ticket index)
    pub scroll_offset: usize,
    /// Whether the list has focus
    pub has_focus: bool,
    /// Number of visible rows
    pub visible_height: usize,
}

/// Scrollable ticket list with selection
#[component]
pub fn TicketList(props: &TicketListProps) -> impl Into<AnyElement<'static>> {
    let theme = theme();
    let border_color = if props.has_focus {
        theme.border_focused
    } else {
        theme.border
    };

    // Calculate which tickets to show
    let start = props.scroll_offset;
    let end = (start + props.visible_height).min(props.tickets.len());
    let visible_tickets: Vec<_> = props.tickets[start..end].to_vec();

    // Track if we need scroll indicators
    let has_more_above = start > 0;
    let has_more_below = end < props.tickets.len();

    element! {
        View(
            width: 100pct,
            height: 100pct,
            flex_direction: FlexDirection::Column,
            border_style: BorderStyle::Round,
            border_color: border_color,
        ) {
            // "More above" indicator
            #(if has_more_above {
                Some(element! {
                    View(height: 1, padding_left: 1) {
                        Text(
                            content: format!("  {} more above", start),
                            color: theme.text_dimmed,
                        )
                    }
                })
            } else {
                None
            })

            // Ticket rows
            #(visible_tickets.iter().enumerate().map(|(i, ft)| {
                let actual_index = start + i;
                let is_selected = actual_index == props.selected_index;
                element! {
                    TicketRow(
                        ticket: ft.clone(),
                        is_selected: is_selected,
                        has_focus: props.has_focus && is_selected,
                    )
                }
            }))

            // Fill remaining space
            View(flex_grow: 1.0)

            // "More below" indicator
            #(if has_more_below {
                Some(element! {
                    View(height: 1, padding_left: 1) {
                        Text(
                            content: format!("  {} more below", props.tickets.len() - end),
                            color: theme.text_dimmed,
                        )
                    }
                })
            } else {
                None
            })
        }
    }
}

/// Props for a single ticket row
#[derive(Default, Props)]
pub struct TicketRowProps {
    /// The filtered ticket to display
    pub ticket: FilteredTicket,
    /// Whether this row is selected
    pub is_selected: bool,
    /// Whether this row has focus
    pub has_focus: bool,
}

/// Single ticket row in the list
#[component]
pub fn TicketRow(props: &TicketRowProps) -> impl Into<AnyElement<'static>> {
    let theme = theme();
    let ticket = &props.ticket.ticket;

    // Get ticket properties
    let id = ticket.id.as_deref().unwrap_or("???");
    let title = ticket.title.as_deref().unwrap_or("(no title)");
    let status = ticket.status.unwrap_or_default();

    // Colors
    let status_color = theme.status_color(status);
    let bg_color = if props.is_selected {
        Some(theme.highlight)
    } else {
        None
    };
    let text_color = if props.is_selected {
        Color::White
    } else {
        theme.text
    };

    // Selection indicator
    let indicator = if props.is_selected { ">" } else { " " };

    // Format status
    let status_str = match status {
        TicketStatus::New => "new",
        TicketStatus::Next => "nxt",
        TicketStatus::InProgress => "wip",
        TicketStatus::Complete => "don",
        TicketStatus::Cancelled => "can",
    };

    // Truncate title if needed (using char-safe truncation)
    let max_title_len = 20;
    let truncated_title = truncate_string(title, max_title_len);

    element! {
        View(
            height: 1,
            width: 100pct,
            flex_direction: FlexDirection::Row,
            padding_left: 1,
            padding_right: 1,
            background_color: bg_color,
        ) {
            // Selection indicator
            Text(content: indicator, color: text_color)

            // Ticket ID
            Text(
                content: format!(" {:<8}", id),
                color: if props.is_selected { Color::White } else { theme.id_color },
            )

            // Status badge
            Text(
                content: format!(" [{}]", status_str),
                color: if props.is_selected { Color::White } else { status_color },
            )

            // Title (with possible fuzzy highlighting)
            Text(
                content: format!(" {}", truncated_title),
                color: text_color,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{TicketMetadata, TicketPriority, TicketType};

    #[allow(dead_code)]
    fn make_filtered_ticket(id: &str, title: &str) -> FilteredTicket {
        FilteredTicket {
            ticket: TicketMetadata {
                id: Some(id.to_string()),
                title: Some(title.to_string()),
                status: Some(TicketStatus::New),
                priority: Some(TicketPriority::P2),
                ticket_type: Some(TicketType::Task),
                ..Default::default()
            },
            score: 0,
            title_indices: vec![],
        }
    }

    #[test]
    fn test_title_truncation() {
        let long_title = "This is a very long title that should be truncated";
        let truncated = truncate_string(long_title, 20);
        assert_eq!(truncated, "This is a very lo...");
    }

    #[test]
    fn test_title_truncation_multibyte() {
        // Test with multi-byte characters
        let multibyte = "Привет мир, это тест"; // Russian text
        let truncated = truncate_string(multibyte, 10);
        assert_eq!(truncated, "Привет ...");
    }
}

//! Scrollable ticket list component
//!
//! Displays a list of tickets with selection highlighting, fuzzy match
//! highlighting, and scrolling support.

use iocraft::prelude::*;

use crate::tui::search::FilteredTicket;
use crate::tui::theme::theme;
use crate::types::TicketStatus;

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
    /// Number of visible rows (required for scroll indicator calculations)
    /// NOTE: This is passed from the parent because scroll logic needs to know
    /// how many rows fit in the visible area for "X more above/below" indicators.
    /// The component uses `height: 100pct` for declarative layout, but scroll
    /// state management needs the actual row count.
    pub visible_height: usize,
    /// Whether a search is currently in progress
    pub searching: bool,
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

    // If searching, show a loading indicator instead of tickets
    if props.searching {
        return element! {
            View(
                width: 100pct,
                height: 100pct,
                flex_direction: FlexDirection::Column,
                border_style: BorderStyle::Round,
                border_color: border_color,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
            ) {
                Text(
                    content: "Searching...",
                    color: theme.text_dimmed,
                )
            }
        };
    }

    // Calculate which tickets to show, accounting for scroll indicator lines
    let start = props.scroll_offset;
    let total = props.tickets.len();

    // Check if we need the "more above" indicator
    let has_more_above = start > 0;

    // Calculate how many ticket rows we can fit
    // Start with visible_height and subtract space for indicators
    let above_indicator_lines = if has_more_above { 1 } else { 0 };

    // Tentatively calculate how many tickets we can show
    let tentative_rows = props.visible_height.saturating_sub(above_indicator_lines);
    let tentative_end = (start + tentative_rows).min(total);

    // Check if we'll need the "more below" indicator
    let has_more_below = tentative_end < total;
    let below_indicator_lines = if has_more_below { 1 } else { 0 };

    // Final calculation: subtract both indicators from available space
    let available_rows = props
        .visible_height
        .saturating_sub(above_indicator_lines + below_indicator_lines);
    let end = (start + available_rows).min(total);
    let visible_tickets: Vec<_> = props.tickets[start..end].to_vec();

    // Recalculate has_more_below with final end (may have changed)
    let has_more_below = end < total;

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
        theme.highlight_text
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

    element! {
        View(
            height: 1,
            width: 100pct,
            flex_direction: FlexDirection::Row,
            padding_left: 1,
            padding_right: 1,
            background_color: bg_color,
        ) {
            // Selection indicator - fixed width, won't shrink
            View(width: 2, flex_shrink: 0.0) {
                Text(content: indicator, color: text_color)
            }

            // Ticket ID - fixed width, won't shrink
            View(width: 9, flex_shrink: 0.0) {
                Text(
                    content: format!("{:<8}", id),
                    color: if props.is_selected { theme.highlight_text } else { theme.id_color },
                )
            }

            // Status badge - fixed width, won't shrink
            View(width: 6, flex_shrink: 0.0) {
                Text(
                    content: format!("[{}]", status_str),
                    color: if props.is_selected { theme.highlight_text } else { status_color },
                )
            }

            // Title - flexible, takes remaining space and truncates via overflow
            View(flex_grow: 1.0, overflow: Overflow::Hidden) {
                Text(
                    content: format!(" {}", title),
                    color: text_color,
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{TicketMetadata, TicketPriority, TicketType};
    use std::sync::Arc;

    #[allow(dead_code)]
    fn make_filtered_ticket(id: &str, title: &str) -> FilteredTicket {
        FilteredTicket {
            ticket: Arc::new(TicketMetadata {
                id: Some(id.to_string()),
                title: Some(title.to_string()),
                status: Some(TicketStatus::New),
                priority: Some(TicketPriority::P2),
                ticket_type: Some(TicketType::Task),
                ..Default::default()
            }),
            score: 0,
            title_indices: vec![],
        }
    }
}

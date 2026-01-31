//! Ticket card component for kanban board
//!
//! A compact card view showing ticket id, title (truncated), priority badge,
//! and type indicator.

use iocraft::prelude::*;

use crate::tui::components::Clickable;
use crate::tui::theme::theme;
use crate::types::{TicketMetadata, TicketPriority, TicketType};
use crate::utils::wrap_text_lines;

/// Props for the TicketCard component
#[derive(Default, Props)]
pub struct TicketCardProps {
    /// The ticket to display
    pub ticket: TicketMetadata,
    /// Whether this card is selected
    pub is_selected: bool,
    /// Available width for the card content (in characters)
    pub width: Option<u32>,
    /// Handler called when card is clicked (optional)
    /// The usize parameter is the row index of the clicked card
    pub on_click: Option<Handler<usize>>,
    /// The row index of this card within its column
    pub row_idx: usize,
}

/// Compact ticket card for kanban board columns
///
/// Layout:
/// ```text
/// +-------------------+
/// | j-a1b2            |
/// | Fix the login bug |
/// | that prevents     |
/// | users from...     |
/// | P1  bug           |
/// +-------------------+
/// ```
#[component]
pub fn TicketCard(props: &TicketCardProps) -> impl Into<AnyElement<'static>> {
    let theme = theme();
    let ticket = &props.ticket;

    // Get ticket properties
    let id = ticket.id.as_deref().unwrap_or("???");
    let title = ticket.title.as_deref().unwrap_or("(no title)");
    let priority = ticket.priority.unwrap_or(TicketPriority::P2);
    let ticket_type = ticket.ticket_type.unwrap_or(TicketType::Task);

    // Colors
    let border_color = if props.is_selected {
        theme.border_focused
    } else {
        theme.border
    };
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

    // Priority indicator
    let priority_str = format!("P{}", priority.as_num());
    let priority_color = if props.is_selected {
        theme.highlight_text
    } else {
        theme.priority_color(priority)
    };

    // Type indicator
    let type_str = ticket_type.to_string();
    let type_color = if props.is_selected {
        theme.highlight_text
    } else {
        theme.type_color(ticket_type)
    };

    // Calculate available width for title text
    // Card has padding_left: 1, padding_right: 1, and border chars (2 total for round border)
    // So available text width = card_width - 4
    let default_width = 20u32; // Reasonable default if width not provided
    let card_width = props.width.unwrap_or(default_width);
    let title_width = card_width.saturating_sub(4) as usize;
    let title_width = title_width.max(8); // Minimum 8 chars to be useful

    // Wrap title to up to 3 lines
    let title_lines = wrap_text_lines(title, title_width, 3);

    // Selection indicator character
    let indicator = if props.is_selected { ">" } else { " " };

    // Wrap with Clickable if an on_click handler is provided
    // Convert Handler<usize> to Handler<()> by wrapping it with the row_idx
    let click_handler = props.on_click.clone().map(|h| {
        let handler = h;
        let idx = props.row_idx;
        Handler::from(move |_: ()| {
            handler(idx);
        })
    });

    if let Some(on_click) = click_handler {
        element! {
            View(margin_top: 1) {
                Clickable(on_click: Some(on_click)) {
                    View(
                        width: 100pct,
                        min_height: 3,
                        flex_direction: FlexDirection::Column,
                        border_style: BorderStyle::Round,
                        border_color: border_color,
                        background_color: bg_color,
                        padding_left: 1,
                        padding_right: 1,
                    ) {
                        // ID row with selection indicator
                        View(flex_direction: FlexDirection::Row) {
                            Text(
                                content: indicator,
                                color: text_color,
                                weight: Weight::Bold,
                            )
                            Text(
                                content: id,
                                color: if props.is_selected { theme.highlight_text } else { theme.id_color },
                                weight: Weight::Bold,
                            )
                        }
                        // Title rows (up to 3 lines)
                        #(title_lines.iter().map(|line| {
                            element! {
                                Text(
                                    content: line.clone(),
                                    color: text_color,
                                )
                            }
                        }))
                        // Priority and type row
                        View(flex_direction: FlexDirection::Row, gap: 1) {
                            Text(
                                content: priority_str,
                                color: priority_color,
                                weight: if priority.as_num() <= 1 { Weight::Bold } else { Weight::Normal },
                            )
                            Text(
                                content: type_str,
                                color: type_color,
                            )
                        }
                    }
                }
            }
        }
    } else {
        element! {
            View(
                width: 100pct,
                min_height: 3,
                flex_direction: FlexDirection::Column,
                border_style: BorderStyle::Round,
                border_color: border_color,
                background_color: bg_color,
                padding_left: 1,
                padding_right: 1,
            ) {
                // ID row with selection indicator
                View(flex_direction: FlexDirection::Row) {
                    Text(
                        content: indicator,
                        color: text_color,
                        weight: Weight::Bold,
                    )
                    Text(
                        content: id,
                        color: if props.is_selected { theme.highlight_text } else { theme.id_color },
                        weight: Weight::Bold,
                    )
                }
                // Title rows (up to 3 lines)
                #(title_lines.iter().map(|line| {
                    element! {
                        Text(
                            content: line.clone(),
                            color: text_color,
                        )
                    }
                }))
                // Priority and type row
                View(flex_direction: FlexDirection::Row, gap: 1) {
                    Text(
                        content: priority_str,
                        color: priority_color,
                        weight: if priority.as_num() <= 1 { Weight::Bold } else { Weight::Normal },
                    )
                    Text(
                        content: type_str,
                        color: type_color,
                    )
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TicketStatus;

    fn make_ticket(id: &str, title: &str, priority: TicketPriority) -> TicketMetadata {
        TicketMetadata {
            id: Some(id.to_string()),
            title: Some(title.to_string()),
            status: Some(TicketStatus::New),
            priority: Some(priority),
            ticket_type: Some(TicketType::Task),
            ..Default::default()
        }
    }

    #[test]
    fn test_priority_display() {
        let ticket = make_ticket("j-a1b2", "Test", TicketPriority::P0);
        assert_eq!(format!("P{}", ticket.priority.unwrap().as_num()), "P0");
    }

    #[test]
    fn test_type_display() {
        let ticket = make_ticket("j-a1b2", "Test", TicketPriority::P2);
        assert_eq!(ticket.ticket_type.unwrap().to_string(), "task");
    }
}

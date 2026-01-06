//! Ticket card component for kanban board
//!
//! A compact card view showing ticket id, title (truncated), priority badge,
//! and type indicator.

use iocraft::prelude::*;

use crate::tui::theme::theme;
use crate::types::{TicketMetadata, TicketPriority, TicketType};
use crate::utils::truncate_string;

/// Props for the TicketCard component
#[derive(Default, Props)]
pub struct TicketCardProps {
    /// The ticket to display
    pub ticket: TicketMetadata,
    /// Whether this card is selected
    pub is_selected: bool,
}

/// Compact ticket card for kanban board columns
///
/// Layout:
/// ```text
/// +---------------+
/// | j-a1b2        |
/// | Fix bug in... |
/// | P1  bug       |
/// +---------------+
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
        Color::White
    } else {
        theme.text
    };

    // Priority indicator
    let priority_str = format!("P{}", priority.as_num());
    let priority_color = if props.is_selected {
        Color::White
    } else {
        theme.priority_color(priority)
    };

    // Type indicator
    let type_str = ticket_type.to_string();
    let type_color = if props.is_selected {
        Color::White
    } else {
        theme.type_color(ticket_type)
    };

    // Truncate title if needed (using char-safe truncation)
    let max_title_len = 15;
    let truncated_title = truncate_string(title, max_title_len);

    // Selection indicator character
    let indicator = if props.is_selected { ">" } else { " " };

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
                    color: if props.is_selected { Color::White } else { theme.id_color },
                    weight: Weight::Bold,
                )
            }
            // Title row
            Text(
                content: truncated_title,
                color: text_color,
            )
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
    fn test_title_truncation_logic() {
        let long_title = "This is a very long title";
        let truncated = truncate_string(long_title, 15);
        assert_eq!(truncated, "This is a ve...");
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

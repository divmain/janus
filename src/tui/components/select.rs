//! Compact inline selector component for enum fields
//!
//! A component that allows cycling through a list of options using
//! left/right arrows or enter key. Displays as: Label: ◀ value ▶

use iocraft::prelude::*;

use crate::tui::components::Clickable;
use crate::tui::theme::theme;
use crate::types::{TicketPriority, TicketStatus, TicketType};

/// Props for the Select component
#[derive(Default, Props)]
pub struct SelectProps<'a> {
    /// Label to display before the selector
    pub label: Option<&'a str>,
    /// List of options to choose from
    pub options: Vec<String>,
    /// Index of the currently selected option
    pub selected_index: usize,
    /// Whether the selector has focus
    pub has_focus: bool,
    /// Optional color for the value (for semantic coloring like status/type/priority)
    pub value_color: Option<Color>,
    /// Handler invoked when left arrow is clicked (cycle backward)
    pub on_prev: Option<Handler<()>>,
    /// Handler invoked when right arrow is clicked (cycle forward)
    pub on_next: Option<Handler<()>>,
}

/// Compact inline selector component with arrow indicators
///
/// Renders as: Label: ◀ value ▶
/// Arrows indicate the value can be cycled with left/right keys.
#[component]
pub fn Select<'a>(props: &SelectProps<'a>) -> impl Into<AnyElement<'a>> {
    let theme = theme();

    let label_color = if props.has_focus {
        theme.border_focused
    } else {
        theme.text_dimmed
    };

    let arrow_color = if props.has_focus {
        theme.border_focused
    } else {
        theme.text_dimmed
    };

    let value_color = props.value_color.unwrap_or(theme.text);

    let current_value = props
        .options
        .get(props.selected_index)
        .cloned()
        .unwrap_or_default();

    element! {
        View(flex_direction: FlexDirection::Row, gap: 1) {
            #(props.label.map(|label| element! {
                Text(
                    content: format!("{}:", label),
                    color: label_color,
                )
            }))
            Clickable(
                on_click: props.on_prev.clone(),
            ) {
                Text(
                    content: "◀",
                    color: arrow_color,
                )
            }
            Text(
                content: current_value,
                color: value_color,
            )
            Clickable(
                on_click: props.on_next.clone(),
            ) {
                Text(
                    content: "▶",
                    color: arrow_color,
                )
            }
        }
    }
}

/// Helper trait for types that can be used with Select
pub trait Selectable: Sized + Clone + Copy + 'static {
    /// Get all possible values for this type
    fn all_values() -> Vec<Self>;
    /// Get the display string for this value
    fn display(&self) -> String;
    /// Get the index of this value in all_values
    fn index(&self) -> usize;
    /// Get the value at the given index
    fn from_index(index: usize) -> Option<Self>;
    /// Get the next value (wrapping)
    fn next(&self) -> Self {
        let values = Self::all_values();
        let next_idx = (self.index() + 1) % values.len();
        values[next_idx]
    }
    /// Get the previous value (wrapping)
    fn prev(&self) -> Self {
        let values = Self::all_values();
        let prev_idx = if self.index() == 0 {
            values.len() - 1
        } else {
            self.index() - 1
        };
        values[prev_idx]
    }
}

impl Selectable for TicketStatus {
    fn all_values() -> Vec<Self> {
        vec![
            TicketStatus::New,
            TicketStatus::Next,
            TicketStatus::InProgress,
            TicketStatus::Complete,
            TicketStatus::Cancelled,
        ]
    }

    fn display(&self) -> String {
        self.to_string()
    }

    fn index(&self) -> usize {
        match self {
            TicketStatus::New => 0,
            TicketStatus::Next => 1,
            TicketStatus::InProgress => 2,
            TicketStatus::Complete => 3,
            TicketStatus::Cancelled => 4,
        }
    }

    fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(TicketStatus::New),
            1 => Some(TicketStatus::Next),
            2 => Some(TicketStatus::InProgress),
            3 => Some(TicketStatus::Complete),
            4 => Some(TicketStatus::Cancelled),
            _ => None,
        }
    }
}

impl Selectable for TicketType {
    fn all_values() -> Vec<Self> {
        vec![
            TicketType::Bug,
            TicketType::Feature,
            TicketType::Task,
            TicketType::Epic,
            TicketType::Chore,
        ]
    }

    fn display(&self) -> String {
        self.to_string()
    }

    fn index(&self) -> usize {
        match self {
            TicketType::Bug => 0,
            TicketType::Feature => 1,
            TicketType::Task => 2,
            TicketType::Epic => 3,
            TicketType::Chore => 4,
        }
    }

    fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(TicketType::Bug),
            1 => Some(TicketType::Feature),
            2 => Some(TicketType::Task),
            3 => Some(TicketType::Epic),
            4 => Some(TicketType::Chore),
            _ => None,
        }
    }
}

impl Selectable for TicketPriority {
    fn all_values() -> Vec<Self> {
        vec![
            TicketPriority::P0,
            TicketPriority::P1,
            TicketPriority::P2,
            TicketPriority::P3,
            TicketPriority::P4,
        ]
    }

    fn display(&self) -> String {
        format!("P{}", self.as_num())
    }

    fn index(&self) -> usize {
        self.as_num() as usize
    }

    fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(TicketPriority::P0),
            1 => Some(TicketPriority::P1),
            2 => Some(TicketPriority::P2),
            3 => Some(TicketPriority::P3),
            4 => Some(TicketPriority::P4),
            _ => None,
        }
    }
}

/// Get option strings for a selectable type
pub fn options_for<T: Selectable>() -> Vec<String> {
    T::all_values().iter().map(|v| v.display()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_selectable() {
        assert_eq!(TicketStatus::New.index(), 0);
        assert_eq!(TicketStatus::New.next(), TicketStatus::Next);
        assert_eq!(TicketStatus::New.prev(), TicketStatus::Cancelled);
        assert_eq!(TicketStatus::from_index(2), Some(TicketStatus::InProgress));
    }

    #[test]
    fn test_type_selectable() {
        assert_eq!(TicketType::Bug.index(), 0);
        assert_eq!(TicketType::Bug.next(), TicketType::Feature);
        assert_eq!(TicketType::Chore.next(), TicketType::Bug);
    }

    #[test]
    fn test_priority_selectable() {
        assert_eq!(TicketPriority::P0.display(), "P0");
        assert_eq!(TicketPriority::P2.index(), 2);
        assert_eq!(TicketPriority::P4.next(), TicketPriority::P0);
    }

    #[test]
    fn test_options_for() {
        let status_opts = options_for::<TicketStatus>();
        assert_eq!(status_opts.len(), 5);
        assert_eq!(status_opts[0], "new");

        let priority_opts = options_for::<TicketPriority>();
        assert_eq!(priority_opts.len(), 5);
        assert_eq!(priority_opts[0], "P0");
    }
}

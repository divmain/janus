//! Theme system for TUI colors and styles
//!
//! Defines color constants consistent with the existing CLI output (commands/mod.rs).

use iocraft::prelude::Color;

use crate::types::{TicketPriority, TicketStatus, TicketType};

/// Theme configuration for TUI components
#[derive(Debug, Clone)]
pub struct Theme {
    // Status colors (consistent with existing CLI)
    pub status_new: Color,
    pub status_next: Color,
    pub status_in_progress: Color,
    pub status_complete: Color,
    pub status_cancelled: Color,

    // Priority colors
    pub priority_p0: Color,
    pub priority_p1: Color,
    pub priority_default: Color,

    // Type colors
    pub type_bug: Color,
    pub type_feature: Color,
    pub type_task: Color,
    pub type_epic: Color,
    pub type_chore: Color,

    // UI colors
    pub border: Color,
    pub border_focused: Color,
    pub background: Color,
    pub text: Color,
    pub text_dimmed: Color,
    pub highlight: Color,
    pub search_match: Color,
    pub id_color: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            // Status colors (matching commands/mod.rs)
            status_new: Color::Yellow,
            status_next: Color::Magenta,
            status_in_progress: Color::Cyan,
            status_complete: Color::Green,
            status_cancelled: Color::Rgb {
                r: 120,
                g: 120,
                b: 120,
            },

            // Priority colors
            priority_p0: Color::Red,
            priority_p1: Color::Yellow,
            priority_default: Color::White,

            // Type colors
            type_bug: Color::Red,
            type_feature: Color::Green,
            type_task: Color::Blue,
            type_epic: Color::Magenta,
            type_chore: Color::Rgb {
                r: 120,
                g: 120,
                b: 120,
            },

            // UI colors
            border: Color::Rgb {
                r: 120,
                g: 120,
                b: 120,
            },
            border_focused: Color::Blue,
            background: Color::Reset,
            text: Color::White,
            text_dimmed: Color::Rgb {
                r: 120,
                g: 120,
                b: 120,
            },
            highlight: Color::Blue,
            search_match: Color::Yellow,
            id_color: Color::Cyan,
        }
    }
}

impl Theme {
    /// Get the color for a ticket status
    pub fn status_color(&self, status: TicketStatus) -> Color {
        match status {
            TicketStatus::New => self.status_new,
            TicketStatus::Next => self.status_next,
            TicketStatus::InProgress => self.status_in_progress,
            TicketStatus::Complete => self.status_complete,
            TicketStatus::Cancelled => self.status_cancelled,
        }
    }

    /// Get the color for a ticket priority
    pub fn priority_color(&self, priority: TicketPriority) -> Color {
        match priority {
            TicketPriority::P0 => self.priority_p0,
            TicketPriority::P1 => self.priority_p1,
            _ => self.priority_default,
        }
    }

    /// Get the color for a ticket type
    pub fn type_color(&self, ticket_type: TicketType) -> Color {
        match ticket_type {
            TicketType::Bug => self.type_bug,
            TicketType::Feature => self.type_feature,
            TicketType::Task => self.type_task,
            TicketType::Epic => self.type_epic,
            TicketType::Chore => self.type_chore,
        }
    }
}

/// Global theme instance
pub static THEME: std::sync::LazyLock<Theme> = std::sync::LazyLock::new(Theme::default);

/// Get a reference to the global theme
pub fn theme() -> &'static Theme {
    &THEME
}

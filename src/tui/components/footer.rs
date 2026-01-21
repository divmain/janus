//! Keyboard shortcuts bar component
//!
//! Displays available keyboard shortcuts at the bottom of the screen.

use iocraft::prelude::*;

use crate::tui::theme::theme;

/// A single keyboard shortcut entry
#[derive(Debug, Clone)]
pub struct Shortcut {
    /// The key or key combination (e.g., "q", "Ctrl+S", "Tab")
    pub key: String,
    /// Description of the action (e.g., "Quit", "Save", "Next field")
    pub action: String,
}

impl Shortcut {
    /// Create a new shortcut
    pub fn new(key: impl Into<String>, action: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            action: action.into(),
        }
    }
}

/// Props for the Footer component
#[derive(Default, Props)]
pub struct FooterProps {
    /// List of keyboard shortcuts to display
    pub shortcuts: Vec<Shortcut>,
}

/// Keyboard shortcuts bar at the bottom of the screen
#[component]
pub fn Footer(props: &FooterProps) -> impl Into<AnyElement<'static>> {
    let theme = theme();

    element! {
        View(
            width: 100pct,
            height: 1,
            flex_direction: FlexDirection::Row,
            padding_left: 1,
            padding_right: 1,
            gap: 2,
            background_color: theme.border,
        ) {
            #(props.shortcuts.iter().map(|shortcut| {
                let key = shortcut.key.clone();
                let action = shortcut.action.clone();
                element! {
                    View(flex_direction: FlexDirection::Row) {
                        Text(
                            content: format!("[{}]", key),
                            color: theme.highlight,
                            weight: Weight::Bold,
                        )
                        Text(
                            content: format!(" {}", action),
                            color: theme.text,
                        )
                    }
                }
            }))
        }
    }
}

/// Shortcuts for the issue browser list pane
pub fn browser_shortcuts() -> Vec<Shortcut> {
    vec![
        Shortcut::new("j/k", "Up/Down"),
        Shortcut::new("g/G", "Top/Bottom"),
        Shortcut::new("/", "Search"),
        Shortcut::new("e", "Edit"),
        Shortcut::new("s", "Cycle Status"),
        Shortcut::new("n", "New Ticket"),
        Shortcut::new("Tab", "Switch Pane"),
        Shortcut::new("q", "Quit"),
    ]
}

/// Shortcuts for the kanban board
pub fn board_shortcuts() -> Vec<Shortcut> {
    vec![
        Shortcut::new("h/l", "Column"),
        Shortcut::new("j/k", "Card"),
        Shortcut::new("/", "Search"),
        Shortcut::new("e", "Edit"),
        Shortcut::new("s/S", "Move Right/Left"),
        Shortcut::new("n", "New Ticket"),
        Shortcut::new("1-5", "Toggle Column"),
        Shortcut::new("q", "Quit"),
    ]
}

/// Shortcuts for the edit form
pub fn edit_shortcuts() -> Vec<Shortcut> {
    vec![
        Shortcut::new("Tab", "Next Field"),
        Shortcut::new("S-Tab", "Prev Field"),
        Shortcut::new("C-s", "Save"),
        Shortcut::new("Esc", "Cancel"),
    ]
}

/// Shortcuts for search mode
pub fn search_shortcuts() -> Vec<Shortcut> {
    vec![
        Shortcut::new("Enter", "Apply Search"),
        Shortcut::new("Tab", "Exit Search"),
        Shortcut::new("Esc", "Clear & Exit"),
        Shortcut::new("C-q", "Quit"),
    ]
}

/// Shortcuts shown when empty state is displayed
pub fn empty_shortcuts() -> Vec<Shortcut> {
    vec![Shortcut::new("n", "New Ticket"), Shortcut::new("q", "Quit")]
}

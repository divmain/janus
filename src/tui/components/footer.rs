//! Keyboard shortcuts bar component
//!
//! Displays available keyboard shortcuts at the bottom of the screen.

use iocraft::prelude::*;

use super::shortcuts::ShortcutsBuilder;
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
            min_height: 1,
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            flex_shrink: 0.0,
            padding_left: 1,
            padding_right: 1,
            column_gap: 2,
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
    ShortcutsBuilder::new()
        .with_navigation()
        .with_search()
        .with_edit()
        .with_quit()
        .add("s", "Cycle Status")
        .add("Tab", "Switch Pane")
        .add("C-t", "Triage")
        .add("y", "Copy ID")
        .build()
}

/// Shortcuts for the kanban board
pub fn board_shortcuts() -> Vec<Shortcut> {
    ShortcutsBuilder::new()
        .with_navigation()
        .with_search()
        .with_edit()
        .with_quit()
        .add("h/l", "Column")
        .add("s/S", "Move Right/Left")
        .add("1-5", "Toggle Column")
        .add("y", "Copy ID")
        .build()
}

/// Shortcuts for the edit form
pub fn edit_shortcuts() -> Vec<Shortcut> {
    ShortcutsBuilder::new()
        .add("Tab", "Next Field")
        .add("S-Tab", "Prev Field")
        .add("C-s", "Save")
        .add("Esc", "Cancel")
        .build()
}

/// Shortcuts for search mode
pub fn search_shortcuts() -> Vec<Shortcut> {
    ShortcutsBuilder::new()
        .add("Enter", "Apply Search")
        .add("Tab", "Exit Search")
        .add("Esc", "Clear & Exit")
        .add("C-q", "Quit")
        .build()
}

/// Shortcuts shown when empty state is displayed
pub fn empty_shortcuts() -> Vec<Shortcut> {
    ShortcutsBuilder::new()
        .add("n", "New Ticket")
        .add("C-q", "Quit")
        .build()
}

/// Shortcuts for triage mode
pub fn triage_shortcuts() -> Vec<Shortcut> {
    ShortcutsBuilder::new()
        .with_navigation()
        .with_search()
        .add("t", "Mark Triaged")
        .add("n", "Add Note")
        .add("c", "Cancel")
        .add("C-t", "Exit Triage")
        .add("C-q", "Quit")
        .build()
}

// =============================================================================
// Modal-specific shortcuts
// =============================================================================

/// Shortcuts for the help modal
pub fn help_modal_shortcuts() -> Vec<Shortcut> {
    ShortcutsBuilder::new()
        .add("j/k", "Scroll")
        .add("g/G", "Top/Bottom")
        .add("Esc", "Close")
        .add("?", "Close")
        .build()
}

/// Shortcuts for the error detail modal
pub fn error_modal_shortcuts() -> Vec<Shortcut> {
    ShortcutsBuilder::new().add("Esc", "Close").build()
}

/// Shortcuts for the filter modal
pub fn filter_modal_shortcuts() -> Vec<Shortcut> {
    ShortcutsBuilder::new()
        .add("Tab/j/k", "Navigate")
        .add("Enter", "Toggle/Edit")
        .add("x", "Clear")
        .add("Esc", "Cancel")
        .build()
}

/// Shortcuts for the sync preview modal
pub fn sync_preview_shortcuts() -> Vec<Shortcut> {
    ShortcutsBuilder::new()
        .add("y", "Accept")
        .add("n", "Skip")
        .add("a", "Accept All")
        .add("c", "Cancel")
        .build()
}

/// Shortcuts for the confirm dialog
pub fn confirm_dialog_shortcuts() -> Vec<Shortcut> {
    ShortcutsBuilder::new()
        .add("Y", "Yes")
        .add("n", "No")
        .add("c", "Cancel")
        .build()
}

/// Shortcuts for the link mode (when selecting a target)
pub fn link_mode_shortcuts() -> Vec<Shortcut> {
    ShortcutsBuilder::new()
        .add("j/k", "Navigate")
        .add("l", "Confirm")
        .add("Enter", "Confirm")
        .add("Esc", "Cancel")
        .build()
}

/// Shortcuts for the note input modal (triage mode)
pub fn note_input_modal_shortcuts() -> Vec<Shortcut> {
    ShortcutsBuilder::new()
        .add("Enter", "Submit")
        .add("Esc", "Cancel")
        .build()
}

/// Shortcuts for the cancel confirm modal (triage mode)
pub fn cancel_confirm_modal_shortcuts() -> Vec<Shortcut> {
    ShortcutsBuilder::new()
        .add("c", "Confirm")
        .add("Esc", "Cancel")
        .build()
}

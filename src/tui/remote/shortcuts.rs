//! Shared shortcut computation logic for remote TUI
//!
//! This module provides common shortcut computation functions used by both
//! the view component (view.rs).

use crate::tui::components::footer::Shortcut;
use crate::tui::components::{
    ShortcutsBuilder, confirm_dialog_shortcuts, error_modal_shortcuts, filter_modal_shortcuts,
    help_modal_shortcuts, link_mode_shortcuts, search_shortcuts, sync_preview_shortcuts,
};

use super::state::ViewMode;

/// Modal visibility state for computing shortcuts
#[derive(Debug, Clone, Copy, Default)]
pub struct ModalVisibility {
    pub show_help_modal: bool,
    pub show_error_modal: bool,
    pub show_sync_preview: bool,
    pub show_confirm_dialog: bool,
    pub show_link_mode: bool,
    pub show_filter: bool,
    pub search_focused: bool,
}

impl ModalVisibility {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Compute keyboard shortcuts based on modal/view state priority
///
/// Checks modals in priority order (most specific first) and returns
/// the appropriate shortcuts. Falls back to normal mode shortcuts if
/// no modal is active.
///
/// This shared function ensures consistent shortcut behavior across the TUI.
pub fn compute_shortcuts(modals: &ModalVisibility, current_view: ViewMode) -> Vec<Shortcut> {
    // Check modal states in priority order
    if modals.show_help_modal {
        return help_modal_shortcuts();
    }

    if modals.show_error_modal {
        return error_modal_shortcuts();
    }

    if modals.show_sync_preview {
        return sync_preview_shortcuts();
    }

    if modals.show_confirm_dialog {
        return confirm_dialog_shortcuts();
    }

    if modals.show_link_mode {
        return link_mode_shortcuts();
    }

    if modals.show_filter {
        return filter_modal_shortcuts();
    }

    if modals.search_focused {
        return search_shortcuts();
    }

    // Normal mode shortcuts - vary by current view
    let base = ShortcutsBuilder::new()
        .with_quit()
        .add("Tab", "Switch View")
        .add("j/k", "Navigate")
        .add("Space", "Select")
        .add("/", "Search")
        .add("P", "Provider")
        .add("r", "Refresh")
        .add("f", "Filter");

    let view_specific = if current_view == ViewMode::Remote {
        base.add("a", "Adopt")
    } else {
        base.add("p", "Push").add("u", "Unlink")
    };

    view_specific
        .add("l", "Link")
        .add("s", "Sync")
        .add("?", "Help")
        .build()
}

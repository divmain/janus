//! State types for the remote TUI

use std::collections::HashSet;

use crate::remote::config::Platform;
use crate::remote::RemoteQuery;

/// Active view mode in the remote TUI
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    #[default]
    Local,
    Remote,
}

impl ViewMode {
    pub fn toggle(self) -> Self {
        match self {
            ViewMode::Local => ViewMode::Remote,
            ViewMode::Remote => ViewMode::Local,
        }
    }
}

/// Navigation state for a single view (local or remote)
/// Groups selected_index, scroll_offset, and selected_ids together
#[derive(Debug, Clone, Default)]
pub struct NavigationData {
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub selected_ids: HashSet<String>,
}

impl NavigationData {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear_selection(&mut self) {
        self.selected_ids.clear();
    }

    pub fn reset(&mut self) {
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.selected_ids.clear();
    }

    pub fn select_item(&mut self, index: usize) {
        self.selected_index = index;
        // Update scroll offset if needed to keep selection visible
        if index < self.scroll_offset {
            self.scroll_offset = index;
        }
    }
}

/// View display state - UI visibility and focus state
#[derive(Debug, Clone, Copy, Default)]
pub struct ViewDisplayData {
    pub active_view: ViewMode,
    pub remote_loading: bool,
    pub show_detail: bool,
    pub detail_pane_focused: bool,
    pub should_exit: bool,
}

impl ViewDisplayData {
    pub fn new() -> Self {
        Self {
            remote_loading: true,
            show_detail: true,
            ..Default::default()
        }
    }

    pub fn toggle_view(&mut self) {
        self.active_view = self.active_view.toggle();
    }

    pub fn set_view(&mut self, view: ViewMode) {
        self.active_view = view;
    }
}

/// Detail pane scroll state
#[derive(Debug, Clone, Copy, Default)]
pub struct DetailScrollData {
    pub local_offset: usize,
    pub remote_offset: usize,
}

impl DetailScrollData {
    pub fn scroll_up(&mut self, view: ViewMode, lines: usize) {
        match view {
            ViewMode::Local => {
                self.local_offset = self.local_offset.saturating_sub(lines);
            }
            ViewMode::Remote => {
                self.remote_offset = self.remote_offset.saturating_sub(lines);
            }
        }
    }

    pub fn scroll_down(&mut self, view: ViewMode, lines: usize) {
        match view {
            ViewMode::Local => {
                self.local_offset += lines;
            }
            ViewMode::Remote => {
                self.remote_offset += lines;
            }
        }
    }

    pub fn get_offset(&self, view: ViewMode) -> usize {
        match view {
            ViewMode::Local => self.local_offset,
            ViewMode::Remote => self.remote_offset,
        }
    }
}

/// Filter and provider configuration state
#[derive(Debug, Clone)]
pub struct FilterConfigData {
    pub active_filters: RemoteQuery,
    pub provider: Platform,
}

impl Default for FilterConfigData {
    fn default() -> Self {
        Self {
            active_filters: RemoteQuery::new(),
            provider: Platform::GitHub,
        }
    }
}

/// Search UI state
#[derive(Debug, Clone, Default)]
pub struct SearchUiData {
    pub query: String,
    pub focused: bool,
}

/// Modal visibility state
#[derive(Debug, Clone, Copy, Default)]
pub struct ModalVisibilityData {
    pub show_help: bool,
    pub help_scroll: usize,
    pub show_error: bool,
}

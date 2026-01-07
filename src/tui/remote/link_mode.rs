//! Link mode state for remote TUI

use super::state::ViewMode;

/// State for link operation flow
#[derive(Debug, Clone)]
pub struct LinkModeState {
    pub source_view: ViewMode,
    pub source_id: String,
    pub source_title: String,
}

impl LinkModeState {
    pub fn new(source_view: ViewMode, source_id: String, source_title: String) -> Self {
        Self {
            source_view,
            source_id,
            source_title,
        }
    }
}

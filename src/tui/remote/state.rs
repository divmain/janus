//! State types for the remote TUI

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

//! Link mode state for remote TUI

use crate::remote::RemoteIssue;

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

/// Data for a link operation to be executed asynchronously
#[derive(Debug, Clone)]
pub struct LinkSource {
    /// The local ticket ID to link
    pub ticket_id: String,
    /// The remote issue to link to
    pub remote_issue: RemoteIssue,
}

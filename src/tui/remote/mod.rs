//! Remote TUI module for managing local tickets and remote issues
//!
//! This module provides TUI functionality for browsing and managing the
//! relationship between local Janus tickets and remote issues (Linear/GitHub).

mod confirm_modal;
mod error_modal;
mod error_toast;
mod filter;
mod filter_modal;
mod handlers;
mod help_modal;
mod link_mode;
mod operations;
mod state;
mod sync_preview;
pub mod view;

pub use confirm_modal::{ConfirmDialog, ConfirmDialogState};
pub use error_modal::ErrorDetailModal;
pub use error_toast::{Toast, ToastLevel};
pub use filter::{
    FilteredLocalTicket, FilteredRemoteIssue, filter_local_tickets, filter_remote_issues,
};
pub use filter_modal::{FilterModal, FilterState};
pub use help_modal::HelpModal;
pub use link_mode::LinkModeState;
pub use state::ViewMode;
pub use sync_preview::{
    SyncChange, SyncChangeWithContext, SyncDecision, SyncDirection, SyncPreview, SyncPreviewState,
};
pub use view::{RemoteTui, RemoteTuiProps};

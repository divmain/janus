//! Handler context containing grouped state references
//!
//! This module organizes the TUI state into logical groups, making it easier
//! to understand which state each handler needs and simplifying testing.

use std::collections::HashSet;

use iocraft::prelude::{Handler, State};

use crate::remote::config::Platform;
use crate::remote::{RemoteIssue, RemoteQuery};
use crate::types::TicketMetadata;

use super::super::error_toast::Toast;
use super::super::filter_modal::FilterState;
use super::super::link_mode::LinkModeState;
use super::super::state::ViewMode;
use super::super::sync_preview::SyncPreviewState;

/// Navigation state for a single view (local or remote)
pub struct NavigationState<'a> {
    pub selected_index: &'a mut State<usize>,
    pub scroll_offset: &'a mut State<usize>,
    pub selected_ids: &'a mut State<HashSet<String>>,
}

/// Data and navigation state for both local and remote views
pub struct ViewData<'a> {
    pub local_tickets: &'a mut State<Vec<TicketMetadata>>,
    pub remote_issues: &'a mut State<Vec<RemoteIssue>>,
    pub local_nav: NavigationState<'a>,
    pub remote_nav: NavigationState<'a>,
    /// Computed count of items in local list (from filtered list)
    pub local_count: usize,
    /// Computed count of items in remote list (from filtered list)
    pub remote_count: usize,
    /// Height of the list area for scroll calculations
    pub list_height: usize,
}

/// Global view state (which view is active, exit flag, etc.)
pub struct ViewState<'a> {
    pub active_view: &'a mut State<ViewMode>,
    pub show_detail: &'a mut State<bool>,
    pub should_exit: &'a mut State<bool>,
}

/// Search functionality state
pub struct SearchState<'a> {
    pub query: &'a mut State<String>,
    pub focused: &'a mut State<bool>,
}

/// Modal and operation states
pub struct ModalState<'a> {
    pub toast: &'a mut State<Option<Toast>>,
    pub link_mode: &'a mut State<Option<LinkModeState>>,
    pub sync_preview: &'a mut State<Option<SyncPreviewState>>,
    pub show_help_modal: &'a mut State<bool>,
    pub show_error_modal: &'a mut State<bool>,
    pub last_error: &'a State<Option<(String, String)>>,
}

/// Filter and provider state
pub struct FilteringState<'a> {
    pub filter_modal: &'a mut State<Option<FilterState>>,
    pub active_filters: &'a mut State<RemoteQuery>,
    pub provider: &'a mut State<Platform>,
}

/// Remote data loading state
pub struct RemoteState<'a> {
    pub loading: &'a mut State<bool>,
}

/// Async operation handlers
pub struct AsyncHandlers<'a> {
    pub fetch_handler: &'a Handler<(Platform, RemoteQuery)>,
    pub push_handler: &'a Handler<(Vec<String>, Platform, RemoteQuery)>,
    pub sync_fetch_handler: &'a Handler<(Vec<String>, Platform)>,
    pub sync_apply_handler: &'a Handler<(SyncPreviewState, Platform, RemoteQuery)>,
}

/// Main context struct holding grouped state for event handlers
///
/// This struct organizes state into logical groups, making it easier to:
/// - Understand which state each handler needs
/// - Test handlers with only relevant state
/// - Reason about dependencies and side effects
pub struct HandlerContext<'a> {
    pub view_state: ViewState<'a>,
    pub view_data: ViewData<'a>,
    pub search: SearchState<'a>,
    pub modals: ModalState<'a>,
    pub filters: FilteringState<'a>,
    pub remote: RemoteState<'a>,
    pub handlers: AsyncHandlers<'a>,
}

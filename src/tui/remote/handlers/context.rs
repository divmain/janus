//! Handler context containing all mutable state references
//!
//! This struct provides a clean interface for handlers to access and modify
//! the TUI state without needing to pass dozens of individual parameters.

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

/// Context struct holding all mutable state for event handlers
pub struct HandlerContext<'a> {
    // View state
    pub active_view: &'a mut State<ViewMode>,
    pub show_detail: &'a mut State<bool>,
    pub should_exit: &'a mut State<bool>,

    // Data state
    pub local_tickets: &'a mut State<Vec<TicketMetadata>>,
    pub remote_issues: &'a mut State<Vec<RemoteIssue>>,

    // Selection state
    pub local_selected_index: &'a mut State<usize>,
    pub remote_selected_index: &'a mut State<usize>,
    pub local_scroll_offset: &'a mut State<usize>,
    pub remote_scroll_offset: &'a mut State<usize>,
    pub local_selected_ids: &'a mut State<HashSet<String>>,
    pub remote_selected_ids: &'a mut State<HashSet<String>>,

    // Computed counts (from filtered lists)
    pub local_count: usize,
    pub remote_count: usize,
    pub list_height: usize,

    // Loading state
    pub remote_loading: &'a mut State<bool>,

    // Modal/operation state
    pub toast: &'a mut State<Option<Toast>>,
    pub link_mode: &'a mut State<Option<LinkModeState>>,
    pub sync_preview: &'a mut State<Option<SyncPreviewState>>,
    pub show_help_modal: &'a mut State<bool>,
    pub show_error_modal: &'a mut State<bool>,
    pub last_error: &'a State<Option<(String, String)>>,

    // Search state
    pub search_query: &'a mut State<String>,
    pub search_focused: &'a mut State<bool>,

    // Provider state
    pub provider: &'a mut State<Platform>,

    // Filter state
    pub filter_state: &'a mut State<Option<FilterState>>,
    pub active_filters: &'a mut State<RemoteQuery>,

    // Async handlers
    pub fetch_handler: &'a Handler<(Platform, RemoteQuery)>,
    pub push_handler: &'a Handler<(Vec<String>, Platform, RemoteQuery)>,
    pub sync_fetch_handler: &'a Handler<(Vec<String>, Platform)>,
    pub sync_apply_handler: &'a Handler<(SyncPreviewState, Platform, RemoteQuery)>,
}

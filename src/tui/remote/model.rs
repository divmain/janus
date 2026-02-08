//! RemoteTui model types for testable state management
//!
//! This module separates state (RemoteState) from view (RemoteViewModel)
//! enabling comprehensive unit testing without the iocraft framework.

use std::collections::HashSet;

use iocraft::prelude::{KeyCode, KeyModifiers};

use crate::remote::{Platform, RemoteIssue, RemoteQuery};
use crate::tui::components::footer::Shortcut;
use crate::tui::components::toast::Toast;
use crate::types::TicketMetadata;

use super::filter::{
    FilteredLocalTicket, FilteredRemoteIssue, filter_local_tickets, filter_remote_issues,
};
use super::filter_modal::FilterState;
use super::link_mode::LinkModeState;
use super::shortcuts::{ModalVisibility, compute_shortcuts};
use super::state::ViewMode;
use super::sync_preview::SyncPreviewState;

// ============================================================================
// State Types
// ============================================================================

/// Raw state that changes during user interaction
#[derive(Debug, Clone)]
pub struct RemoteState {
    // Data
    /// All local tickets loaded from the repository
    pub local_tickets: Vec<TicketMetadata>,
    /// All remote issues fetched from the provider
    pub remote_issues: Vec<RemoteIssue>,

    // View state
    /// Currently active view (Local or Remote)
    pub active_view: ViewMode,
    /// Whether to show the detail panel
    pub show_detail: bool,

    // Navigation - Local list
    /// Index of the selected item in the local list
    pub local_selected_index: usize,
    /// Scroll offset for the local list
    pub local_scroll_offset: usize,
    /// Set of selected local ticket IDs (for multi-select)
    pub local_selected_ids: HashSet<String>,

    // Navigation - Remote list
    /// Index of the selected item in the remote list
    pub remote_selected_index: usize,
    /// Scroll offset for the remote list
    pub remote_scroll_offset: usize,
    /// Set of selected remote issue IDs (for multi-select)
    pub remote_selected_ids: HashSet<String>,

    // Search
    /// Current search query string
    pub search_query: String,
    /// Whether the search box is focused
    pub search_focused: bool,

    // Modals
    /// Optional toast notification to display
    pub toast: Option<Toast>,
    /// Link mode state (when linking local to remote)
    pub link_mode: Option<LinkModeState>,
    /// Sync preview state (when reviewing sync changes)
    pub sync_preview: Option<SyncPreviewState>,
    /// Filter modal state (when setting filters)
    pub filter_modal: Option<FilterState>,
    /// Whether the help modal is visible
    pub show_help_modal: bool,
    /// Whether the error modal is visible
    pub show_error_modal: bool,
    /// Last error (title, message) for error modal
    pub last_error: Option<(String, String)>,

    // Filters
    /// Active filters for the remote query
    pub active_filters: RemoteQuery,
    /// Current remote provider platform
    pub provider: Platform,

    // Loading/app state
    /// Whether data is currently being loaded
    pub is_loading: bool,
    /// Whether the application should exit
    pub should_exit: bool,
}

impl Default for RemoteState {
    fn default() -> Self {
        Self {
            local_tickets: Vec::new(),
            remote_issues: Vec::new(),
            active_view: ViewMode::default(),
            show_detail: false,
            local_selected_index: 0,
            local_scroll_offset: 0,
            local_selected_ids: HashSet::new(),
            remote_selected_index: 0,
            remote_scroll_offset: 0,
            remote_selected_ids: HashSet::new(),
            search_query: String::new(),
            search_focused: false,
            toast: None,
            link_mode: None,
            sync_preview: None,
            filter_modal: None,
            show_help_modal: false,
            show_error_modal: false,
            last_error: None,
            active_filters: RemoteQuery::new(),
            provider: Platform::GitHub,
            is_loading: false,
            should_exit: false,
        }
    }
}

// ============================================================================
// Action Types
// ============================================================================

/// All possible actions on the remote TUI
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteAction {
    // Navigation
    /// Move selection up one item
    MoveUp,
    /// Move selection down one item
    MoveDown,
    /// Move selection up and extend selection
    MoveUpExtendSelection,
    /// Move selection down and extend selection
    MoveDownExtendSelection,
    /// Jump to the first item
    GoToTop,
    /// Jump to the last item
    GoToBottom,
    /// Page down (half page)
    PageUp,
    /// Page up (half page)
    PageDown,

    // View
    /// Toggle between local and remote views
    ToggleView,
    /// Toggle the detail panel
    ToggleDetail,

    // Selection
    /// Toggle selection of current item
    ToggleSelection,
    /// Select all items in current view
    SelectAll,
    /// Clear all selections
    ClearSelection,

    // Search
    /// Focus the search box
    FocusSearch,
    /// Update the search query text
    UpdateSearch(String),
    /// Exit search mode, keeping the query
    ExitSearch,
    /// Clear search query and exit search mode
    ClearSearchAndExit,

    // Modals
    /// Show the help modal
    ShowHelp,
    /// Hide the help modal
    HideHelp,
    /// Show the filter modal
    ShowFilterModal,
    /// Hide the filter modal
    HideFilterModal,
    /// Show the error modal
    ShowErrorModal,
    /// Hide the error modal
    HideErrorModal,
    /// Dismiss the toast notification
    DismissToast,

    // Link mode
    /// Start link mode (linking local to remote)
    StartLinkMode,
    /// Cancel link mode
    CancelLinkMode,
    /// Confirm the link operation
    ConfirmLink,
    /// Select a link target by index
    SelectLinkTarget(usize),

    // Sync
    /// Start sync preview
    StartSync,
    /// Cancel sync operation
    CancelSync,
    /// Apply sync changes
    ApplySync,
    /// Toggle inclusion of a sync item by index
    ToggleSyncItem(usize),

    // Operations (async - handled externally)
    /// Fetch/refresh data from remote
    Fetch,
    /// Push changes to remote
    Push,
    /// Edit the currently selected local ticket
    EditLocal,
    /// Open the currently selected remote issue in browser
    OpenRemote,
    /// Unlink the currently selected ticket from its remote
    Unlink,

    // App
    /// Quit the application
    Quit,
}

// ============================================================================
// View Model Types
// ============================================================================

/// Computed view model for rendering the entire remote TUI
#[derive(Debug, Clone)]
pub struct RemoteViewModel {
    /// Header view model
    pub header: HeaderViewModel,
    /// Local list view model
    pub local_list: LocalListViewModel,
    /// Remote list view model
    pub remote_list: RemoteListViewModel,
    /// Detail view model
    pub detail: DetailViewModel,
    /// Search view model
    pub search: SearchViewModel,
    /// Modal view model
    pub modal: ModalViewModel,
    /// Toast notification to display
    pub toast: Option<Toast>,
    /// Keyboard shortcuts to display in footer
    pub shortcuts: Vec<Shortcut>,
    /// Whether the app is in loading state
    pub is_loading: bool,
}

/// View model for the header bar
#[derive(Debug, Clone)]
pub struct HeaderViewModel {
    /// Current platform name
    pub platform_name: String,
    /// Count of local tickets
    pub local_count: usize,
    /// Count of remote issues
    pub remote_count: usize,
    /// Whether filters are active
    pub has_active_filters: bool,
}

/// View model for the local ticket list
#[derive(Debug, Clone)]
pub struct LocalListViewModel {
    /// Filtered local tickets to display
    pub tickets: Vec<FilteredLocalTicket>,
    /// Index of the selected ticket
    pub selected_index: usize,
    /// Scroll offset for virtual scrolling
    pub scroll_offset: usize,
    /// Whether this list is currently focused
    pub is_focused: bool,
    /// Set of selected ticket IDs
    pub selected_ids: HashSet<String>,
    /// Number of visible items
    pub visible_count: usize,
}

/// View model for the remote issue list
#[derive(Debug, Clone)]
pub struct RemoteListViewModel {
    /// Filtered remote issues to display
    pub issues: Vec<FilteredRemoteIssue>,
    /// Index of the selected issue
    pub selected_index: usize,
    /// Scroll offset for virtual scrolling
    pub scroll_offset: usize,
    /// Whether this list is currently focused
    pub is_focused: bool,
    /// Set of selected issue IDs
    pub selected_ids: HashSet<String>,
    /// Number of visible items
    pub visible_count: usize,
}

/// View model for the detail panel
#[derive(Debug, Clone)]
pub struct DetailViewModel {
    /// Selected local ticket (if in local view)
    pub local_ticket: Option<TicketMetadata>,
    /// Selected remote issue (if in remote view)
    pub remote_issue: Option<RemoteIssue>,
    /// Whether the detail panel is visible
    pub is_visible: bool,
    /// Current view mode
    pub view_mode: ViewMode,
}

/// View model for the search box
#[derive(Debug, Clone)]
pub struct SearchViewModel {
    /// Current search query
    pub query: String,
    /// Whether the search box is focused
    pub is_focused: bool,
    /// Number of matching results in current view
    pub result_count: usize,
}

/// View model for modals
#[derive(Debug, Clone)]
pub struct ModalViewModel {
    /// Whether help modal is visible
    pub show_help: bool,
    /// Whether filter modal is visible
    pub show_filter: bool,
    /// Filter state (if filter modal is visible)
    pub filter_state: Option<FilterState>,
    /// Whether error modal is visible
    pub show_error: bool,
    /// Error details (title, message)
    pub error: Option<(String, String)>,
    /// Whether link mode is active
    pub link_mode_active: bool,
    /// Link mode state (if active)
    pub link_mode_state: Option<LinkModeState>,
    /// Whether sync preview is active
    pub sync_preview_active: bool,
    /// Sync preview state (if active)
    pub sync_preview_state: Option<SyncPreviewState>,
}

// ============================================================================
// Pure Functions
// ============================================================================

/// Pure function: compute view model from state
///
/// This function takes the raw remote state and produces a fully computed
/// view model that can be directly used for rendering.
pub fn compute_remote_view_model(state: &RemoteState, list_height: usize) -> RemoteViewModel {
    // Filter local tickets and remote issues by search query
    let filtered_local = filter_local_tickets(&state.local_tickets, &state.search_query);
    let filtered_remote = filter_remote_issues(&state.remote_issues, &state.search_query);

    let local_count = filtered_local.len();
    let remote_count = filtered_remote.len();

    // Get selected items for detail view
    let selected_local_ticket = filtered_local
        .get(state.local_selected_index)
        .map(|ft| ft.ticket.clone());

    let selected_remote_issue = filtered_remote
        .get(state.remote_selected_index)
        .map(|fi| fi.issue.clone());

    // Compute shortcuts based on current state
    let shortcuts = compute_shortcuts(
        &ModalVisibility {
            show_help_modal: state.show_help_modal,
            show_error_modal: state.show_error_modal,
            show_sync_preview: state.sync_preview.is_some(),
            show_confirm_dialog: false, // confirm_dialog is not tracked in RemoteState
            show_link_mode: state.link_mode.is_some(),
            show_filter: state.filter_modal.is_some(),
            search_focused: state.search_focused,
        },
        state.active_view,
    );

    // Check if any filters are active
    let has_active_filters = state.active_filters.status.is_some()
        || state.active_filters.assignee.is_some()
        || state.active_filters.labels.is_some();

    // Determine result count for search based on active view
    let result_count = match state.active_view {
        ViewMode::Local => local_count,
        ViewMode::Remote => remote_count,
    };

    RemoteViewModel {
        header: HeaderViewModel {
            platform_name: state.provider.to_string(),
            local_count: state.local_tickets.len(),
            remote_count: state.remote_issues.len(),
            has_active_filters,
        },
        local_list: LocalListViewModel {
            tickets: filtered_local,
            selected_index: state.local_selected_index,
            scroll_offset: state.local_scroll_offset,
            is_focused: state.active_view == ViewMode::Local
                && !state.search_focused
                && state.link_mode.is_none()
                && state.sync_preview.is_none(),
            selected_ids: state.local_selected_ids.clone(),
            visible_count: list_height.min(local_count),
        },
        remote_list: RemoteListViewModel {
            issues: filtered_remote,
            selected_index: state.remote_selected_index,
            scroll_offset: state.remote_scroll_offset,
            is_focused: state.active_view == ViewMode::Remote
                && !state.search_focused
                && state.link_mode.is_none()
                && state.sync_preview.is_none(),
            selected_ids: state.remote_selected_ids.clone(),
            visible_count: list_height.min(remote_count),
        },
        detail: DetailViewModel {
            local_ticket: selected_local_ticket,
            remote_issue: selected_remote_issue,
            is_visible: state.show_detail,
            view_mode: state.active_view,
        },
        search: SearchViewModel {
            query: state.search_query.clone(),
            is_focused: state.search_focused,
            result_count,
        },
        modal: ModalViewModel {
            show_help: state.show_help_modal,
            show_filter: state.filter_modal.is_some(),
            filter_state: state.filter_modal.clone(),
            show_error: state.show_error_modal,
            error: state.last_error.clone(),
            link_mode_active: state.link_mode.is_some(),
            link_mode_state: state.link_mode.clone(),
            sync_preview_active: state.sync_preview.is_some(),
            sync_preview_state: state.sync_preview.clone(),
        },
        toast: state.toast.clone(),
        shortcuts,
        is_loading: state.is_loading,
    }
}

/// Pure function: apply action to state (reducer pattern)
///
/// This function takes the current state and an action, returning the new state.
/// It contains only pure state transitions - no side effects like network I/O.
///
/// Note: Some actions (like Fetch, Push, EditLocal, OpenRemote, Unlink) require
/// async I/O and are handled separately by the component.
pub fn reduce_remote_state(
    mut state: RemoteState,
    action: RemoteAction,
    list_height: usize,
) -> RemoteState {
    // Get filtered counts for navigation bounds
    let filtered_local = filter_local_tickets(&state.local_tickets, &state.search_query);
    let filtered_remote = filter_remote_issues(&state.remote_issues, &state.search_query);
    let local_count = filtered_local.len();
    let remote_count = filtered_remote.len();

    match action {
        // Navigation
        RemoteAction::MoveUp => match state.active_view {
            ViewMode::Local => {
                state.local_selected_index = state.local_selected_index.saturating_sub(1);
                state.local_scroll_offset = adjust_scroll(
                    state.local_scroll_offset,
                    state.local_selected_index,
                    list_height,
                );
            }
            ViewMode::Remote => {
                state.remote_selected_index = state.remote_selected_index.saturating_sub(1);
                state.remote_scroll_offset = adjust_scroll(
                    state.remote_scroll_offset,
                    state.remote_selected_index,
                    list_height,
                );
            }
        },
        RemoteAction::MoveDown => match state.active_view {
            ViewMode::Local => {
                if local_count > 0 {
                    state.local_selected_index =
                        (state.local_selected_index + 1).min(local_count - 1);
                    state.local_scroll_offset = adjust_scroll(
                        state.local_scroll_offset,
                        state.local_selected_index,
                        list_height,
                    );
                }
            }
            ViewMode::Remote => {
                if remote_count > 0 {
                    state.remote_selected_index =
                        (state.remote_selected_index + 1).min(remote_count - 1);
                    state.remote_scroll_offset = adjust_scroll(
                        state.remote_scroll_offset,
                        state.remote_selected_index,
                        list_height,
                    );
                }
            }
        },
        RemoteAction::MoveUpExtendSelection => {
            // Move up and add current to selection
            match state.active_view {
                ViewMode::Local => {
                    if let Some(ticket) = filtered_local.get(state.local_selected_index)
                        && let Some(id) = &ticket.ticket.id
                    {
                        state.local_selected_ids.insert(id.clone());
                    }
                    state.local_selected_index = state.local_selected_index.saturating_sub(1);
                    if let Some(ticket) = filtered_local.get(state.local_selected_index)
                        && let Some(id) = &ticket.ticket.id
                    {
                        state.local_selected_ids.insert(id.clone());
                    }
                    state.local_scroll_offset = adjust_scroll(
                        state.local_scroll_offset,
                        state.local_selected_index,
                        list_height,
                    );
                }
                ViewMode::Remote => {
                    if let Some(issue) = filtered_remote.get(state.remote_selected_index) {
                        state.remote_selected_ids.insert(issue.issue.id.clone());
                    }
                    state.remote_selected_index = state.remote_selected_index.saturating_sub(1);
                    if let Some(issue) = filtered_remote.get(state.remote_selected_index) {
                        state.remote_selected_ids.insert(issue.issue.id.clone());
                    }
                    state.remote_scroll_offset = adjust_scroll(
                        state.remote_scroll_offset,
                        state.remote_selected_index,
                        list_height,
                    );
                }
            }
        }
        RemoteAction::MoveDownExtendSelection => match state.active_view {
            ViewMode::Local => {
                if local_count > 0 {
                    if let Some(ticket) = filtered_local.get(state.local_selected_index)
                        && let Some(id) = &ticket.ticket.id
                    {
                        state.local_selected_ids.insert(id.clone());
                    }
                    state.local_selected_index =
                        (state.local_selected_index + 1).min(local_count - 1);
                    if let Some(ticket) = filtered_local.get(state.local_selected_index)
                        && let Some(id) = &ticket.ticket.id
                    {
                        state.local_selected_ids.insert(id.clone());
                    }
                    state.local_scroll_offset = adjust_scroll(
                        state.local_scroll_offset,
                        state.local_selected_index,
                        list_height,
                    );
                }
            }
            ViewMode::Remote => {
                if remote_count > 0 {
                    if let Some(issue) = filtered_remote.get(state.remote_selected_index) {
                        state.remote_selected_ids.insert(issue.issue.id.clone());
                    }
                    state.remote_selected_index =
                        (state.remote_selected_index + 1).min(remote_count - 1);
                    if let Some(issue) = filtered_remote.get(state.remote_selected_index) {
                        state.remote_selected_ids.insert(issue.issue.id.clone());
                    }
                    state.remote_scroll_offset = adjust_scroll(
                        state.remote_scroll_offset,
                        state.remote_selected_index,
                        list_height,
                    );
                }
            }
        },
        RemoteAction::GoToTop => match state.active_view {
            ViewMode::Local => {
                state.local_selected_index = 0;
                state.local_scroll_offset = 0;
            }
            ViewMode::Remote => {
                state.remote_selected_index = 0;
                state.remote_scroll_offset = 0;
            }
        },
        RemoteAction::GoToBottom => match state.active_view {
            ViewMode::Local => {
                if local_count > 0 {
                    state.local_selected_index = local_count - 1;
                    state.local_scroll_offset = adjust_scroll(
                        state.local_scroll_offset,
                        state.local_selected_index,
                        list_height,
                    );
                }
            }
            ViewMode::Remote => {
                if remote_count > 0 {
                    state.remote_selected_index = remote_count - 1;
                    state.remote_scroll_offset = adjust_scroll(
                        state.remote_scroll_offset,
                        state.remote_selected_index,
                        list_height,
                    );
                }
            }
        },
        RemoteAction::PageUp => {
            let jump = list_height / 2;
            match state.active_view {
                ViewMode::Local => {
                    state.local_selected_index = state.local_selected_index.saturating_sub(jump);
                    state.local_scroll_offset = adjust_scroll(
                        state.local_scroll_offset,
                        state.local_selected_index,
                        list_height,
                    );
                }
                ViewMode::Remote => {
                    state.remote_selected_index = state.remote_selected_index.saturating_sub(jump);
                    state.remote_scroll_offset = adjust_scroll(
                        state.remote_scroll_offset,
                        state.remote_selected_index,
                        list_height,
                    );
                }
            }
        }
        RemoteAction::PageDown => {
            let jump = list_height / 2;
            match state.active_view {
                ViewMode::Local => {
                    if local_count > 0 {
                        state.local_selected_index =
                            (state.local_selected_index + jump).min(local_count - 1);
                        state.local_scroll_offset = adjust_scroll(
                            state.local_scroll_offset,
                            state.local_selected_index,
                            list_height,
                        );
                    }
                }
                ViewMode::Remote => {
                    if remote_count > 0 {
                        state.remote_selected_index =
                            (state.remote_selected_index + jump).min(remote_count - 1);
                        state.remote_scroll_offset = adjust_scroll(
                            state.remote_scroll_offset,
                            state.remote_selected_index,
                            list_height,
                        );
                    }
                }
            }
        }

        // View
        RemoteAction::ToggleView => {
            state.active_view = state.active_view.toggle();
        }
        RemoteAction::ToggleDetail => {
            state.show_detail = !state.show_detail;
        }

        // Selection
        RemoteAction::ToggleSelection => match state.active_view {
            ViewMode::Local => {
                if let Some(ticket) = filtered_local.get(state.local_selected_index)
                    && let Some(id) = &ticket.ticket.id
                {
                    if state.local_selected_ids.contains(id) {
                        state.local_selected_ids.remove(id);
                    } else {
                        state.local_selected_ids.insert(id.clone());
                    }
                }
            }
            ViewMode::Remote => {
                if let Some(issue) = filtered_remote.get(state.remote_selected_index) {
                    let id = &issue.issue.id;
                    if state.remote_selected_ids.contains(id) {
                        state.remote_selected_ids.remove(id);
                    } else {
                        state.remote_selected_ids.insert(id.clone());
                    }
                }
            }
        },
        RemoteAction::SelectAll => match state.active_view {
            ViewMode::Local => {
                for ticket in &filtered_local {
                    if let Some(id) = &ticket.ticket.id {
                        state.local_selected_ids.insert(id.clone());
                    }
                }
            }
            ViewMode::Remote => {
                for issue in &filtered_remote {
                    state.remote_selected_ids.insert(issue.issue.id.clone());
                }
            }
        },
        RemoteAction::ClearSelection => match state.active_view {
            ViewMode::Local => {
                state.local_selected_ids.clear();
            }
            ViewMode::Remote => {
                state.remote_selected_ids.clear();
            }
        },

        // Search
        RemoteAction::FocusSearch => {
            state.search_focused = true;
        }
        RemoteAction::UpdateSearch(query) => {
            state.search_query = query;
            // Reset selection when search changes
            state.local_selected_index = 0;
            state.local_scroll_offset = 0;
            state.remote_selected_index = 0;
            state.remote_scroll_offset = 0;
        }
        RemoteAction::ExitSearch => {
            state.search_focused = false;
        }
        RemoteAction::ClearSearchAndExit => {
            state.search_query = String::new();
            state.search_focused = false;
            // Reset selection
            state.local_selected_index = 0;
            state.local_scroll_offset = 0;
            state.remote_selected_index = 0;
            state.remote_scroll_offset = 0;
        }

        // Modals
        RemoteAction::ShowHelp => {
            state.show_help_modal = true;
        }
        RemoteAction::HideHelp => {
            state.show_help_modal = false;
        }
        RemoteAction::ShowFilterModal => {
            state.filter_modal = Some(FilterState::from_query(&state.active_filters));
        }
        RemoteAction::HideFilterModal => {
            state.filter_modal = None;
        }
        RemoteAction::ShowErrorModal => {
            state.show_error_modal = true;
        }
        RemoteAction::HideErrorModal => {
            state.show_error_modal = false;
        }
        RemoteAction::DismissToast => {
            state.toast = None;
        }

        // Link mode
        RemoteAction::StartLinkMode => {
            // Start link mode from current selection
            if state.active_view == ViewMode::Local
                && let Some(ticket) = filtered_local.get(state.local_selected_index)
                && let Some(id) = &ticket.ticket.id
            {
                state.link_mode = Some(LinkModeState::new(
                    ViewMode::Local,
                    id.clone(),
                    ticket
                        .ticket
                        .title
                        .clone()
                        .unwrap_or_else(|| "Untitled".to_string()),
                ));
                // Switch to remote view for selecting target
                state.active_view = ViewMode::Remote;
            }
        }
        RemoteAction::CancelLinkMode => {
            if let Some(link_state) = &state.link_mode {
                // Restore original view
                state.active_view = link_state.source_view;
            }
            state.link_mode = None;
        }
        RemoteAction::ConfirmLink | RemoteAction::SelectLinkTarget(_) => {
            // These require async I/O, handled externally
            // ConfirmLink uses current selection
            // SelectLinkTarget is just setting the index before confirm
        }

        // Sync
        RemoteAction::StartSync => {
            // Sync preview initialization is handled externally (needs async fetch)
        }
        RemoteAction::CancelSync => {
            state.sync_preview = None;
        }
        RemoteAction::ApplySync => {
            // Actual sync application is handled externally
        }
        RemoteAction::ToggleSyncItem(index) => {
            if let Some(ref mut sync_state) = state.sync_preview
                && index < sync_state.changes.len()
            {
                let change = &mut sync_state.changes[index];
                change.decision = match change.decision {
                    Some(super::sync_preview::SyncDecision::Accept) => {
                        Some(super::sync_preview::SyncDecision::Skip)
                    }
                    Some(super::sync_preview::SyncDecision::Skip) => {
                        Some(super::sync_preview::SyncDecision::Accept)
                    }
                    None => Some(super::sync_preview::SyncDecision::Accept),
                };
            }
        }

        // Operations (async - handled externally)
        RemoteAction::Fetch
        | RemoteAction::Push
        | RemoteAction::EditLocal
        | RemoteAction::OpenRemote
        | RemoteAction::Unlink => {
            // These require async I/O or system context, handled externally
        }

        // App
        RemoteAction::Quit => {
            state.should_exit = true;
        }
    }

    state
}

/// Adjust scroll offset to keep selected item visible
///
/// Returns the new scroll offset that ensures the selected index is visible
/// within the list height.
pub fn adjust_scroll(scroll_offset: usize, selected_index: usize, list_height: usize) -> usize {
    if list_height == 0 {
        return 0;
    }

    // If selected is above the visible area, scroll up
    if selected_index < scroll_offset {
        return selected_index;
    }

    // If selected is below the visible area, scroll down
    if selected_index >= scroll_offset + list_height {
        return selected_index.saturating_sub(list_height - 1);
    }

    // Selected is within visible area, keep scroll as is
    scroll_offset
}

/// Convert a key event to a RemoteAction (pure function)
///
/// This function maps keyboard events to abstract actions, enabling
/// unit testing of the key mapping logic without any iocraft dependencies.
///
/// Takes the full state to check modal states for context-sensitive key handling.
///
/// Returns `None` if the key doesn't map to any action.
pub fn key_to_action(
    code: KeyCode,
    modifiers: KeyModifiers,
    state: &RemoteState,
) -> Option<RemoteAction> {
    // Check modal states first (they capture all input)

    // Help modal - Esc or '?' closes it
    if state.show_help_modal {
        return match code {
            KeyCode::Esc | KeyCode::Char('?') => Some(RemoteAction::HideHelp),
            _ => None,
        };
    }

    // Error modal - Esc closes it
    if state.show_error_modal {
        return match code {
            KeyCode::Esc => Some(RemoteAction::HideErrorModal),
            _ => None,
        };
    }

    // Sync preview mode
    if state.sync_preview.is_some() {
        return match code {
            KeyCode::Esc | KeyCode::Char('c') => Some(RemoteAction::CancelSync),
            KeyCode::Enter | KeyCode::Char('y') => Some(RemoteAction::ApplySync),
            KeyCode::Char(' ') | KeyCode::Char('n') => {
                // Toggle current item - need to get current index from sync preview
                state
                    .sync_preview
                    .as_ref()
                    .map(|sync_state| RemoteAction::ToggleSyncItem(sync_state.current_change_index))
            }
            KeyCode::Char('j') | KeyCode::Down => Some(RemoteAction::MoveDown),
            KeyCode::Char('k') | KeyCode::Up => Some(RemoteAction::MoveUp),
            _ => None,
        };
    }

    // Link mode
    if state.link_mode.is_some() {
        return match code {
            KeyCode::Esc => Some(RemoteAction::CancelLinkMode),
            KeyCode::Enter => Some(RemoteAction::ConfirmLink),
            KeyCode::Char('j') | KeyCode::Down => Some(RemoteAction::MoveDown),
            KeyCode::Char('k') | KeyCode::Up => Some(RemoteAction::MoveUp),
            _ => None,
        };
    }

    // Filter modal
    if state.filter_modal.is_some() {
        return match code {
            KeyCode::Esc => Some(RemoteAction::HideFilterModal),
            // Other filter modal keys are handled by the modal component
            _ => None,
        };
    }

    // Search mode
    if state.search_focused {
        return search_key_to_action(code, modifiers);
    }

    // Normal mode
    normal_key_to_action(code, modifiers)
}

/// Convert a key event in search mode to a RemoteAction
fn search_key_to_action(code: KeyCode, modifiers: KeyModifiers) -> Option<RemoteAction> {
    match (code, modifiers) {
        // Escape clears and exits
        (KeyCode::Esc, _) => Some(RemoteAction::ClearSearchAndExit),
        // Enter exits keeping query
        (KeyCode::Enter, _) => Some(RemoteAction::ExitSearch),
        // Ctrl+Q quits
        (KeyCode::Char('q'), m) if m.contains(KeyModifiers::CONTROL) => Some(RemoteAction::Quit),
        // Other characters are handled by the search box component
        _ => None,
    }
}

/// Convert a key event in normal mode to a RemoteAction
fn normal_key_to_action(code: KeyCode, modifiers: KeyModifiers) -> Option<RemoteAction> {
    // Handle shift+j/k for extend selection first
    if modifiers.contains(KeyModifiers::SHIFT) {
        return match code {
            KeyCode::Char('J') | KeyCode::Char('j') => Some(RemoteAction::MoveDownExtendSelection),
            KeyCode::Char('K') | KeyCode::Char('k') => Some(RemoteAction::MoveUpExtendSelection),
            KeyCode::Char('G') | KeyCode::Char('g') => Some(RemoteAction::GoToBottom),
            KeyCode::Char('P') | KeyCode::Char('p') => Some(RemoteAction::Push),
            _ => None,
        };
    }

    match (code, modifiers) {
        // Navigation
        (KeyCode::Char('j') | KeyCode::Down, KeyModifiers::NONE) => Some(RemoteAction::MoveDown),
        (KeyCode::Char('k') | KeyCode::Up, KeyModifiers::NONE) => Some(RemoteAction::MoveUp),
        (KeyCode::Char('g'), KeyModifiers::NONE) => Some(RemoteAction::GoToTop),
        (KeyCode::Char('G'), KeyModifiers::NONE) => Some(RemoteAction::GoToBottom),
        (KeyCode::PageUp, _) => Some(RemoteAction::PageUp),
        (KeyCode::PageDown, _) => Some(RemoteAction::PageDown),

        // View
        (KeyCode::Tab, KeyModifiers::NONE) => Some(RemoteAction::ToggleView),
        (KeyCode::Char('d'), KeyModifiers::NONE) => Some(RemoteAction::ToggleDetail),

        // Selection
        (KeyCode::Char(' '), KeyModifiers::NONE) => Some(RemoteAction::ToggleSelection),

        // Search
        (KeyCode::Char('/'), KeyModifiers::NONE) => Some(RemoteAction::FocusSearch),

        // Operations
        (KeyCode::Char('r'), KeyModifiers::NONE) => Some(RemoteAction::Fetch),
        (KeyCode::Char('s'), KeyModifiers::NONE) => Some(RemoteAction::StartSync),
        (KeyCode::Char('l'), KeyModifiers::NONE) => Some(RemoteAction::StartLinkMode),
        (KeyCode::Char('u'), KeyModifiers::NONE) => Some(RemoteAction::Unlink),
        (KeyCode::Char('e') | KeyCode::Enter, KeyModifiers::NONE) => Some(RemoteAction::EditLocal),
        (KeyCode::Char('o'), KeyModifiers::NONE) => Some(RemoteAction::OpenRemote),

        // Modals
        (KeyCode::Char('f'), KeyModifiers::NONE) => Some(RemoteAction::ShowFilterModal),
        (KeyCode::Char('?'), KeyModifiers::NONE) => Some(RemoteAction::ShowHelp),

        // App
        (KeyCode::Char('q') | KeyCode::Esc, KeyModifiers::NONE) => Some(RemoteAction::Quit),

        _ => None,
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::RemoteStatus;
    use crate::types::{TicketPriority, TicketStatus, TicketType};

    fn make_ticket(id: &str, title: &str, status: TicketStatus) -> TicketMetadata {
        TicketMetadata {
            id: Some(id.to_string()),
            title: Some(title.to_string()),
            status: Some(status),
            priority: Some(TicketPriority::P2),
            ticket_type: Some(TicketType::Task),
            ..Default::default()
        }
    }

    fn make_remote_issue(id: &str, title: &str) -> RemoteIssue {
        RemoteIssue {
            id: id.to_string(),
            title: title.to_string(),
            body: String::new(),
            status: RemoteStatus::Open,
            priority: None,
            assignee: None,
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            url: format!("https://example.com/issues/{id}"),
            labels: vec![],
            team: None,
            project: None,
            milestone: None,
            due_date: None,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            creator: None,
        }
    }

    fn default_state() -> RemoteState {
        RemoteState::default()
    }

    fn state_with_data() -> RemoteState {
        RemoteState {
            local_tickets: vec![
                make_ticket("j-1", "Task 1", TicketStatus::New),
                make_ticket("j-2", "Task 2", TicketStatus::InProgress),
                make_ticket("j-3", "Task 3", TicketStatus::Complete),
            ],
            remote_issues: vec![
                make_remote_issue("GH-1", "Issue 1"),
                make_remote_issue("GH-2", "Issue 2"),
                make_remote_issue("GH-3", "Issue 3"),
            ],
            ..default_state()
        }
    }

    // ========================================================================
    // Navigation Tests
    // ========================================================================

    #[test]
    fn test_reduce_move_down_local() {
        let state = state_with_data();
        let new_state = reduce_remote_state(state, RemoteAction::MoveDown, 20);
        assert_eq!(new_state.local_selected_index, 1);
    }

    #[test]
    fn test_reduce_move_down_remote() {
        let mut state = state_with_data();
        state.active_view = ViewMode::Remote;
        let new_state = reduce_remote_state(state, RemoteAction::MoveDown, 20);
        assert_eq!(new_state.remote_selected_index, 1);
    }

    #[test]
    fn test_reduce_move_up_at_top() {
        let state = state_with_data();
        let new_state = reduce_remote_state(state, RemoteAction::MoveUp, 20);
        assert_eq!(new_state.local_selected_index, 0);
    }

    #[test]
    fn test_reduce_move_down_at_bottom() {
        let mut state = state_with_data();
        state.local_selected_index = 2;
        let new_state = reduce_remote_state(state, RemoteAction::MoveDown, 20);
        assert_eq!(new_state.local_selected_index, 2);
    }

    #[test]
    fn test_reduce_go_to_top() {
        let mut state = state_with_data();
        state.local_selected_index = 2;
        state.local_scroll_offset = 1;
        let new_state = reduce_remote_state(state, RemoteAction::GoToTop, 20);
        assert_eq!(new_state.local_selected_index, 0);
        assert_eq!(new_state.local_scroll_offset, 0);
    }

    #[test]
    fn test_reduce_go_to_bottom() {
        let state = state_with_data();
        let new_state = reduce_remote_state(state, RemoteAction::GoToBottom, 20);
        assert_eq!(new_state.local_selected_index, 2);
    }

    #[test]
    fn test_reduce_page_down() {
        let mut state = state_with_data();
        state.local_tickets = (0..20)
            .map(|i| make_ticket(&format!("j-{i}"), &format!("Task {i}"), TicketStatus::New))
            .collect();
        let new_state = reduce_remote_state(state, RemoteAction::PageDown, 10);
        assert_eq!(new_state.local_selected_index, 5); // Half page
    }

    #[test]
    fn test_reduce_page_up() {
        let mut state = state_with_data();
        state.local_tickets = (0..20)
            .map(|i| make_ticket(&format!("j-{i}"), &format!("Task {i}"), TicketStatus::New))
            .collect();
        state.local_selected_index = 10;
        let new_state = reduce_remote_state(state, RemoteAction::PageUp, 10);
        assert_eq!(new_state.local_selected_index, 5);
    }

    // ========================================================================
    // View Toggle Tests
    // ========================================================================

    #[test]
    fn test_reduce_toggle_view() {
        let state = state_with_data();
        assert_eq!(state.active_view, ViewMode::Local);
        let new_state = reduce_remote_state(state, RemoteAction::ToggleView, 20);
        assert_eq!(new_state.active_view, ViewMode::Remote);
    }

    #[test]
    fn test_reduce_toggle_detail() {
        let state = state_with_data();
        assert!(!state.show_detail);
        let new_state = reduce_remote_state(state, RemoteAction::ToggleDetail, 20);
        assert!(new_state.show_detail);
    }

    // ========================================================================
    // Selection Tests
    // ========================================================================

    #[test]
    fn test_reduce_toggle_selection() {
        let state = state_with_data();
        let new_state = reduce_remote_state(state, RemoteAction::ToggleSelection, 20);
        assert!(new_state.local_selected_ids.contains("j-1"));
    }

    #[test]
    fn test_reduce_toggle_selection_off() {
        let mut state = state_with_data();
        state.local_selected_ids.insert("j-1".to_string());
        let new_state = reduce_remote_state(state, RemoteAction::ToggleSelection, 20);
        assert!(!new_state.local_selected_ids.contains("j-1"));
    }

    #[test]
    fn test_reduce_select_all() {
        let state = state_with_data();
        let new_state = reduce_remote_state(state, RemoteAction::SelectAll, 20);
        assert_eq!(new_state.local_selected_ids.len(), 3);
    }

    #[test]
    fn test_reduce_clear_selection() {
        let mut state = state_with_data();
        state.local_selected_ids.insert("j-1".to_string());
        state.local_selected_ids.insert("j-2".to_string());
        let new_state = reduce_remote_state(state, RemoteAction::ClearSelection, 20);
        assert!(new_state.local_selected_ids.is_empty());
    }

    #[test]
    fn test_reduce_move_down_extend_selection() {
        let state = state_with_data();
        let new_state = reduce_remote_state(state, RemoteAction::MoveDownExtendSelection, 20);
        assert!(new_state.local_selected_ids.contains("j-1"));
        assert!(new_state.local_selected_ids.contains("j-2"));
        assert_eq!(new_state.local_selected_index, 1);
    }

    // ========================================================================
    // Search Tests
    // ========================================================================

    #[test]
    fn test_reduce_focus_search() {
        let state = state_with_data();
        let new_state = reduce_remote_state(state, RemoteAction::FocusSearch, 20);
        assert!(new_state.search_focused);
    }

    #[test]
    fn test_reduce_update_search() {
        let state = state_with_data();
        let new_state =
            reduce_remote_state(state, RemoteAction::UpdateSearch("test".to_string()), 20);
        assert_eq!(new_state.search_query, "test");
    }

    #[test]
    fn test_reduce_update_search_resets_selection() {
        let mut state = state_with_data();
        state.local_selected_index = 2;
        let new_state =
            reduce_remote_state(state, RemoteAction::UpdateSearch("test".to_string()), 20);
        assert_eq!(new_state.local_selected_index, 0);
    }

    #[test]
    fn test_reduce_exit_search() {
        let mut state = state_with_data();
        state.search_focused = true;
        state.search_query = "test".to_string();
        let new_state = reduce_remote_state(state, RemoteAction::ExitSearch, 20);
        assert!(!new_state.search_focused);
        assert_eq!(new_state.search_query, "test"); // Query preserved
    }

    #[test]
    fn test_reduce_clear_search_and_exit() {
        let mut state = state_with_data();
        state.search_focused = true;
        state.search_query = "test".to_string();
        let new_state = reduce_remote_state(state, RemoteAction::ClearSearchAndExit, 20);
        assert!(!new_state.search_focused);
        assert!(new_state.search_query.is_empty());
    }

    // ========================================================================
    // Modal Tests
    // ========================================================================

    #[test]
    fn test_reduce_show_help() {
        let state = state_with_data();
        let new_state = reduce_remote_state(state, RemoteAction::ShowHelp, 20);
        assert!(new_state.show_help_modal);
    }

    #[test]
    fn test_reduce_hide_help() {
        let mut state = state_with_data();
        state.show_help_modal = true;
        let new_state = reduce_remote_state(state, RemoteAction::HideHelp, 20);
        assert!(!new_state.show_help_modal);
    }

    #[test]
    fn test_reduce_show_filter_modal() {
        let state = state_with_data();
        let new_state = reduce_remote_state(state, RemoteAction::ShowFilterModal, 20);
        assert!(new_state.filter_modal.is_some());
    }

    #[test]
    fn test_reduce_hide_filter_modal() {
        let mut state = state_with_data();
        state.filter_modal = Some(FilterState::default());
        let new_state = reduce_remote_state(state, RemoteAction::HideFilterModal, 20);
        assert!(new_state.filter_modal.is_none());
    }

    #[test]
    fn test_reduce_dismiss_toast() {
        let mut state = state_with_data();
        state.toast = Some(Toast::success("Test"));
        let new_state = reduce_remote_state(state, RemoteAction::DismissToast, 20);
        assert!(new_state.toast.is_none());
    }

    // ========================================================================
    // Link Mode Tests
    // ========================================================================

    #[test]
    fn test_reduce_start_link_mode() {
        let state = state_with_data();
        let new_state = reduce_remote_state(state, RemoteAction::StartLinkMode, 20);
        assert!(new_state.link_mode.is_some());
        assert_eq!(new_state.active_view, ViewMode::Remote); // Switched to select target
    }

    #[test]
    fn test_reduce_cancel_link_mode() {
        let mut state = state_with_data();
        state.link_mode = Some(LinkModeState::new(
            ViewMode::Local,
            "j-1".to_string(),
            "Task 1".to_string(),
        ));
        state.active_view = ViewMode::Remote;
        let new_state = reduce_remote_state(state, RemoteAction::CancelLinkMode, 20);
        assert!(new_state.link_mode.is_none());
        assert_eq!(new_state.active_view, ViewMode::Local); // Restored
    }

    // ========================================================================
    // App Tests
    // ========================================================================

    #[test]
    fn test_reduce_quit() {
        let state = state_with_data();
        let new_state = reduce_remote_state(state, RemoteAction::Quit, 20);
        assert!(new_state.should_exit);
    }

    // ========================================================================
    // Key Mapping Tests
    // ========================================================================

    #[test]
    fn test_key_to_action_navigation() {
        let state = default_state();
        assert_eq!(
            key_to_action(KeyCode::Char('j'), KeyModifiers::NONE, &state),
            Some(RemoteAction::MoveDown)
        );
        assert_eq!(
            key_to_action(KeyCode::Down, KeyModifiers::NONE, &state),
            Some(RemoteAction::MoveDown)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('k'), KeyModifiers::NONE, &state),
            Some(RemoteAction::MoveUp)
        );
        assert_eq!(
            key_to_action(KeyCode::Up, KeyModifiers::NONE, &state),
            Some(RemoteAction::MoveUp)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('g'), KeyModifiers::NONE, &state),
            Some(RemoteAction::GoToTop)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('G'), KeyModifiers::NONE, &state),
            Some(RemoteAction::GoToBottom)
        );
    }

    #[test]
    fn test_key_to_action_view() {
        let state = default_state();
        assert_eq!(
            key_to_action(KeyCode::Tab, KeyModifiers::NONE, &state),
            Some(RemoteAction::ToggleView)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('d'), KeyModifiers::NONE, &state),
            Some(RemoteAction::ToggleDetail)
        );
    }

    #[test]
    fn test_key_to_action_operations() {
        let state = default_state();
        assert_eq!(
            key_to_action(KeyCode::Char('r'), KeyModifiers::NONE, &state),
            Some(RemoteAction::Fetch)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('s'), KeyModifiers::NONE, &state),
            Some(RemoteAction::StartSync)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('l'), KeyModifiers::NONE, &state),
            Some(RemoteAction::StartLinkMode)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('u'), KeyModifiers::NONE, &state),
            Some(RemoteAction::Unlink)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('e'), KeyModifiers::NONE, &state),
            Some(RemoteAction::EditLocal)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('o'), KeyModifiers::NONE, &state),
            Some(RemoteAction::OpenRemote)
        );
    }

    #[test]
    fn test_key_to_action_modals() {
        let state = default_state();
        assert_eq!(
            key_to_action(KeyCode::Char('f'), KeyModifiers::NONE, &state),
            Some(RemoteAction::ShowFilterModal)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('?'), KeyModifiers::NONE, &state),
            Some(RemoteAction::ShowHelp)
        );
        // 'q' and Esc both quit
        assert_eq!(
            key_to_action(KeyCode::Char('q'), KeyModifiers::NONE, &state),
            Some(RemoteAction::Quit)
        );
        assert_eq!(
            key_to_action(KeyCode::Esc, KeyModifiers::NONE, &state),
            Some(RemoteAction::Quit)
        );
    }

    #[test]
    fn test_key_to_action_search_mode() {
        let mut state = default_state();
        state.search_focused = true;

        assert_eq!(
            key_to_action(KeyCode::Esc, KeyModifiers::NONE, &state),
            Some(RemoteAction::ClearSearchAndExit)
        );
        assert_eq!(
            key_to_action(KeyCode::Enter, KeyModifiers::NONE, &state),
            Some(RemoteAction::ExitSearch)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('q'), KeyModifiers::CONTROL, &state),
            Some(RemoteAction::Quit)
        );
        // Regular keys return None in search mode
        assert_eq!(
            key_to_action(KeyCode::Char('a'), KeyModifiers::NONE, &state),
            None
        );
    }

    #[test]
    fn test_key_to_action_help_modal() {
        let mut state = default_state();
        state.show_help_modal = true;

        assert_eq!(
            key_to_action(KeyCode::Esc, KeyModifiers::NONE, &state),
            Some(RemoteAction::HideHelp)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('?'), KeyModifiers::NONE, &state),
            Some(RemoteAction::HideHelp)
        );
        // Other keys return None
        assert_eq!(
            key_to_action(KeyCode::Char('j'), KeyModifiers::NONE, &state),
            None
        );
    }

    #[test]
    fn test_key_to_action_link_mode() {
        let mut state = default_state();
        state.link_mode = Some(LinkModeState::new(
            ViewMode::Local,
            "j-1".to_string(),
            "Task".to_string(),
        ));

        assert_eq!(
            key_to_action(KeyCode::Esc, KeyModifiers::NONE, &state),
            Some(RemoteAction::CancelLinkMode)
        );
        assert_eq!(
            key_to_action(KeyCode::Enter, KeyModifiers::NONE, &state),
            Some(RemoteAction::ConfirmLink)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('j'), KeyModifiers::NONE, &state),
            Some(RemoteAction::MoveDown)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('k'), KeyModifiers::NONE, &state),
            Some(RemoteAction::MoveUp)
        );
    }

    // ========================================================================
    // View Model Tests
    // ========================================================================

    #[test]
    fn test_compute_view_model_empty() {
        let state = default_state();
        let vm = compute_remote_view_model(&state, 20);

        assert_eq!(vm.local_list.tickets.len(), 0);
        assert_eq!(vm.remote_list.issues.len(), 0);
        assert!(vm.detail.local_ticket.is_none());
        assert!(vm.detail.remote_issue.is_none());
        assert!(!vm.is_loading);
    }

    #[test]
    fn test_compute_view_model_with_data() {
        let state = state_with_data();
        let vm = compute_remote_view_model(&state, 20);

        assert_eq!(vm.local_list.tickets.len(), 3);
        assert_eq!(vm.remote_list.issues.len(), 3);
        assert!(vm.detail.local_ticket.is_some());
        assert!(vm.detail.remote_issue.is_some());
        assert_eq!(vm.header.local_count, 3);
        assert_eq!(vm.header.remote_count, 3);
    }

    #[test]
    fn test_compute_view_model_with_search() {
        let mut state = state_with_data();
        state.search_query = "Task 1".to_string();
        let vm = compute_remote_view_model(&state, 20);

        assert_eq!(vm.local_list.tickets.len(), 1);
        assert_eq!(vm.search.query, "Task 1");
    }

    #[test]
    fn test_compute_view_model_focus_states() {
        let mut state = state_with_data();

        // Local view focused
        state.active_view = ViewMode::Local;
        let vm = compute_remote_view_model(&state, 20);
        assert!(vm.local_list.is_focused);
        assert!(!vm.remote_list.is_focused);

        // Remote view focused
        state.active_view = ViewMode::Remote;
        let vm = compute_remote_view_model(&state, 20);
        assert!(!vm.local_list.is_focused);
        assert!(vm.remote_list.is_focused);

        // Search focused
        state.search_focused = true;
        let vm = compute_remote_view_model(&state, 20);
        assert!(!vm.local_list.is_focused);
        assert!(!vm.remote_list.is_focused);
        assert!(vm.search.is_focused);
    }

    // ========================================================================
    // Helper Function Tests
    // ========================================================================

    #[test]
    fn test_adjust_scroll() {
        // Within bounds - no change
        assert_eq!(adjust_scroll(0, 5, 10), 0);
        assert_eq!(adjust_scroll(5, 8, 10), 5);

        // Below visible - scroll down
        assert_eq!(adjust_scroll(0, 15, 10), 6);

        // Above visible - scroll up
        assert_eq!(adjust_scroll(10, 5, 10), 5);

        // Zero height
        assert_eq!(adjust_scroll(5, 10, 0), 0);
    }

    // ========================================================================
    // Shortcut Tests
    // ========================================================================

    #[test]
    fn test_compute_shortcuts_normal() {
        let shortcuts = compute_shortcuts(&ModalVisibility::new(), ViewMode::Local);
        assert!(!shortcuts.is_empty());
        assert!(shortcuts.iter().any(|s| s.key == "C-q"));
        assert!(shortcuts.iter().any(|s| s.key == "Tab"));
    }

    #[test]
    fn test_compute_shortcuts_help_modal() {
        let shortcuts = compute_shortcuts(
            &ModalVisibility {
                show_help_modal: true,
                ..Default::default()
            },
            ViewMode::Local,
        );
        assert!(shortcuts.iter().any(|s| s.key == "Esc"));
    }

    #[test]
    fn test_compute_shortcuts_search() {
        let shortcuts = compute_shortcuts(
            &ModalVisibility {
                search_focused: true,
                ..Default::default()
            },
            ViewMode::Local,
        );
        assert!(shortcuts.iter().any(|s| s.key == "Enter"));
        assert!(shortcuts.iter().any(|s| s.key == "Esc"));
    }
}

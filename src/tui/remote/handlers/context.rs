//! Handler context containing grouped state references
//!
//! This module organizes the TUI state into logical groups, making it easier
//! to understand which state each handler needs and simplifying testing.

use std::collections::HashSet;

use iocraft::prelude::{Handler, State};

use crate::remote::Platform;
use crate::remote::{RemoteIssue, RemoteQuery};
use crate::tui::remote::link_mode::LinkSource;
use crate::tui::remote::state::{
    DetailScrollData, FilterConfigData, ModalVisibilityData, NavigationData, SearchUiData,
    ViewDisplayData, ViewMode,
};
use crate::tui::search_orchestrator::SearchState as SearchOrchestrator;
use crate::types::TicketMetadata;

use super::super::confirm_modal::ConfirmDialogState;
use super::super::error_toast::Toast;
use super::super::filter_modal::FilterState;
use super::super::link_mode::LinkModeState;
use super::super::sync_preview::SyncPreviewState;

/// Navigation state for a single view (local or remote) - using grouped state
pub struct NavigationState<'a> {
    pub nav: &'a mut State<NavigationData>,
}

impl<'a> NavigationState<'a> {
    pub fn selected_index(&self) -> usize {
        self.nav.read().selected_index
    }

    pub fn scroll_offset(&self) -> usize {
        self.nav.read().scroll_offset
    }

    pub fn selected_ids(&self) -> HashSet<String> {
        self.nav.read().selected_ids.clone()
    }

    pub fn set_selected_index(&mut self, index: usize) {
        let mut nav = self.nav.read().clone();
        nav.selected_index = index;
        self.nav.set(nav);
    }

    pub fn set_scroll_offset(&mut self, offset: usize) {
        let mut nav = self.nav.read().clone();
        nav.scroll_offset = offset;
        self.nav.set(nav);
    }

    pub fn set_selected_ids(&mut self, ids: HashSet<String>) {
        let mut nav = self.nav.read().clone();
        nav.selected_ids = ids;
        self.nav.set(nav);
    }

    pub fn clear_selection(&mut self) {
        let mut nav = self.nav.read().clone();
        nav.clear_selection();
        self.nav.set(nav);
    }

    #[allow(dead_code)]
    pub fn select_item(&mut self, index: usize) {
        let mut nav = self.nav.read().clone();
        nav.select_item(index);
        self.nav.set(nav);
    }
}

/// Data and navigation state for both local and remote views
#[allow(dead_code)] // Fields are cloned for future handler use
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
    /// Scroll offset for detail panes (grouped)
    pub detail_scroll: &'a mut State<DetailScrollData>,
    /// Cloned local tickets data (avoids complex re-read patterns)
    pub local_tickets_data: Vec<TicketMetadata>,
    /// Cloned remote issues data (avoids complex re-read patterns)
    pub remote_issues_data: Vec<RemoteIssue>,
}

/// Global view state (which view is active, exit flag, etc.) - using grouped state
pub struct ViewState<'a> {
    pub display: &'a mut State<ViewDisplayData>,
}

impl<'a> ViewState<'a> {
    pub fn active_view(&self) -> ViewMode {
        self.display.get().active_view
    }

    pub fn show_detail(&self) -> bool {
        self.display.get().show_detail
    }

    #[allow(dead_code)]
    pub fn should_exit(&self) -> bool {
        self.display.get().should_exit
    }

    pub fn detail_pane_focused(&self) -> bool {
        self.display.get().detail_pane_focused
    }

    pub fn set_active_view(&mut self, view: ViewMode) {
        let mut display = self.display.get();
        display.active_view = view;
        self.display.set(display);
    }

    pub fn toggle_view(&mut self) {
        let mut display = self.display.get();
        display.toggle_view();
        self.display.set(display);
    }

    #[allow(dead_code)]
    pub fn set_show_detail(&mut self, show: bool) {
        let mut display = self.display.get();
        display.show_detail = show;
        self.display.set(display);
    }

    pub fn toggle_show_detail(&mut self) {
        let mut display = self.display.get();
        display.show_detail = !display.show_detail;
        self.display.set(display);
    }

    pub fn set_should_exit(&mut self, exit: bool) {
        let mut display = self.display.get();
        display.should_exit = exit;
        self.display.set(display);
    }

    pub fn set_detail_pane_focused(&mut self, focused: bool) {
        let mut display = *self.display.read();
        display.detail_pane_focused = focused;
        self.display.set(display);
    }

    #[allow(dead_code)]
    pub fn loading(&self) -> bool {
        self.display.read().remote_loading
    }

    pub fn set_loading(&mut self, loading: bool) {
        let mut display = *self.display.read();
        display.remote_loading = loading;
        self.display.set(display);
    }
}

/// Search functionality state - using grouped state
pub struct SearchState<'a> {
    pub ui: &'a mut State<SearchUiData>,
    /// Search orchestrator for Enter-triggered search
    pub orchestrator: &'a mut SearchOrchestrator,
}

impl<'a> SearchState<'a> {
    #[allow(dead_code)]
    pub fn query(&self) -> String {
        self.ui.read().query.clone()
    }

    pub fn is_focused(&self) -> bool {
        self.ui.read().focused
    }

    pub fn set_query(&mut self, query: String) {
        let mut ui = self.ui.read().clone();
        ui.query = query;
        self.ui.set(ui);
    }

    pub fn set_focused(&mut self, focused: bool) {
        let mut ui = self.ui.read().clone();
        ui.focused = focused;
        self.ui.set(ui);
    }
}

/// Modal and operation states
pub struct ModalState<'a> {
    pub toast: &'a mut State<Option<Toast>>,
    pub link_mode: &'a mut State<Option<LinkModeState>>,
    pub sync_preview: &'a mut State<Option<SyncPreviewState>>,
    pub confirm_dialog: &'a mut State<Option<ConfirmDialogState>>,
    /// Modal visibility state (grouped)
    pub visibility: &'a mut State<ModalVisibilityData>,
    #[allow(dead_code)]
    pub last_error: &'a State<Option<(String, String)>>,
}

impl<'a> ModalState<'a> {
    pub fn show_help(&self) -> bool {
        self.visibility.get().show_help
    }

    pub fn help_scroll(&self) -> usize {
        self.visibility.get().help_scroll
    }

    pub fn show_error(&self) -> bool {
        self.visibility.get().show_error
    }

    pub fn set_show_help(&mut self, show: bool) {
        let mut visibility = self.visibility.get();
        visibility.show_help = show;
        self.visibility.set(visibility);
    }

    pub fn toggle_help(&mut self) {
        let mut visibility = self.visibility.get();
        visibility.show_help = !visibility.show_help;
        self.visibility.set(visibility);
    }

    pub fn set_help_scroll(&mut self, scroll: usize) {
        let mut visibility = self.visibility.get();
        visibility.help_scroll = scroll;
        self.visibility.set(visibility);
    }

    #[allow(dead_code)]
    pub fn scroll_help_up(&mut self, lines: usize) {
        let mut visibility = self.visibility.get();
        visibility.help_scroll = visibility.help_scroll.saturating_sub(lines);
        self.visibility.set(visibility);
    }

    #[allow(dead_code)]
    pub fn scroll_help_down(&mut self, lines: usize) {
        let mut visibility = self.visibility.get();
        visibility.help_scroll += lines;
        self.visibility.set(visibility);
    }

    pub fn set_show_error(&mut self, show: bool) {
        let mut visibility = self.visibility.get();
        visibility.show_error = show;
        self.visibility.set(visibility);
    }

    pub fn toggle_error(&mut self) {
        let mut visibility = self.visibility.get();
        visibility.show_error = !visibility.show_error;
        self.visibility.set(visibility);
    }
}

/// Filter and provider state - using grouped state
pub struct FilteringState<'a> {
    pub filter_modal: &'a mut State<Option<FilterState>>,
    /// Filter configuration (grouped)
    pub config: &'a mut State<FilterConfigData>,
}

impl<'a> FilteringState<'a> {
    pub fn active_filters(&self) -> RemoteQuery {
        self.config.read().clone().active_filters.clone()
    }

    pub fn provider(&self) -> Platform {
        self.config.read().clone().provider
    }

    pub fn set_active_filters(&mut self, filters: RemoteQuery) {
        let mut config = self.config.read().clone();
        config.active_filters = filters;
        self.config.set(config);
    }

    pub fn set_provider(&mut self, platform: Platform) {
        let mut config = self.config.read().clone();
        config.provider = platform;
        self.config.set(config);
    }
}

/// Async operation handlers
pub struct AsyncHandlers<'a> {
    pub fetch_handler: &'a Handler<(Platform, RemoteQuery)>,
    pub push_handler: &'a Handler<(Vec<String>, Platform, RemoteQuery)>,
    pub sync_fetch_handler: &'a Handler<(Vec<String>, Platform)>,
    pub sync_apply_handler: &'a Handler<(SyncPreviewState, Platform, RemoteQuery)>,
    pub link_handler: &'a Handler<LinkSource>,
    pub unlink_handler: &'a Handler<Vec<String>>,
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
    pub handlers: AsyncHandlers<'a>,
}

impl<'a> HandlerContext<'a> {
    /// Build a lightweight, read-only snapshot of which modals/modes are active.
    ///
    /// This is consumed by `keymap::key_to_action` so that key mapping is a
    /// pure function that doesn't need mutable access to the context.
    pub fn modal_state_snapshot(&self) -> super::keymap::ModalStateSnapshot {
        super::keymap::ModalStateSnapshot {
            show_help_modal: self.modals.show_help(),
            show_error_modal: self.modals.show_error(),
            sync_preview_active: self.modals.sync_preview.read().is_some(),
            sync_preview_current_index: self
                .modals
                .sync_preview
                .read()
                .as_ref()
                .map(|s| s.current_change_index),
            link_mode_active: self.modals.link_mode.read().is_some(),
            filter_modal_active: self.filters.filter_modal.read().is_some(),
            confirm_dialog_active: self.modals.confirm_dialog.read().is_some(),
            search_focused: self.search.is_focused(),
            detail_pane_focused: self.view_state.detail_pane_focused(),
        }
    }
}

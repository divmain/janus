//! Key-to-action mapping for the remote TUI
//!
//! This module is the single authoritative key mapper. It converts raw
//! `(KeyCode, KeyModifiers)` pairs into high-level `RemoteAction` values,
//! taking modal state into account so that each key press resolves to at
//! most one action.

use iocraft::prelude::{KeyCode, KeyModifiers};

// ============================================================================
// Action enum
// ============================================================================

/// All possible actions the handler system can dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum RemoteAction {
    // Navigation
    MoveUp,
    MoveDown,
    MoveUpExtendSelection,
    MoveDownExtendSelection,
    GoToTop,
    GoToBottom,
    PageUp,
    PageDown,

    // View
    ToggleView,
    ToggleDetail,

    // Selection
    ToggleSelection,
    SelectAll,
    ClearSelection,

    // Search
    FocusSearch,
    ExitSearch,
    ClearSearchAndExit,

    // Modals
    ShowHelp,
    HideHelp,
    ShowFilterModal,
    HideFilterModal,
    ToggleErrorModal,
    DismissModal,

    // Help modal scrolling
    ScrollHelpUp,
    ScrollHelpDown,
    ScrollHelpToTop,
    ScrollHelpToBottom,

    // Detail pane scrolling
    DetailScrollUp,
    DetailScrollDown,
    DetailScrollToTop,
    DetailScrollToBottom,

    // Confirm dialog
    ConfirmYes,
    ConfirmNo,

    // Sync preview
    StartSync,
    SyncAccept,
    SyncSkip,
    SyncAcceptAll,
    SyncCancel,

    // Link mode
    StartLinkMode,
    CancelLinkMode,
    LinkConfirm,

    // Filter modal
    FilterTab,
    FilterBackTab,
    FilterClear,
    FilterEnter,
    FilterMoveDown,
    FilterMoveUp,

    // Operations
    Refresh,
    SwitchProvider,
    Adopt,
    PushLocal,
    UnlinkLocal,

    // App
    Quit,

    /// Key was recognised but requires no further action (absorb it).
    Consumed,
}

// ============================================================================
// Modal state snapshot
// ============================================================================

/// Lightweight, read-only snapshot of which modals/modes are currently active.
///
/// Built from `HandlerContext` so that `key_to_action` can be a pure function.
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct ModalStateSnapshot {
    pub show_help_modal: bool,
    pub show_error_modal: bool,
    pub sync_preview_active: bool,
    pub sync_preview_current_index: Option<usize>,
    pub link_mode_active: bool,
    pub filter_modal_active: bool,
    pub confirm_dialog_active: bool,
    pub search_focused: bool,
    pub detail_pane_focused: bool,
}

// ============================================================================
// Public entry point
// ============================================================================

/// Map a raw key event to an abstract `RemoteAction`.
///
/// Returns `None` when the key has no mapping in the current modal context
/// (i.e. let the iocraft component handle it – e.g. typing into search).
pub fn key_to_action(
    code: KeyCode,
    modifiers: KeyModifiers,
    state: &ModalStateSnapshot,
) -> Option<RemoteAction> {
    // Priority order mirrors the old if/return chain but is now data-driven.

    // 1. Help modal – captures all keys
    if state.show_help_modal {
        return help_modal_key(code);
    }

    // 2. Error modal – captures all keys
    if state.show_error_modal {
        return error_modal_key(code);
    }

    // 3. Confirm dialog – captures all keys
    if state.confirm_dialog_active {
        return confirm_dialog_key(code);
    }

    // 4. Sync preview – captures all keys
    if state.sync_preview_active {
        return sync_preview_key(code);
    }

    // 5. Filter modal – captures all keys
    if state.filter_modal_active {
        return filter_modal_key(code);
    }

    // 6. Link mode – captures all keys
    if state.link_mode_active {
        return link_mode_key(code);
    }

    // 7. Search mode – Esc/Enter/Ctrl-Q are intercepted; everything else
    //    falls through to the search-box component (returns None).
    if state.search_focused {
        return search_key_to_action(code, modifiers);
    }

    // 8. Detail pane focused
    if state.detail_pane_focused {
        return detail_pane_key(code);
    }

    // 9. Normal mode
    normal_key_to_action(code, modifiers)
}

// ============================================================================
// Per-mode mappers
// ============================================================================

fn help_modal_key(code: KeyCode) -> Option<RemoteAction> {
    match code {
        KeyCode::Char('j') | KeyCode::Down => Some(RemoteAction::ScrollHelpDown),
        KeyCode::Char('k') | KeyCode::Up => Some(RemoteAction::ScrollHelpUp),
        KeyCode::Char('g') => Some(RemoteAction::ScrollHelpToTop),
        KeyCode::Char('G') => Some(RemoteAction::ScrollHelpToBottom),
        KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => Some(RemoteAction::HideHelp),
        _ => Some(RemoteAction::Consumed),
    }
}

fn error_modal_key(code: KeyCode) -> Option<RemoteAction> {
    match code {
        KeyCode::Esc => Some(RemoteAction::ToggleErrorModal),
        _ => Some(RemoteAction::Consumed),
    }
}

fn confirm_dialog_key(code: KeyCode) -> Option<RemoteAction> {
    match code {
        KeyCode::Char('y') | KeyCode::Char('Y') => Some(RemoteAction::ConfirmYes),
        KeyCode::Char('n')
        | KeyCode::Char('N')
        | KeyCode::Char('c')
        | KeyCode::Char('C')
        | KeyCode::Esc => Some(RemoteAction::ConfirmNo),
        _ => Some(RemoteAction::Consumed),
    }
}

fn sync_preview_key(code: KeyCode) -> Option<RemoteAction> {
    match code {
        KeyCode::Char('y') => Some(RemoteAction::SyncAccept),
        KeyCode::Char('n') => Some(RemoteAction::SyncSkip),
        KeyCode::Char('a') => Some(RemoteAction::SyncAcceptAll),
        KeyCode::Char('c') | KeyCode::Esc => Some(RemoteAction::SyncCancel),
        KeyCode::Char('j') | KeyCode::Down => Some(RemoteAction::MoveDown),
        KeyCode::Char('k') | KeyCode::Up => Some(RemoteAction::MoveUp),
        _ => Some(RemoteAction::Consumed),
    }
}

fn link_mode_key(code: KeyCode) -> Option<RemoteAction> {
    match code {
        KeyCode::Char('l') => Some(RemoteAction::LinkConfirm),
        KeyCode::Esc => Some(RemoteAction::CancelLinkMode),
        KeyCode::Char('j') | KeyCode::Down => Some(RemoteAction::MoveDown),
        KeyCode::Char('k') | KeyCode::Up => Some(RemoteAction::MoveUp),
        _ => Some(RemoteAction::Consumed),
    }
}

fn filter_modal_key(code: KeyCode) -> Option<RemoteAction> {
    match code {
        KeyCode::Tab => Some(RemoteAction::FilterTab),
        KeyCode::BackTab => Some(RemoteAction::FilterBackTab),
        KeyCode::Char('x') => Some(RemoteAction::FilterClear),
        KeyCode::Enter => Some(RemoteAction::FilterEnter),
        KeyCode::Char('j') | KeyCode::Down => Some(RemoteAction::FilterMoveDown),
        KeyCode::Char('k') | KeyCode::Up => Some(RemoteAction::FilterMoveUp),
        KeyCode::Esc => Some(RemoteAction::HideFilterModal),
        _ => Some(RemoteAction::Consumed),
    }
}

fn detail_pane_key(code: KeyCode) -> Option<RemoteAction> {
    match code {
        KeyCode::Char('j') | KeyCode::Down => Some(RemoteAction::DetailScrollDown),
        KeyCode::Char('k') | KeyCode::Up => Some(RemoteAction::DetailScrollUp),
        KeyCode::Char('g') => Some(RemoteAction::DetailScrollToTop),
        KeyCode::Char('G') => Some(RemoteAction::DetailScrollToBottom),
        KeyCode::Tab | KeyCode::BackTab => Some(RemoteAction::ToggleView),
        KeyCode::Esc => Some(RemoteAction::DismissModal),
        _ => Some(RemoteAction::Consumed),
    }
}

/// Keys recognised while the search box is focused.
///
/// Returns `None` for normal typing so the search-box component can handle it.
fn search_key_to_action(code: KeyCode, modifiers: KeyModifiers) -> Option<RemoteAction> {
    match (code, modifiers) {
        (KeyCode::Esc, _) => Some(RemoteAction::ClearSearchAndExit),
        (KeyCode::Enter, _) => Some(RemoteAction::ExitSearch),
        (KeyCode::Char('q'), m) if m.contains(KeyModifiers::CONTROL) => Some(RemoteAction::Quit),
        _ => None, // let the search component handle it
    }
}

/// Normal-mode key mapping (no modals active, search not focused).
fn normal_key_to_action(code: KeyCode, modifiers: KeyModifiers) -> Option<RemoteAction> {
    // Handle shift-modified keys first
    if modifiers.contains(KeyModifiers::SHIFT) {
        return match code {
            KeyCode::Char('J') | KeyCode::Char('j') => Some(RemoteAction::MoveDownExtendSelection),
            KeyCode::Char('K') | KeyCode::Char('k') => Some(RemoteAction::MoveUpExtendSelection),
            KeyCode::Char('G') | KeyCode::Char('g') => Some(RemoteAction::GoToBottom),
            KeyCode::Char('P') | KeyCode::Char('p') => Some(RemoteAction::SwitchProvider),
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
        (KeyCode::BackTab, _) => Some(RemoteAction::ToggleView),
        (KeyCode::Char('d'), KeyModifiers::NONE) => Some(RemoteAction::ToggleDetail),
        (KeyCode::Enter, KeyModifiers::NONE) => Some(RemoteAction::ToggleDetail),

        // Selection
        (KeyCode::Char(' '), KeyModifiers::NONE) => Some(RemoteAction::ToggleSelection),

        // Search
        (KeyCode::Char('/'), KeyModifiers::NONE) => Some(RemoteAction::FocusSearch),

        // Operations
        (KeyCode::Char('r'), KeyModifiers::NONE) => Some(RemoteAction::Refresh),
        (KeyCode::Char('s'), KeyModifiers::NONE) => Some(RemoteAction::StartSync),
        (KeyCode::Char('l'), KeyModifiers::NONE) => Some(RemoteAction::StartLinkMode),
        (KeyCode::Char('p'), KeyModifiers::NONE) => Some(RemoteAction::PushLocal),
        (KeyCode::Char('u'), KeyModifiers::NONE) => Some(RemoteAction::UnlinkLocal),
        (KeyCode::Char('a'), KeyModifiers::NONE) => Some(RemoteAction::Adopt),

        // Modals
        (KeyCode::Char('f'), KeyModifiers::NONE) => Some(RemoteAction::ShowFilterModal),
        (KeyCode::Char('?'), KeyModifiers::NONE) => Some(RemoteAction::ShowHelp),
        (KeyCode::Char('e'), KeyModifiers::NONE) => Some(RemoteAction::ToggleErrorModal),

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

    fn default_snapshot() -> ModalStateSnapshot {
        ModalStateSnapshot::default()
    }

    // ====================================================================
    // Normal-mode navigation
    // ====================================================================

    #[test]
    fn test_key_to_action_navigation() {
        let s = default_snapshot();
        assert_eq!(
            key_to_action(KeyCode::Char('j'), KeyModifiers::NONE, &s),
            Some(RemoteAction::MoveDown)
        );
        assert_eq!(
            key_to_action(KeyCode::Down, KeyModifiers::NONE, &s),
            Some(RemoteAction::MoveDown)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('k'), KeyModifiers::NONE, &s),
            Some(RemoteAction::MoveUp)
        );
        assert_eq!(
            key_to_action(KeyCode::Up, KeyModifiers::NONE, &s),
            Some(RemoteAction::MoveUp)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('g'), KeyModifiers::NONE, &s),
            Some(RemoteAction::GoToTop)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('G'), KeyModifiers::NONE, &s),
            Some(RemoteAction::GoToBottom)
        );
        assert_eq!(
            key_to_action(KeyCode::PageUp, KeyModifiers::NONE, &s),
            Some(RemoteAction::PageUp)
        );
        assert_eq!(
            key_to_action(KeyCode::PageDown, KeyModifiers::NONE, &s),
            Some(RemoteAction::PageDown)
        );
    }

    // ====================================================================
    // Normal-mode view
    // ====================================================================

    #[test]
    fn test_key_to_action_view() {
        let s = default_snapshot();
        assert_eq!(
            key_to_action(KeyCode::Tab, KeyModifiers::NONE, &s),
            Some(RemoteAction::ToggleView)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('d'), KeyModifiers::NONE, &s),
            Some(RemoteAction::ToggleDetail)
        );
        assert_eq!(
            key_to_action(KeyCode::Enter, KeyModifiers::NONE, &s),
            Some(RemoteAction::ToggleDetail)
        );
    }

    // ====================================================================
    // Normal-mode operations
    // ====================================================================

    #[test]
    fn test_key_to_action_operations() {
        let s = default_snapshot();
        assert_eq!(
            key_to_action(KeyCode::Char('r'), KeyModifiers::NONE, &s),
            Some(RemoteAction::Refresh)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('s'), KeyModifiers::NONE, &s),
            Some(RemoteAction::StartSync)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('l'), KeyModifiers::NONE, &s),
            Some(RemoteAction::StartLinkMode)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('a'), KeyModifiers::NONE, &s),
            Some(RemoteAction::Adopt)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('p'), KeyModifiers::NONE, &s),
            Some(RemoteAction::PushLocal)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('u'), KeyModifiers::NONE, &s),
            Some(RemoteAction::UnlinkLocal)
        );
    }

    // ====================================================================
    // Normal-mode modals
    // ====================================================================

    #[test]
    fn test_key_to_action_modals() {
        let s = default_snapshot();
        assert_eq!(
            key_to_action(KeyCode::Char('f'), KeyModifiers::NONE, &s),
            Some(RemoteAction::ShowFilterModal)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('?'), KeyModifiers::NONE, &s),
            Some(RemoteAction::ShowHelp)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('e'), KeyModifiers::NONE, &s),
            Some(RemoteAction::ToggleErrorModal)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('q'), KeyModifiers::NONE, &s),
            Some(RemoteAction::Quit)
        );
        assert_eq!(
            key_to_action(KeyCode::Esc, KeyModifiers::NONE, &s),
            Some(RemoteAction::Quit)
        );
    }

    // ====================================================================
    // Search mode
    // ====================================================================

    #[test]
    fn test_key_to_action_search_mode() {
        let s = ModalStateSnapshot {
            search_focused: true,
            ..default_snapshot()
        };

        assert_eq!(
            key_to_action(KeyCode::Esc, KeyModifiers::NONE, &s),
            Some(RemoteAction::ClearSearchAndExit)
        );
        assert_eq!(
            key_to_action(KeyCode::Enter, KeyModifiers::NONE, &s),
            Some(RemoteAction::ExitSearch)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('q'), KeyModifiers::CONTROL, &s),
            Some(RemoteAction::Quit)
        );
        // Regular keys return None so the search component handles them
        assert_eq!(
            key_to_action(KeyCode::Char('a'), KeyModifiers::NONE, &s),
            None
        );
    }

    // ====================================================================
    // Help modal
    // ====================================================================

    #[test]
    fn test_key_to_action_help_modal() {
        let s = ModalStateSnapshot {
            show_help_modal: true,
            ..default_snapshot()
        };

        assert_eq!(
            key_to_action(KeyCode::Esc, KeyModifiers::NONE, &s),
            Some(RemoteAction::HideHelp)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('?'), KeyModifiers::NONE, &s),
            Some(RemoteAction::HideHelp)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('q'), KeyModifiers::NONE, &s),
            Some(RemoteAction::HideHelp)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('j'), KeyModifiers::NONE, &s),
            Some(RemoteAction::ScrollHelpDown)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('k'), KeyModifiers::NONE, &s),
            Some(RemoteAction::ScrollHelpUp)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('g'), KeyModifiers::NONE, &s),
            Some(RemoteAction::ScrollHelpToTop)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('G'), KeyModifiers::NONE, &s),
            Some(RemoteAction::ScrollHelpToBottom)
        );
        // Other keys consumed
        assert_eq!(
            key_to_action(KeyCode::Char('x'), KeyModifiers::NONE, &s),
            Some(RemoteAction::Consumed)
        );
    }

    // ====================================================================
    // Link mode
    // ====================================================================

    #[test]
    fn test_key_to_action_link_mode() {
        let s = ModalStateSnapshot {
            link_mode_active: true,
            ..default_snapshot()
        };

        assert_eq!(
            key_to_action(KeyCode::Esc, KeyModifiers::NONE, &s),
            Some(RemoteAction::CancelLinkMode)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('l'), KeyModifiers::NONE, &s),
            Some(RemoteAction::LinkConfirm)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('j'), KeyModifiers::NONE, &s),
            Some(RemoteAction::MoveDown)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('k'), KeyModifiers::NONE, &s),
            Some(RemoteAction::MoveUp)
        );
        // Other keys consumed
        assert_eq!(
            key_to_action(KeyCode::Char('x'), KeyModifiers::NONE, &s),
            Some(RemoteAction::Consumed)
        );
    }

    // ====================================================================
    // Confirm dialog
    // ====================================================================

    #[test]
    fn test_key_to_action_confirm_dialog() {
        let s = ModalStateSnapshot {
            confirm_dialog_active: true,
            ..default_snapshot()
        };

        assert_eq!(
            key_to_action(KeyCode::Char('y'), KeyModifiers::NONE, &s),
            Some(RemoteAction::ConfirmYes)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('Y'), KeyModifiers::NONE, &s),
            Some(RemoteAction::ConfirmYes)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('n'), KeyModifiers::NONE, &s),
            Some(RemoteAction::ConfirmNo)
        );
        assert_eq!(
            key_to_action(KeyCode::Esc, KeyModifiers::NONE, &s),
            Some(RemoteAction::ConfirmNo)
        );
        // Other keys consumed
        assert_eq!(
            key_to_action(KeyCode::Char('x'), KeyModifiers::NONE, &s),
            Some(RemoteAction::Consumed)
        );
    }

    // ====================================================================
    // Detail pane focus
    // ====================================================================

    #[test]
    fn test_key_to_action_detail_pane_focused() {
        let s = ModalStateSnapshot {
            detail_pane_focused: true,
            ..default_snapshot()
        };

        assert_eq!(
            key_to_action(KeyCode::Char('j'), KeyModifiers::NONE, &s),
            Some(RemoteAction::DetailScrollDown)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('k'), KeyModifiers::NONE, &s),
            Some(RemoteAction::DetailScrollUp)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('g'), KeyModifiers::NONE, &s),
            Some(RemoteAction::DetailScrollToTop)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('G'), KeyModifiers::NONE, &s),
            Some(RemoteAction::DetailScrollToBottom)
        );
        assert_eq!(
            key_to_action(KeyCode::Tab, KeyModifiers::NONE, &s),
            Some(RemoteAction::ToggleView)
        );
        assert_eq!(
            key_to_action(KeyCode::Esc, KeyModifiers::NONE, &s),
            Some(RemoteAction::DismissModal)
        );
        // Other keys consumed
        assert_eq!(
            key_to_action(KeyCode::Char('x'), KeyModifiers::NONE, &s),
            Some(RemoteAction::Consumed)
        );
    }

    // ====================================================================
    // Sync preview
    // ====================================================================

    #[test]
    fn test_key_to_action_sync_preview() {
        let s = ModalStateSnapshot {
            sync_preview_active: true,
            sync_preview_current_index: Some(0),
            ..default_snapshot()
        };

        assert_eq!(
            key_to_action(KeyCode::Char('y'), KeyModifiers::NONE, &s),
            Some(RemoteAction::SyncAccept)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('n'), KeyModifiers::NONE, &s),
            Some(RemoteAction::SyncSkip)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('a'), KeyModifiers::NONE, &s),
            Some(RemoteAction::SyncAcceptAll)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('c'), KeyModifiers::NONE, &s),
            Some(RemoteAction::SyncCancel)
        );
        assert_eq!(
            key_to_action(KeyCode::Esc, KeyModifiers::NONE, &s),
            Some(RemoteAction::SyncCancel)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('j'), KeyModifiers::NONE, &s),
            Some(RemoteAction::MoveDown)
        );
        // Other keys consumed
        assert_eq!(
            key_to_action(KeyCode::Char('z'), KeyModifiers::NONE, &s),
            Some(RemoteAction::Consumed)
        );
    }

    // ====================================================================
    // Filter modal
    // ====================================================================

    #[test]
    fn test_key_to_action_filter_modal() {
        let s = ModalStateSnapshot {
            filter_modal_active: true,
            ..default_snapshot()
        };

        assert_eq!(
            key_to_action(KeyCode::Tab, KeyModifiers::NONE, &s),
            Some(RemoteAction::FilterTab)
        );
        assert_eq!(
            key_to_action(KeyCode::BackTab, KeyModifiers::NONE, &s),
            Some(RemoteAction::FilterBackTab)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('x'), KeyModifiers::NONE, &s),
            Some(RemoteAction::FilterClear)
        );
        assert_eq!(
            key_to_action(KeyCode::Enter, KeyModifiers::NONE, &s),
            Some(RemoteAction::FilterEnter)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('j'), KeyModifiers::NONE, &s),
            Some(RemoteAction::FilterMoveDown)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('k'), KeyModifiers::NONE, &s),
            Some(RemoteAction::FilterMoveUp)
        );
        assert_eq!(
            key_to_action(KeyCode::Esc, KeyModifiers::NONE, &s),
            Some(RemoteAction::HideFilterModal)
        );
        // Other keys consumed
        assert_eq!(
            key_to_action(KeyCode::Char('z'), KeyModifiers::NONE, &s),
            Some(RemoteAction::Consumed)
        );
    }

    // ====================================================================
    // Error modal
    // ====================================================================

    #[test]
    fn test_key_to_action_error_modal() {
        let s = ModalStateSnapshot {
            show_error_modal: true,
            ..default_snapshot()
        };

        assert_eq!(
            key_to_action(KeyCode::Esc, KeyModifiers::NONE, &s),
            Some(RemoteAction::ToggleErrorModal)
        );
        // Other keys consumed
        assert_eq!(
            key_to_action(KeyCode::Char('j'), KeyModifiers::NONE, &s),
            Some(RemoteAction::Consumed)
        );
    }

    // ====================================================================
    // Shift modifiers (extend selection, provider switch)
    // ====================================================================

    #[test]
    fn test_key_to_action_shift_modifiers() {
        let s = default_snapshot();
        assert_eq!(
            key_to_action(KeyCode::Char('J'), KeyModifiers::SHIFT, &s),
            Some(RemoteAction::MoveDownExtendSelection)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('K'), KeyModifiers::SHIFT, &s),
            Some(RemoteAction::MoveUpExtendSelection)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('G'), KeyModifiers::SHIFT, &s),
            Some(RemoteAction::GoToBottom)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('P'), KeyModifiers::SHIFT, &s),
            Some(RemoteAction::SwitchProvider)
        );
    }
}

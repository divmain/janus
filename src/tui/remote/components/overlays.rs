//! Remote TUI modal overlay components
//!
//! Contains the modal overlay rendering for filter, help, error, confirm, and sync preview modals.

use iocraft::prelude::*;

use crate::tui::remote::confirm_modal::{ConfirmDialog, ConfirmDialogState};
use crate::tui::remote::error_modal::ErrorDetailModal;
use crate::tui::remote::filter_modal::{FilterModal, FilterState};
use crate::tui::remote::help_modal::HelpModal;
use crate::tui::remote::sync_preview::{SyncPreview, SyncPreviewState};

/// Props for the ModalOverlays component
#[derive(Default, Props)]
pub struct ModalOverlaysProps {
    /// Filter modal state (Some if modal should be shown)
    pub filter_state: Option<FilterState>,
    /// Whether to show the help modal
    pub show_help_modal: bool,
    /// Help modal scroll offset
    pub help_modal_scroll: usize,
    /// Handler for help modal scroll up
    pub on_help_scroll_up: Option<Handler<()>>,
    /// Handler for help modal scroll down
    pub on_help_scroll_down: Option<Handler<()>>,
    /// Whether to show the error modal
    pub show_error_modal: bool,
    /// Last error information (type, message)
    pub last_error: Option<(String, String)>,
    /// Sync preview state (Some if modal should be shown)
    pub sync_preview_state: Option<SyncPreviewState>,
    /// Confirm dialog state (Some if modal should be shown)
    pub confirm_dialog_state: Option<ConfirmDialogState>,
}

/// Modal overlays container for filter, help, and error modals
#[component]
pub fn ModalOverlays(props: &ModalOverlaysProps) -> impl Into<AnyElement<'static>> {
    // Wrapper View with proper positioning so children (ModalOverlay) can use absolute positioning
    // This wrapper has no visual presence - just provides positioning context
    element! {
        View(
            width: 100pct,
            height: 100pct,
            position: Position::Absolute,
            top: 0,
            left: 0,
        ) {
            // Filter modal - rendered directly since FilterModal handles its own positioning via ModalOverlay
            #(props.filter_state.as_ref().map(|state| {
                let state_clone = state.clone();
                element! { FilterModal(state: state_clone, on_close: None) }
            }))

            // Help modal - rendered directly since HelpModal handles its own positioning via ModalOverlay
            #(if props.show_help_modal {
                let scroll = props.help_modal_scroll;
                Some(element! {
                    HelpModal(
                        scroll_offset: Some(scroll),
                        on_close: None,
                        on_scroll_up: props.on_help_scroll_up.clone(),
                        on_scroll_down: props.on_help_scroll_down.clone(),
                    )
                })
            } else {
                None
            })

            // Error detail modal - rendered directly since ErrorDetailModal handles its own positioning via ModalOverlay
            #(if props.show_error_modal {
                props.last_error.as_ref().map(|(error_type, error_message)| {
                    let error_type_clone = error_type.clone();
                    let error_message_clone = error_message.clone();
                    element! {
                        ErrorDetailModal(error_type: error_type_clone.clone(), error_message: error_message_clone.clone(), on_close: None)
                    }
                })
            } else {
                None
            })

            // Sync preview modal - rendered directly since SyncPreview handles its own positioning via ModalOverlay
            #(props.sync_preview_state.as_ref().map(|state| {
                let state_clone = state.clone();
                element! {
                    SyncPreview(
                        changes: state_clone.changes,
                        current_change_index: state_clone.current_change_index,
                        scroll_offset: state_clone.scroll_offset,
                        on_close: state_clone.on_close,
                        on_scroll_up: state_clone.on_scroll_up,
                        on_scroll_down: state_clone.on_scroll_down,
                        on_accept: state_clone.on_accept,
                        on_skip: state_clone.on_skip,
                        on_accept_all: state_clone.on_accept_all,
                        on_cancel: state_clone.on_cancel,
                    )
                }
            }))

            // Confirm dialog modal - rendered directly since ConfirmDialog handles its own positioning via ModalOverlay
            #(props.confirm_dialog_state.as_ref().map(|state| {
                let message = state.message.clone();
                element! { ConfirmDialog(message: message, on_close: None) }
            }))
        }
    }
}

/// Render a link mode banner
pub fn render_link_mode_banner(
    link_mode: &Option<crate::tui::remote::link_mode::LinkModeState>,
) -> Option<AnyElement<'static>> {
    link_mode.as_ref().map(|lm| {
        element! {
                View(
                    width: 100pct,
                    padding_left: 1,
                    padding_right: 1,
                    border_edges: Edges::Bottom,
                    border_style: BorderStyle::Single,
                    border_color: Color::Yellow,
                    background_color: Color::Rgb { r: 120, g: 120, b: 120 },
                ) {
                Text(
                    content: format!(
                        "Link {} ({}) -> select target, [l] to confirm, [Esc] to cancel",
                        lm.source_id,
                        lm.source_title
                    ),
                    color: Color::Yellow,
                )
            }
        }
        .into_any()
    })
}

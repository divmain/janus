//! Remote TUI modal overlay components
//!
//! Contains the modal overlay rendering for filter, help, and error modals.

use iocraft::prelude::*;

use crate::tui::remote::error_modal::ErrorDetailModal;
use crate::tui::remote::filter_modal::{FilterModal, FilterState};
use crate::tui::remote::help_modal::HelpModal;

/// Props for the ModalOverlays component
#[derive(Default, Props)]
pub struct ModalOverlaysProps {
    /// Filter modal state (Some if modal should be shown)
    pub filter_state: Option<FilterState>,
    /// Whether to show the help modal
    pub show_help_modal: bool,
    /// Whether to show the error modal
    pub show_error_modal: bool,
    /// Last error information (type, message)
    pub last_error: Option<(String, String)>,
}

/// Modal overlays container for filter, help, and error modals
#[component]
pub fn ModalOverlays(props: &ModalOverlaysProps) -> impl Into<AnyElement<'static>> {
    element! {
        View() {
            // Filter modal overlay
            #(props.filter_state.as_ref().map(|state| {
                let state_clone = state.clone();
                element! {
                    View(
                        width: 100pct,
                        height: 100pct,
                        position: Position::Absolute,
                        top: 0,
                        left: 0,
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        background_color: Color::DarkGrey,
                    ) {
                        FilterModal(state: state_clone)
                    }
                }
            }))

            // Help modal overlay
            #(if props.show_help_modal {
                Some(element! {
                    View(
                        width: 100pct,
                        height: 100pct,
                        position: Position::Absolute,
                        top: 0,
                        left: 0,
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        background_color: Color::DarkGrey,
                    ) {
                        HelpModal()
                    }
                })
            } else {
                None
            })

            // Error detail modal overlay
            #(if props.show_error_modal {
                props.last_error.as_ref().map(|(error_type, error_message)| {
                    let error_type_clone = error_type.clone();
                    let error_message_clone = error_message.clone();
                    element! {
                        View(
                            width: 100pct,
                            height: 100pct,
                            position: Position::Absolute,
                            top: 0,
                            left: 0,
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            background_color: Color::DarkGrey,
                        ) {
                            ErrorDetailModal(error_type: error_type_clone.clone(), error_message: error_message_clone.clone())
                        }
                    }
                })
            } else {
                None
            })
        }
    }
}

/// Render a toast notification
pub fn render_toast(
    toast: &Option<crate::tui::remote::error_toast::Toast>,
) -> Option<AnyElement<'static>> {
    toast.as_ref().map(|t| {
        element! {
            View(
                width: 100pct,
                height: 3,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                background_color: Color::Black,
                border_edges: Edges::Top,
                border_style: BorderStyle::Single,
                border_color: t.color(),
            ) {
                Text(content: t.message.clone(), color: t.color())
            }
        }
        .into_any()
    })
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
                background_color: Color::DarkGrey,
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

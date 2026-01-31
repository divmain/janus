//! Sync preview modal for remote TUI
//!
//! Displays sync changes one at a time with navigation between changes.
//! Supports scrolling for long field values that exceed the modal height.

use iocraft::prelude::*;

use crate::tui::components::{
    Clickable, ModalBorderColor, ModalContainer, ModalHeight, ModalOverlay, ModalWidth, TextViewer,
};
use crate::tui::theme::theme;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncChange {
    pub field_name: String,
    pub local_value: String,
    pub remote_value: String,
    pub direction: SyncDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncDirection {
    #[default]
    LocalToRemote,
    RemoteToLocal,
}

/// User decision for a sync change
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncDecision {
    Accept,
    Skip,
}

/// A sync change with its ticket context
#[derive(Debug, Clone)]
pub struct SyncChangeWithContext {
    pub ticket_id: String,
    pub remote_ref: String,
    pub change: SyncChange,
    pub decision: Option<SyncDecision>,
}

#[derive(Clone, Default, Props)]
pub struct SyncPreviewState {
    pub changes: Vec<SyncChangeWithContext>,
    pub current_change_index: usize,
    /// Scroll offset for long content
    pub scroll_offset: usize,
    /// Handler invoked when modal is closed via X button
    pub on_close: Option<Handler<()>>,
    /// Handler invoked when scroll up is requested (mouse wheel)
    pub on_scroll_up: Option<Handler<()>>,
    /// Handler invoked when scroll down is requested (mouse wheel)
    pub on_scroll_down: Option<Handler<()>>,
    /// Handler invoked when Accept button is clicked
    pub on_accept: Option<Handler<()>>,
    /// Handler invoked when Skip button is clicked
    pub on_skip: Option<Handler<()>>,
    /// Handler invoked when Accept All button is clicked
    pub on_accept_all: Option<Handler<()>>,
    /// Handler invoked when Cancel button is clicked
    pub on_cancel: Option<Handler<()>>,
}

impl std::fmt::Debug for SyncPreviewState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyncPreviewState")
            .field("changes", &self.changes)
            .field("current_change_index", &self.current_change_index)
            .field("scroll_offset", &self.scroll_offset)
            .field("on_close", &self.on_close.is_some())
            .field("on_scroll_up", &self.on_scroll_up.is_some())
            .field("on_scroll_down", &self.on_scroll_down.is_some())
            .finish()
    }
}

impl SyncPreviewState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        changes: Vec<SyncChangeWithContext>,
        on_close: Option<Handler<()>>,
        on_scroll_up: Option<Handler<()>>,
        on_scroll_down: Option<Handler<()>>,
        on_accept: Option<Handler<()>>,
        on_skip: Option<Handler<()>>,
        on_accept_all: Option<Handler<()>>,
        on_cancel: Option<Handler<()>>,
    ) -> Self {
        Self {
            changes,
            current_change_index: 0,
            scroll_offset: 0,
            on_close,
            on_scroll_up,
            on_scroll_down,
            on_accept,
            on_skip,
            on_accept_all,
            on_cancel,
        }
    }

    pub fn current_change(&self) -> Option<&SyncChangeWithContext> {
        self.changes.get(self.current_change_index)
    }

    pub fn current_change_mut(&mut self) -> Option<&mut SyncChangeWithContext> {
        self.changes.get_mut(self.current_change_index)
    }

    /// Accept the current change and advance
    pub fn accept_current(&mut self) -> bool {
        if let Some(change) = self.current_change_mut() {
            change.decision = Some(SyncDecision::Accept);
        }
        self.advance()
    }

    /// Skip the current change and advance
    pub fn skip_current(&mut self) -> bool {
        if let Some(change) = self.current_change_mut() {
            change.decision = Some(SyncDecision::Skip);
        }
        self.advance()
    }

    /// Accept all remaining changes
    pub fn accept_all(&mut self) {
        for change in &mut self.changes[self.current_change_index..] {
            change.decision = Some(SyncDecision::Accept);
        }
        self.current_change_index = self.changes.len();
    }

    /// Advance to the next change
    pub fn advance(&mut self) -> bool {
        if self.current_change_index < self.changes.len() {
            self.current_change_index += 1;
            self.current_change_index < self.changes.len()
        } else {
            false
        }
    }

    pub fn has_more(&self) -> bool {
        self.current_change_index < self.changes.len()
    }

    /// Get all accepted changes
    pub fn accepted_changes(&self) -> Vec<&SyncChangeWithContext> {
        self.changes
            .iter()
            .filter(|c| c.decision == Some(SyncDecision::Accept))
            .collect()
    }
}

/// Sync preview modal component
///
/// Displays sync changes one at a time with navigation between changes.
/// Supports mouse wheel scrolling for long field values.
/// Features clickable buttons for Accept, Skip, Accept All, and Cancel actions.
/// Sync preview modal component
///
/// Displays sync changes one at a time with navigation between changes.
/// Supports mouse wheel scrolling for long field values.
/// Features clickable buttons for Accept, Skip, Accept All, and Cancel actions.
#[component]
pub fn SyncPreview(props: &SyncPreviewState, _hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = theme();
    let current_idx = props.current_change_index;
    let total = props.changes.len();
    let scroll_offset = props.scroll_offset;
    let has_changes = props.current_change_index < total;

    // Clone handlers
    let on_close = props.on_close.clone();
    let scroll_up = props.on_scroll_up.clone();
    let scroll_down = props.on_scroll_down.clone();
    let on_accept = props.on_accept.clone();
    let on_skip = props.on_skip.clone();
    let on_accept_all = props.on_accept_all.clone();
    let on_cancel = props.on_cancel.clone();

    // Extract all data from props before using in element! macro to avoid borrow issues
    let content_text: Option<String> = if has_changes {
        let change_ctx = &props.changes[current_idx];
        Some(format!(
            "Ticket: {} | Field: {}\n\n---\n\nLocal:\n{}\n\nRemote:\n{}",
            change_ctx.ticket_id,
            change_ctx.change.field_name,
            change_ctx.change.local_value,
            change_ctx.change.remote_value
        ))
    } else {
        None
    };

    element! {
        ModalOverlay() {
            ModalContainer(
                width: Some(ModalWidth::Fixed(80)),
                height: Some(ModalHeight::Fixed(22)),
                border_color: Some(ModalBorderColor::Info),
                title: Some("Sync Preview".to_string()),
                footer_text: None,
                on_close,
            ) {
                // Progress counter
                Text(
                    content: format!("Change {}/{}", current_idx + 1, total),
                    color: theme.text_dimmed,
                )
                Text(content: "")

                // Content area
                #(if let Some(text) = content_text {
                    Some(element! {
                        View(flex_grow: 1.0, width: 100pct) {
                            Clickable(
                                on_scroll_up: scroll_up,
                                on_scroll_down: scroll_down,
                            ) {
                                TextViewer(
                                    text: text,
                                    scroll_offset: scroll_offset,
                                    has_focus: true,
                                    placeholder: None,
                                )
                            }
                        }
                    })
                } else {
                    Some(element! {
                        View(
                            flex_direction: FlexDirection::Column,
                            width: 100pct,
                            align_items: AlignItems::Center,
                            flex_grow: 1.0,
                        ) {
                            Text(content: "")
                            Text(content: "No changes found", color: Color::Green, weight: Weight::Bold)
                            Text(content: "")
                            Text(content: "All items are in sync.", color: theme.text_dimmed)
                        }
                    })
                })

                // Button row at the bottom
                View(
                    width: 100pct,
                    padding_top: 1,
                    border_edges: Edges::Top,
                    border_style: BorderStyle::Single,
                    border_color: theme.border,
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::Center,
                    gap: 2,
                ) {
                    // Per-change action buttons (Accept/Skip) - only when there are changes
                    #(if has_changes {
                        if let (Some(accept_h), Some(skip_h)) = (on_accept.clone(), on_skip.clone()) {
                            Some(element! {
                                View(flex_direction: FlexDirection::Row, gap: 1) {
                                    // Accept button - green for "apply this change"
                                    Button(
                                        handler: move |_| { accept_h(()); },
                                        has_focus: false,
                                    ) {
                                        View(
                                            border_style: BorderStyle::Round,
                                            border_color: Color::Green,
                                            padding_left: 1,
                                            padding_right: 1,
                                            background_color: Color::Green,
                                        ) {
                                            Text(
                                                content: "[y] Accept",
                                                color: Color::Black,
                                                weight: Weight::Bold,
                                            )
                                        }
                                    }

                                    // Skip button - yellow/amber for "skip this change"
                                    Button(
                                        handler: move |_| { skip_h(()); },
                                        has_focus: false,
                                    ) {
                                        View(
                                            border_style: BorderStyle::Round,
                                            border_color: Color::Yellow,
                                            padding_left: 1,
                                            padding_right: 1,
                                            background_color: Color::Yellow,
                                        ) {
                                            Text(
                                                content: "[n] Skip",
                                                color: Color::Black,
                                                weight: Weight::Bold,
                                            )
                                        }
                                    }
                                }
                            })
                        } else {
                            None
                        }
                    } else {
                        None
                    })

                    // Global action buttons (Accept All/Cancel)
                    View(flex_direction: FlexDirection::Row, gap: 1) {
                        // Accept All button - bold blue for global action (only when there are changes)
                        #(if has_changes {
                            on_accept_all.clone().map(|handler| {
                                element! {
                                    Button(
                                        handler: move |_| { handler(()); },
                                        has_focus: false,
                                    ) {
                                        View(
                                            border_style: BorderStyle::Round,
                                            border_color: Color::Blue,
                                            padding_left: 1,
                                            padding_right: 1,
                                            background_color: Color::Blue,
                                        ) {
                                            Text(
                                                content: "[a] Accept All",
                                                color: Color::White,
                                                weight: Weight::Bold,
                                            )
                                        }
                                    }
                                }
                            })
                        } else {
                            None
                        })

                        // Cancel button - gray for cancel/close (always visible)
                        #(on_cancel.clone().map(|handler| {
                            element! {
                                Button(
                                    handler: move |_| { handler(()); },
                                    has_focus: false,
                                ) {
                                    View(
                                        border_style: BorderStyle::Round,
                                        border_color: Color::Grey,
                                        padding_left: 1,
                                        padding_right: 1,
                                        background_color: Color::Grey,
                                    ) {
                                        Text(
                                            content: "[c] Cancel",
                                            color: Color::White,
                                            weight: Weight::Bold,
                                        )
                                    }
                                }
                            }
                        }))
                    }
                }
            }
        }
    }
}

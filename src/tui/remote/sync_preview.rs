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
    pub fn new(
        changes: Vec<SyncChangeWithContext>,
        on_close: Option<Handler<()>>,
        on_scroll_up: Option<Handler<()>>,
        on_scroll_down: Option<Handler<()>>,
    ) -> Self {
        Self {
            changes,
            current_change_index: 0,
            scroll_offset: 0,
            on_close,
            on_scroll_up,
            on_scroll_down,
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
#[component]
pub fn SyncPreview<'a>(props: &SyncPreviewState, _hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let theme = theme();
    let current_idx = props.current_change_index;
    let total = props.changes.len();
    let scroll_offset = props.scroll_offset;

    // Build footer text based on current state
    let footer_text = if props.current_change().is_some() {
        match props.current_change().unwrap().change.direction {
            SyncDirection::LocalToRemote => "[y] local->remote | [n] skip | [a] all | [c] cancel",
            SyncDirection::RemoteToLocal => "[y] remote->local | [n] skip | [a] all | [c] cancel",
        }
    } else {
        "[c] to close"
    };

    element! {
        ModalOverlay() {
            ModalContainer(
                width: Some(ModalWidth::Fixed(80)),
                height: Some(ModalHeight::Fixed(20)),
                border_color: Some(ModalBorderColor::Info),
                title: Some("Sync Preview".to_string()),
                footer_text: Some(footer_text.to_string()),
                on_close: props.on_close.clone(),
            ) {
                // Progress counter
                Text(
                    content: format!("Change {}/{}", current_idx + 1, total),
                    color: theme.text_dimmed,
                )
                Text(content: "")

                // Content based on current change with scroll support
                #(if let Some(change_ctx) = props.current_change() {
                    let change = &change_ctx.change;
                    let content_text = format!(
                        "Ticket: {} | Field: {}\n\n---\n\nLocal:\n{}\n\nRemote:\n{}",
                        change_ctx.ticket_id,
                        change.field_name,
                        change.local_value,
                        change.remote_value
                    );
                    Some(element! {
                        View(
                            flex_direction: FlexDirection::Column,
                            width: 100pct,
                            flex_grow: 1.0,
                            overflow: Overflow::Hidden,
                        ) {
                            // Scrollable content using TextViewer with mouse wheel support
                            Clickable(
                                on_scroll_up: props.on_scroll_up.clone(),
                                on_scroll_down: props.on_scroll_down.clone(),
                            ) {
                                TextViewer(
                                    text: content_text,
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
            }
        }
    }
}

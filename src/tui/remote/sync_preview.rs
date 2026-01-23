//! Sync preview modal for remote TUI

use iocraft::prelude::*;

use crate::tui::components::{
    ModalBorderColor, ModalContainer, ModalHeight, ModalOverlay, ModalWidth,
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

#[derive(Debug, Clone, Default, Props)]
pub struct SyncPreviewState {
    pub changes: Vec<SyncChangeWithContext>,
    pub current_change_index: usize,
}

impl SyncPreviewState {
    pub fn new(changes: Vec<SyncChangeWithContext>) -> Self {
        Self {
            changes,
            current_change_index: 0,
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
#[component]
pub fn SyncPreview<'a>(props: &SyncPreviewState, _hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let theme = theme();
    let current_idx = props.current_change_index;
    let total = props.changes.len();

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
            ) {
                // Progress counter
                Text(
                    content: format!("Change {}/{}", current_idx + 1, total),
                    color: theme.text_dimmed,
                )
                Text(content: "")

                // Content based on current change
                #(if let Some(change_ctx) = props.current_change() {
                    let change = &change_ctx.change;
                    Some(element! {
                        View(
                            flex_direction: FlexDirection::Column,
                            width: 100pct,
                        ) {
                            // Ticket and field info
                            Text(
                                content: format!("Ticket: {} | Field: {}", change_ctx.ticket_id, change.field_name),
                                color: Color::Yellow,
                                weight: Weight::Bold,
                            )
                            Text(content: "")

                            // Separator
                            View(
                                width: 100pct,
                                border_edges: Edges::Bottom,
                                border_style: BorderStyle::Single,
                                border_color: theme.border,
                            ) {
                                Text(content: "")
                            }
                            Text(content: "")

                            // Local value
                            Text(content: "Local:", color: Color::Green, weight: Weight::Bold)
                            Text(content: change.local_value.clone(), color: theme.text_dimmed)
                            Text(content: "")

                            // Remote value
                            Text(content: "Remote:", color: Color::Red, weight: Weight::Bold)
                            Text(content: change.remote_value.clone(), color: theme.text_dimmed)
                        }
                    })
                } else {
                    Some(element! {
                        View(
                            flex_direction: FlexDirection::Column,
                            width: 100pct,
                            align_items: AlignItems::Center,
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

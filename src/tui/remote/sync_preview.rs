//! Sync preview modal for remote TUI

use iocraft::prelude::*;

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

#[derive(Debug, Clone, Props)]
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
    let current_idx = props.current_change_index;
    let total = props.changes.len();

    element! {
        View(
            width: 100pct,
            height: 100pct,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            background_color: Color::Black,
        ) {
            View(
                width: 80,
                height: 20,
                border_style: BorderStyle::Double,
                border_color: Color::Cyan,
                padding: 1,
                flex_direction: FlexDirection::Column,
                background_color: Color::DarkGrey,
            ) {
                Text(
                    content: "Sync Preview",
                    color: Color::Cyan,
                    weight: Weight::Bold,
                )
                Text(content: format!("{}/{}", current_idx + 1, total), color: Color::DarkGrey)

                #(if let Some(change_ctx) = props.current_change() {
                    let change = &change_ctx.change;
                    Some(element! {
                        View(
                            flex_direction: FlexDirection::Column,
                        ) {
                            Text(content: "")
                            Text(
                                content: format!("Ticket: {} | Field: {}", change_ctx.ticket_id, change.field_name),
                                color: Color::Yellow,
                                weight: Weight::Bold,
                            )
                            Text(content: "")

                            View(border_edges: Edges::Bottom, border_style: BorderStyle::Single, border_color: Color::DarkGrey) {
                                Text(content: "")
                            }

                            Text(content: "")

                            Text(content: "Local:", color: Color::Green)
                            Text(content: change.local_value.clone(), color: Color::DarkGrey)

                            Text(content: "")
                            Text(content: "Remote:", color: Color::Red)
                            Text(content: change.remote_value.clone(), color: Color::DarkGrey)

                            Text(content: "")
                            Text(content: "")

                            Text(
                                content: match change.direction {
                                    SyncDirection::LocalToRemote => "[y] local->remote / [n] skip / [a] all / [c] cancel",
                                    SyncDirection::RemoteToLocal => "[y] remote->local / [n] skip / [a] all / [c] cancel",
                                },
                                color: Color::Cyan,
                            )
                        }
                    })
                } else {
                    Some(element! {
                        View(
                            flex_direction: FlexDirection::Column,
                        ) {
                            Text(content: "")
                            Text(content: "No changes found", color: Color::Green)
                            Text(content: "")
                            Text(content: "[c] to close", color: Color::Cyan)
                        }
                    })
                })
            }
        }
    }
}

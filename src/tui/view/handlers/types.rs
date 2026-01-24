//! Types for view handler actions
//!
//! This module defines the action types that handlers can send to the async
//! queue for processing.

use crate::tui::action_queue::{Action, ActionResult, TicketMetadata as QueueTicketMetadata};
use std::pin::Pin;

use crate::tui::services::TicketService;

/// Actions that can be sent to the async processing queue
#[derive(Debug, Clone)]
pub enum ViewAction {
    /// Cycle a ticket's status to the next value
    CycleStatus {
        /// Ticket ID to update
        id: String,
    },
    /// Load a ticket for editing
    LoadForEdit {
        /// Ticket ID to load
        id: String,
    },
    /// Mark a ticket as triaged
    MarkTriaged {
        /// Ticket ID to triage
        id: String,
        /// Whether to mark as triaged (true) or not triaged (false)
        triaged: bool,
    },
    /// Cancel a ticket
    CancelTicket {
        /// Ticket ID to cancel
        id: String,
    },
    /// Add a note to a ticket
    AddNote {
        /// Ticket ID to add note to
        id: String,
        /// The note text to add
        note: String,
    },
}

impl Action for ViewAction {
    fn execute(self) -> Pin<Box<dyn std::future::Future<Output = ActionResult> + Send>> {
        Box::pin(async move {
            match self {
                ViewAction::CycleStatus { id } => match TicketService::cycle_status(&id).await {
                    Ok(_) => ActionResult::Result {
                        success: true,
                        message: Some(format!("Status cycled for {}", id)),
                    },
                    Err(e) => ActionResult::Result {
                        success: false,
                        message: Some(format!("Failed to cycle status: {}", e)),
                    },
                },
                ViewAction::LoadForEdit { id } => match TicketService::load_for_edit(&id).await {
                    Ok((metadata, body)) => {
                        let queue_metadata = QueueTicketMetadata {
                            id: metadata.id.clone(),
                            uuid: metadata.uuid,
                            title: metadata.title.clone(),
                            status: metadata.status,
                            ticket_type: metadata.ticket_type,
                            priority: metadata.priority,
                            triaged: metadata.triaged,
                            created: metadata.created,
                            file_path: metadata.file_path.map(|p| p.to_string_lossy().to_string()),
                            deps: metadata.deps,
                            links: metadata.links,
                            external_ref: metadata.external_ref,
                            remote: metadata.remote,
                            parent: metadata.parent,
                            spawned_from: metadata.spawned_from,
                            spawn_context: metadata.spawn_context,
                            depth: metadata.depth,
                            completion_summary: metadata.completion_summary,
                        };
                        ActionResult::LoadForEdit {
                            success: true,
                            message: Some(format!("Loaded {} for editing", id)),
                            id: id.clone(),
                            metadata: Box::new(queue_metadata),
                            body,
                        }
                    }
                    Err(e) => ActionResult::Result {
                        success: false,
                        message: Some(format!("Failed to load ticket: {}", e)),
                    },
                },
                ViewAction::MarkTriaged { id, triaged } => {
                    match TicketService::mark_triaged(&id, triaged).await {
                        Ok(_) => ActionResult::Result {
                            success: true,
                            message: Some(if triaged {
                                format!("Marked {} as triaged", id)
                            } else {
                                format!("Unmarked {} as triaged", id)
                            }),
                        },
                        Err(e) => ActionResult::Result {
                            success: false,
                            message: Some(format!("Failed to mark as triaged: {}", e)),
                        },
                    }
                }
                ViewAction::CancelTicket { id } => {
                    match TicketService::set_status(&id, crate::types::TicketStatus::Cancelled)
                        .await
                    {
                        Ok(_) => ActionResult::Result {
                            success: true,
                            message: Some(format!("Cancelled {}", id)),
                        },
                        Err(e) => ActionResult::Result {
                            success: false,
                            message: Some(format!("Failed to cancel ticket: {}", e)),
                        },
                    }
                }
                ViewAction::AddNote { id, note } => {
                    match TicketService::add_note(&id, &note).await {
                        Ok(_) => ActionResult::Result {
                            success: true,
                            message: Some(format!("Added note to {}", id)),
                        },
                        Err(e) => ActionResult::Result {
                            success: false,
                            message: Some(format!("Failed to add note: {}", e)),
                        },
                    }
                }
            }
        })
    }
}

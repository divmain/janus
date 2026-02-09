//! Ticket repository module.
//!
//! This module provides functions for querying and retrieving tickets.
//! All functions are async and use the in-memory store.

use crate::TicketMetadata;
use crate::cache::get_or_init_store;
use crate::ticket::parse_ticket;
use crate::types::LoadResult;
use crate::utils::DirScanner;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Result of loading tickets from disk, including both successes and failures
pub type TicketLoadResult = LoadResult<TicketMetadata>;

impl TicketLoadResult {
    /// Add a successfully loaded ticket
    pub fn add_ticket(&mut self, ticket: TicketMetadata) {
        self.items.push(ticket);
    }

    /// Convert to a Result, returning Err if there are failures
    pub fn into_result(self) -> crate::error::Result<Vec<TicketMetadata>> {
        if self.has_failures() {
            let failure_msgs: Vec<String> = self
                .failed
                .iter()
                .map(|(f, e)| format!("  - {f}: {e}"))
                .collect();
            Err(crate::error::JanusError::TicketLoadFailed(failure_msgs))
        } else {
            Ok(self.items)
        }
    }

    /// Get just the tickets, ignoring failures
    pub fn into_tickets(self) -> Vec<TicketMetadata> {
        self.items
    }
}

/// Find all ticket files in the tickets directory
pub fn find_tickets() -> Result<Vec<String>, std::io::Error> {
    use crate::types::tickets_items_dir;

    DirScanner::find_markdown_files(tickets_items_dir())
}

/// Get all tickets from the in-memory store.
///
/// Returns a `TicketLoadResult` containing all loaded tickets.
/// The store is populated from disk on first access and kept
/// up-to-date via the filesystem watcher.
pub async fn get_all_tickets() -> Result<TicketLoadResult, crate::error::JanusError> {
    match get_or_init_store().await {
        Ok(store) => {
            let tickets = store.get_all_tickets();
            let mut result = TicketLoadResult::new();
            for ticket in tickets {
                result.add_ticket(ticket);
            }
            Ok(result)
        }
        Err(_) => {
            // Fall back to disk reads if store initialization fails
            Ok(get_all_tickets_from_disk())
        }
    }
}

/// Get all tickets from disk (fallback when store is unavailable)
///
/// Returns a `TicketLoadResult` containing both successfully loaded tickets
/// and any failures that occurred during loading.
pub fn get_all_tickets_from_disk() -> TicketLoadResult {
    use crate::types::tickets_items_dir;

    let files = match find_tickets() {
        Ok(files) => files,
        Err(e) => {
            let mut result = TicketLoadResult::new();
            result.add_failure(
                "<directory>",
                format!("failed to read tickets directory: {e}"),
            );
            return result;
        }
    };

    let mut result = TicketLoadResult::new();
    let items_dir = tickets_items_dir();

    for file in files {
        let file_path = items_dir.join(&file);
        match fs::read_to_string(&file_path) {
            Ok(content_str) => match parse_ticket(&content_str) {
                Ok(mut metadata) => {
                    metadata.id = Some(crate::types::TicketId::new_unchecked(
                        file.strip_suffix(".md").unwrap_or(&file),
                    ));
                    metadata.file_path = Some(file_path);
                    result.add_ticket(metadata);
                }
                Err(e) => {
                    result.add_failure(&file, format!("parse error: {e}"));
                }
            },
            Err(e) => {
                result.add_failure(&file, format!("read error: {e}"));
            }
        }
    }

    result
}

/// Build a HashMap by ID from all tickets
pub async fn build_ticket_map() -> Result<HashMap<String, TicketMetadata>, crate::error::JanusError>
{
    match get_or_init_store().await {
        Ok(store) => Ok(store.build_ticket_map()),
        Err(_) => {
            // Fall back to disk reads if store initialization fails
            let result = get_all_tickets_from_disk();
            let map = result
                .items
                .into_iter()
                .filter_map(|m| m.id.clone().map(|id| (id.to_string(), m)))
                .collect();
            Ok(map)
        }
    }
}

/// Get all tickets and the map together (efficient single call)
pub async fn get_all_tickets_with_map()
-> Result<(Vec<TicketMetadata>, HashMap<String, TicketMetadata>), crate::error::JanusError> {
    let result = get_all_tickets().await?;
    let map: HashMap<_, _> = result
        .items
        .iter()
        .filter_map(|m| m.id.clone().map(|id| (id.to_string(), m.clone())))
        .collect();
    Ok((result.items, map))
}

/// Get file modification time
pub fn get_file_mtime(path: &Path) -> Option<std::time::SystemTime> {
    DirScanner::get_file_mtime(path)
}

/// Get the count of tickets spawned from a given ticket.
pub async fn get_children_count(ticket_id: &str) -> Result<usize, crate::error::JanusError> {
    match get_or_init_store().await {
        Ok(store) => Ok(store.get_children_count(ticket_id)),
        Err(_) => Ok(0),
    }
}

/// Get the count of children for all tickets that have spawned children.
///
/// Returns a HashMap mapping parent ticket IDs to their children count.
pub async fn get_all_children_counts() -> Result<HashMap<String, usize>, crate::error::JanusError> {
    match get_or_init_store().await {
        Ok(store) => Ok(store.get_all_children_counts()),
        Err(_) => Ok(HashMap::new()),
    }
}

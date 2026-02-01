//! Ticket repository module.
//!
//! This module provides functions for querying and retrieving tickets.
//! All functions are async and support caching when available.

use crate::ticket::content;
use crate::utils::DirScanner;
use crate::{TicketMetadata, cache};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tokio::fs as tokio_fs;

/// Result of loading tickets from disk, including both successes and failures
#[derive(Debug, Clone)]
pub struct TicketLoadResult {
    /// Successfully loaded tickets
    pub tickets: Vec<TicketMetadata>,
    /// Failed files with their error messages (filename, error)
    pub failed: Vec<(String, String)>,
}

impl TicketLoadResult {
    /// Create a new empty result
    pub fn new() -> Self {
        TicketLoadResult {
            tickets: Vec::new(),
            failed: Vec::new(),
        }
    }

    /// Add a successfully loaded ticket
    pub fn add_ticket(&mut self, ticket: TicketMetadata) {
        self.tickets.push(ticket);
    }

    /// Add a failed file with its error
    pub fn add_failure(&mut self, filename: impl Into<String>, error: impl Into<String>) {
        self.failed.push((filename.into(), error.into()));
    }

    /// Check if any failures occurred
    pub fn has_failures(&self) -> bool {
        !self.failed.is_empty()
    }

    /// Get the number of successfully loaded tickets
    pub fn success_count(&self) -> usize {
        self.tickets.len()
    }

    /// Get the number of failed files
    pub fn failure_count(&self) -> usize {
        self.failed.len()
    }

    /// Convert to a Result, returning Err if there are failures
    pub fn into_result(self) -> crate::error::Result<Vec<TicketMetadata>> {
        if self.has_failures() {
            let failure_msgs: Vec<String> = self
                .failed
                .iter()
                .map(|(f, e)| format!("  - {}: {}", f, e))
                .collect();
            Err(crate::error::JanusError::TicketLoadFailed(failure_msgs))
        } else {
            Ok(self.tickets)
        }
    }

    /// Get just the tickets, ignoring failures
    pub fn into_tickets(self) -> Vec<TicketMetadata> {
        self.tickets
    }
}

impl Default for TicketLoadResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Find all ticket files in the tickets directory
pub fn find_tickets() -> Result<Vec<String>, std::io::Error> {
    use crate::types::tickets_items_dir;

    DirScanner::find_markdown_files(tickets_items_dir())
}

/// Get all tickets from cache or disk
///
/// Returns a `TicketLoadResult` containing both successfully loaded tickets
/// and any failures that occurred during loading.
///
/// Uses mtime comparison to avoid unnecessary file reads - only re-reads
/// files that have been modified since they were cached.
pub async fn get_all_tickets() -> Result<TicketLoadResult, crate::error::JanusError> {
    if let Some(cache) = cache::get_or_init_cache().await {
        if let Ok(cached_tickets) = cache.get_all_tickets().await {
            let mut result = TicketLoadResult::new();
            let items_dir = crate::types::tickets_items_dir();

            // Scan directory to get current file mtimes
            let disk_files = match cache::TicketCache::scan_directory_static(&items_dir) {
                Ok(files) => files,
                Err(e) => {
                    eprintln!(
                        "Warning: failed to scan tickets directory: {}. Falling back to file reads.",
                        e
                    );
                    return Ok(get_all_tickets_from_disk());
                }
            };

            // Get cached mtimes for comparison
            let cached_mtimes = match cache.get_cached_mtimes().await {
                Ok(mtimes) => mtimes,
                Err(e) => {
                    eprintln!(
                        "Warning: failed to get cached mtimes: {}. Falling back to file reads.",
                        e
                    );
                    return Ok(get_all_tickets_from_disk());
                }
            };

            // Build a map of cached tickets by ID for quick lookup
            let cached_map: std::collections::HashMap<_, _> = cached_tickets
                .into_iter()
                .filter_map(|t| t.id.clone().map(|id| (id, t)))
                .collect();

            // Process each file on disk
            for (id, disk_mtime) in disk_files {
                let file_path = items_dir.join(format!("{}.md", id));

                // Check if file needs re-reading (not in cache or mtime differs)
                let needs_reread = match cached_mtimes.get(&id) {
                    Some(&cached_mtime) => disk_mtime != cached_mtime,
                    None => true, // Not in cache, must read
                };

                if needs_reread {
                    // File modified or not cached - read from disk
                    match tokio_fs::read_to_string(&file_path).await {
                        Ok(content) => match content::parse(&content) {
                            Ok(mut metadata) => {
                                if metadata.id.is_none() {
                                    metadata.id = Some(id.clone());
                                }
                                metadata.file_path = Some(file_path);
                                result.add_ticket(metadata);
                            }
                            Err(e) => {
                                result.add_failure(
                                    file_path.to_string_lossy().into_owned(),
                                    e.to_string(),
                                );
                            }
                        },
                        Err(e) => {
                            result.add_failure(
                                file_path.to_string_lossy().into_owned(),
                                e.to_string(),
                            );
                        }
                    }
                } else if let Some(cached_ticket) = cached_map.get(&id) {
                    // File unchanged - use cached data, just update file_path
                    let mut metadata = cached_ticket.clone();
                    metadata.file_path = Some(file_path);
                    result.add_ticket(metadata);
                }
            }

            if result.success_count() == 0 && result.failure_count() > 0 {
                eprintln!("Warning: cache read had failures, falling back to file reads");
                return Ok(get_all_tickets_from_disk());
            }
            return Ok(result);
        }
        eprintln!("Warning: cache read failed, falling back to file reads");
    }

    Ok(get_all_tickets_from_disk())
}

/// Get all tickets from disk (fallback when cache is unavailable)
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
                format!("failed to read tickets directory: {}", e),
            );
            return result;
        }
    };

    let mut result = TicketLoadResult::new();
    let items_dir = tickets_items_dir();

    for file in files {
        let file_path = items_dir.join(&file);
        match fs::read_to_string(&file_path) {
            Ok(content_str) => match content::parse(&content_str) {
                Ok(mut metadata) => {
                    metadata.id = Some(file.strip_suffix(".md").unwrap_or(&file).to_string());
                    metadata.file_path = Some(file_path);
                    result.add_ticket(metadata);
                }
                Err(e) => {
                    result.add_failure(&file, format!("parse error: {}", e));
                }
            },
            Err(e) => {
                result.add_failure(&file, format!("read error: {}", e));
            }
        }
    }

    result
}

/// Build a HashMap by ID from all tickets
pub async fn build_ticket_map() -> Result<HashMap<String, TicketMetadata>, crate::error::JanusError>
{
    if let Some(cache) = cache::get_or_init_cache().await {
        if let Ok(map) = cache.build_ticket_map().await {
            return Ok(map);
        }
        eprintln!("Warning: cache read failed, falling back to file reads");
    }

    // Fallback: get all tickets and build map
    let result = get_all_tickets().await?;
    let map: HashMap<_, _> = result
        .tickets
        .into_iter()
        .filter_map(|m| m.id.clone().map(|id| (id, m)))
        .collect();
    Ok(map)
}

/// Get all tickets and the map together (efficient single call)
pub async fn get_all_tickets_with_map()
-> Result<(Vec<TicketMetadata>, HashMap<String, TicketMetadata>), crate::error::JanusError> {
    let result = get_all_tickets().await?;
    let map: HashMap<_, _> = result
        .tickets
        .iter()
        .filter_map(|m| m.id.clone().map(|id| (id, m.clone())))
        .collect();
    Ok((result.tickets, map))
}

/// Get file modification time
pub fn get_file_mtime(path: &Path) -> Option<std::time::SystemTime> {
    DirScanner::get_file_mtime(path)
}

/// Get the count of tickets spawned from a given ticket.
///
/// This function uses the cache when available, falling back to
/// scanning all tickets and counting matches.
pub async fn get_children_count(ticket_id: &str) -> Result<usize, crate::error::JanusError> {
    if let Some(cache) = cache::get_or_init_cache().await {
        if let Ok(count) = cache.get_children_count(ticket_id).await {
            return Ok(count);
        }
        eprintln!("Warning: cache read failed, falling back to file reads");
    }

    // Fallback: scan all tickets and count matches
    let result = get_all_tickets().await?;
    Ok(result
        .tickets
        .iter()
        .filter(|t| t.spawned_from.as_ref() == Some(&ticket_id.to_string()))
        .count())
}

/// Get the count of children for all tickets that have spawned children.
///
/// This performs a single GROUP BY query instead of N individual queries.
/// Returns a HashMap mapping parent ticket IDs to their children count.
pub async fn get_all_children_counts() -> Result<HashMap<String, usize>, crate::error::JanusError> {
    if let Some(cache) = cache::get_or_init_cache().await {
        if let Ok(counts) = cache.get_all_children_counts().await {
            return Ok(counts);
        }
        eprintln!("Warning: cache read failed, falling back to file reads");
    }

    // Fallback: scan all tickets and build counts map
    let result = get_all_tickets().await?;
    let mut counts: HashMap<String, usize> = HashMap::new();
    for ticket in &result.tickets {
        if let Some(parent_id) = &ticket.spawned_from {
            *counts.entry(parent_id.clone()).or_insert(0) += 1;
        }
    }
    Ok(counts)
}

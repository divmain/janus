//! Ticket repository module.
//!
//! This module provides functions for querying and retrieving tickets.
//! All functions are async and support caching when available.

use crate::ticket::content;
use crate::types::LoadResult;
use crate::utils::DirScanner;
use crate::{TicketMetadata, cache};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tokio::fs as tokio_fs;

/// Action to take when cache validation failures are detected
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CacheRecoveryAction {
    /// No action needed - validation was successful
    NoAction,
    /// Repair individual cache entries
    RepairIndividual,
    /// Force a full cache rebuild
    ForceRebuild,
}

/// Scan the tickets directory to get current file mtimes.
///
/// Returns a HashMap mapping ticket IDs to their modification times.
fn scan_disk_files(items_dir: &Path) -> crate::error::Result<HashMap<String, i64>> {
    cache::TicketCache::scan_directory_static(items_dir).map_err(|e| {
        crate::error::JanusError::CacheAccessFailed(items_dir.to_path_buf(), e.to_string())
    })
}

/// Read and parse a single ticket from disk.
///
/// Reads the file content and parses it into TicketMetadata.
/// The ticket ID is set from the provided ID if not already present in the metadata.
async fn read_ticket_from_disk(id: &str, file_path: &Path) -> crate::error::Result<TicketMetadata> {
    let content = tokio_fs::read_to_string(file_path).await.map_err(|e| {
        crate::error::JanusError::StorageError {
            operation: "read",
            item_type: "ticket",
            path: file_path.to_path_buf(),
            source: e,
        }
    })?;

    let mut metadata = content::parse(&content)
        .map_err(|e| crate::error::JanusError::InvalidFormat(e.to_string()))?;

    if metadata.id.is_none() {
        metadata.id = Some(id.to_string());
    }
    metadata.file_path = Some(file_path.to_path_buf());

    Ok(metadata)
}

/// Compare disk mtimes with cached mtimes to determine which files need re-reading.
///
/// Returns a tuple of (needs_reread, unchanged) where:
/// - needs_reread: IDs that have been modified or are not in cache
/// - unchanged: IDs that are unchanged and can use cached data
fn compute_cache_diff(
    disk_files: &HashMap<String, i64>,
    cached_mtimes: &HashMap<String, i64>,
) -> (Vec<String>, Vec<String>) {
    let mut needs_reread = Vec::new();
    let mut unchanged = Vec::new();

    for (id, disk_mtime) in disk_files {
        match cached_mtimes.get(id) {
            Some(&cached_mtime) if disk_mtime == &cached_mtime => {
                unchanged.push(id.clone());
            }
            _ => {
                needs_reread.push(id.clone());
            }
        }
    }

    (needs_reread, unchanged)
}

/// Handle cache validation failures and determine recovery action.
///
/// If failure rate exceeds 10%, returns ForceRebuild.
/// Otherwise, if there are tickets to repair, returns RepairIndividual.
fn handle_validation_failures(
    failures: usize,
    total: usize,
    tickets_to_repair: &[TicketMetadata],
) -> CacheRecoveryAction {
    if total == 0 {
        return CacheRecoveryAction::NoAction;
    }

    let failure_rate = (failures as f64 / total as f64) * 100.0;

    if failure_rate > 10.0 {
        CacheRecoveryAction::ForceRebuild
    } else if !tickets_to_repair.is_empty() {
        CacheRecoveryAction::RepairIndividual
    } else {
        CacheRecoveryAction::NoAction
    }
}

/// Repair cache entries based on validation results.
///
/// Either repairs individual cache entries or triggers full rebuild based on failure rate.
async fn repair_cache_entries(
    cache: &cache::TicketCache,
    tickets: &[TicketMetadata],
    validation_failures: usize,
    total_validated: usize,
) -> crate::error::Result<()> {
    let action = handle_validation_failures(validation_failures, total_validated, tickets);

    match action {
        CacheRecoveryAction::ForceRebuild => {
            eprintln!(
                "Warning: {} validation failures out of {} files exceeds threshold. Triggering cache rebuild...",
                validation_failures, total_validated
            );
            if let Err(e) = cache.force_rebuild_tickets().await {
                eprintln!(
                    "Warning: cache rebuild failed: {}. Continuing with disk data.",
                    e
                );
            }
        }
        CacheRecoveryAction::RepairIndividual => {
            for ticket in tickets {
                if let Err(e) = cache.update_ticket(ticket).await {
                    eprintln!(
                        "Warning: failed to repair cache entry for ticket '{}': {}.",
                        ticket.id.as_deref().unwrap_or("unknown"),
                        e
                    );
                }
            }
        }
        CacheRecoveryAction::NoAction => {}
    }

    Ok(())
}

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
                .map(|(f, e)| format!("  - {}: {}", f, e))
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

/// Get all tickets from cache or disk
///
/// Returns a `TicketLoadResult` containing both successfully loaded tickets
/// and any failures that occurred during loading.
///
/// Uses mtime comparison to avoid unnecessary file reads - only re-reads
/// files that have been modified since they were cached.
///
/// When validation detects stale or corrupted cache data, this function will:
/// 1. Repair individual cache entries for successfully validated tickets
/// 2. Trigger a full cache rebuild if the validation failure rate exceeds 10%
pub async fn get_all_tickets() -> Result<TicketLoadResult, crate::error::JanusError> {
    let cache = match cache::get_or_init_cache().await {
        Some(c) => c,
        None => return Ok(get_all_tickets_from_disk()),
    };

    let cached_tickets = match cache.get_all_tickets().await {
        Ok(t) => t,
        Err(_) => return Ok(get_all_tickets_from_disk()),
    };

    let items_dir = crate::types::tickets_items_dir();

    // Step 1: Scan disk to get current file mtimes
    let disk_files = match scan_disk_files(&items_dir) {
        Ok(files) => files,
        Err(_) => return Ok(get_all_tickets_from_disk()),
    };

    // Step 2: Get cached mtimes for comparison
    let cached_mtimes = match cache.get_cached_mtimes().await {
        Ok(m) => m,
        Err(_) => return Ok(get_all_tickets_from_disk()),
    };

    // Step 3: Compute diff to find files needing re-read
    let (needs_reread, unchanged) = compute_cache_diff(&disk_files, &cached_mtimes);

    // Build cached ticket map for quick lookup
    let cached_map: HashMap<_, _> = cached_tickets
        .into_iter()
        .filter_map(|t| t.id.clone().map(|id| (id, t)))
        .collect();

    let mut result = TicketLoadResult::new();
    let mut tickets_to_repair: Vec<TicketMetadata> = Vec::new();
    let mut validation_failures = 0_usize;

    // Step 4: Process unchanged files from cache
    for id in unchanged {
        if let Some(cached) = cached_map.get(&id) {
            let mut metadata = cached.clone();
            metadata.file_path = Some(items_dir.join(format!("{}.md", id)));
            result.add_ticket(metadata);
        }
    }

    // Step 5: Read modified/new files from disk
    for id in needs_reread {
        let file_path = items_dir.join(format!("{}.md", id));
        match read_ticket_from_disk(&id, &file_path).await {
            Ok(metadata) => {
                tickets_to_repair.push(metadata.clone());
                result.add_ticket(metadata);
            }
            Err(e) => {
                validation_failures += 1;
                result.add_failure(file_path.to_string_lossy().into_owned(), e.to_string());
            }
        }
    }

    // Step 6: Repair cache if needed
    let total_validated = disk_files.len();
    repair_cache_entries(
        &cache,
        &tickets_to_repair,
        validation_failures,
        total_validated,
    )
    .await?;

    // Fallback to disk if all cache reads failed
    if result.success_count() == 0 && result.failure_count() > 0 {
        return Ok(get_all_tickets_from_disk());
    }

    Ok(result)
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
        .items
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
        .items
        .iter()
        .filter_map(|m| m.id.clone().map(|id| (id, m.clone())))
        .collect();
    Ok((result.items, map))
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
        .items
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
    for ticket in &result.items {
        if let Some(parent_id) = &ticket.spawned_from {
            *counts.entry(parent_id.clone()).or_insert(0) += 1;
        }
    }
    Ok(counts)
}

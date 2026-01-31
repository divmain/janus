//! Synchronization logic for keeping the cache up-to-date with disk.
//!
//! This module handles:
//! - Scanning directories for changes (additions, modifications, deletions)
//! - Syncing tickets and plans from disk to cache
//! - Generic sync implementation using the CacheableItem trait

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::SystemTime;

use crate::error::{JanusError as CacheError, Result};
use crate::events::log_cache_rebuilt;
use crate::plan::types::PlanMetadata;
use crate::types::TicketMetadata;

use super::database::TicketCache;
use super::traits::CacheableItem;

#[cfg(feature = "semantic-search")]
use crate::embedding::model::{EMBEDDING_MODEL_NAME, generate_ticket_embedding};

/// Statistics from a sync operation
#[derive(Debug, Default)]
pub struct SyncStats {
    pub added: usize,
    pub modified: usize,
    pub removed: usize,
    pub cache_was_empty: bool,
}

impl SyncStats {
    pub fn total_changes(&self) -> usize {
        self.added + self.modified + self.removed
    }
}

impl TicketCache {
    /// Sync both tickets and plans from disk to cache.
    ///
    /// Returns true if any changes were made, false if cache was already up to date.
    pub async fn sync(&self) -> Result<bool> {
        let start = std::time::Instant::now();
        let (tickets_changed, ticket_stats) = self.sync_tickets_with_stats().await?;
        let (plans_changed, plan_stats) = self.sync_plans_with_stats().await?;
        let duration = start.elapsed();

        // Log if any sync operation occurred that modified the cache
        let total_ticket_changes = ticket_stats.total_changes();
        let total_plan_changes = plan_stats.total_changes();
        let has_changes = tickets_changed || plans_changed;
        let is_initial_sync = ticket_stats.cache_was_empty && total_ticket_changes > 0;

        // Log event for any sync that actually updates the cache
        if has_changes {
            let reason = if is_initial_sync {
                "initial_cache_population"
            } else {
                "incremental_sync"
            };

            let trigger = if is_initial_sync {
                "cache_empty_on_startup"
            } else {
                "mtime_changes_detected"
            };

            log_cache_rebuilt(
                reason,
                trigger,
                Some(duration.as_millis() as u64),
                Some(ticket_stats.added + ticket_stats.modified),
                Some(serde_json::json!({
                    "tickets": {
                        "added": ticket_stats.added,
                        "modified": ticket_stats.modified,
                        "removed": ticket_stats.removed,
                        "cache_was_empty": ticket_stats.cache_was_empty,
                    },
                    "plans": {
                        "added": plan_stats.added,
                        "modified": plan_stats.modified,
                        "removed": plan_stats.removed,
                        "cache_was_empty": plan_stats.cache_was_empty,
                    },
                    "total_changes": total_ticket_changes + total_plan_changes,
                })),
            );
        }

        Ok(tickets_changed || plans_changed)
    }

    /// Sync tickets from disk to cache.
    ///
    /// Returns true if any changes were made, false if cache was already up to date.
    pub async fn sync_tickets(&self) -> Result<bool> {
        let (changed, _) = self.sync_tickets_with_stats().await?;
        Ok(changed)
    }

    /// Sync tickets and return detailed stats.
    async fn sync_tickets_with_stats(&self) -> Result<(bool, SyncStats)> {
        self.sync_items_with_stats::<TicketMetadata>().await
    }

    /// Sync plans from disk to cache.
    ///
    /// Returns true if any changes were made, false if cache was already up to date.
    pub async fn sync_plans(&self) -> Result<bool> {
        let (changed, _) = self.sync_plans_with_stats().await?;
        Ok(changed)
    }

    /// Sync plans and return detailed stats.
    async fn sync_plans_with_stats(&self) -> Result<(bool, SyncStats)> {
        self.sync_items_with_stats::<PlanMetadata>().await
    }

    /// Generic sync implementation that returns detailed statistics.
    async fn sync_items_with_stats<T: CacheableItem>(&self) -> Result<(bool, SyncStats)> {
        let dir = T::directory();

        if !dir.exists() {
            fs::create_dir_all(&dir).map_err(CacheError::Io)?;
            return Ok((false, SyncStats::default()));
        }

        let disk_files = Self::scan_directory_static(&dir)?;
        let cached_mtimes = self.get_cached_mtimes_for::<T>().await?;
        let cache_was_empty = cached_mtimes.is_empty();

        let mut added = Vec::new();
        let mut modified = Vec::new();
        let mut removed = Vec::new();

        for (id, disk_mtime) in &disk_files {
            if let Some(&cache_mtime) = cached_mtimes.get(id) {
                if *disk_mtime != cache_mtime {
                    modified.push(id.clone());
                }
            } else {
                added.push(id.clone());
            }
        }

        for id in cached_mtimes.keys() {
            if !disk_files.contains_key(id) {
                removed.push(id.clone());
            }
        }

        if added.is_empty() && modified.is_empty() && removed.is_empty() {
            return Ok((
                false,
                SyncStats {
                    cache_was_empty,
                    ..Default::default()
                },
            ));
        }

        let stats = SyncStats {
            added: added.len(),
            modified: modified.len(),
            removed: removed.len(),
            cache_was_empty,
        };

        // Read and parse items before starting the transaction
        let mut items_to_upsert = Vec::new();
        for id in &added {
            match T::parse_from_file(id).await {
                Ok((metadata, mtime_ns)) => {
                    items_to_upsert.push((metadata, mtime_ns));
                }
                Err(e) => {
                    eprintln!(
                        "Warning: failed to parse {} '{}': {}. Skipping...",
                        T::item_name(),
                        id,
                        e
                    );
                }
            }
        }
        for id in &modified {
            match T::parse_from_file(id).await {
                Ok((metadata, mtime_ns)) => {
                    items_to_upsert.push((metadata, mtime_ns));
                }
                Err(e) => {
                    eprintln!(
                        "Warning: keeping stale cache entry for {} '{}' due to parse failure: {}",
                        T::item_name(),
                        id,
                        e
                    );
                }
            }
        }

        // Use transaction for atomicity
        let conn = self.create_connection().await?;
        let tx = conn.unchecked_transaction().await?;

        for (metadata, mtime_ns) in &items_to_upsert {
            metadata.insert_into_cache(&tx, *mtime_ns).await?;
        }

        if !removed.is_empty() {
            let placeholders: Vec<String> =
                (1..=removed.len()).map(|i| format!("?{}", i)).collect();
            let placeholders_str = placeholders.join(", ");
            let delete_sql = format!(
                "DELETE FROM {} WHERE {} IN ({})",
                T::table_name(),
                T::id_column(),
                placeholders_str
            );
            let ids: Vec<&str> = removed.iter().map(|id| id.as_str()).collect();
            tx.execute(&delete_sql, ids).await?;
        }

        tx.commit().await?;

        Ok((true, stats))
    }

    /// Scan a directory for .md files and return their IDs and mtimes.
    pub(crate) fn scan_directory_static(dir: &Path) -> Result<HashMap<String, i64>> {
        let mut files = HashMap::new();

        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(files),
            Err(e) => return Err(CacheError::Io(e)),
        };

        for entry in entries {
            let entry = entry.map_err(CacheError::Io)?;
            let path = entry.path();

            if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                continue;
            }

            let metadata = entry.metadata().map_err(CacheError::Io)?;
            let mtime = metadata.modified().map_err(CacheError::Io)?;
            let mtime_ns = mtime
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_err(|e| CacheError::Io(std::io::Error::other(e)))?
                .as_nanos() as i64;

            if let Some(id) = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(|s| s.to_string())
            {
                files.insert(id, mtime_ns);
            }
        }

        Ok(files)
    }

    /// Get cached mtimes for a specific item type.
    async fn get_cached_mtimes_for<T: CacheableItem>(&self) -> Result<HashMap<String, i64>> {
        let mut mtimes = HashMap::new();

        let query = format!(
            "SELECT {}, mtime_ns FROM {}",
            T::id_column(),
            T::table_name()
        );
        let conn = self.create_connection().await?;
        let mut rows = conn.query(&query, ()).await?;

        while let Some(row) = rows.next().await? {
            let id: String = row.get(0)?;
            let mtime: i64 = row.get(1)?;
            mtimes.insert(id, mtime);
        }

        Ok(mtimes)
    }

    /// Check if embeddings need to be regenerated due to model version mismatch.
    ///
    /// This method checks the stored embedding model version in the cache metadata
    /// against the current model version. Returns true if they don't match or if
    /// no model version is stored.
    #[cfg(feature = "semantic-search")]
    pub async fn needs_reembedding(&self) -> Result<bool> {
        let stored_model = self.get_meta("embedding_model").await?;

        match stored_model {
            Some(model) => Ok(model != EMBEDDING_MODEL_NAME),
            None => Ok(true), // No model tracked, needs reembedding
        }
    }

    /// Regenerate embeddings for all tickets in the cache.
    ///
    /// This method is called during cache rebuild when a model version mismatch
    /// is detected. It regenerates embeddings for all tickets and updates the
    /// cache with the new embeddings.
    ///
    /// # Arguments
    /// * `output_json` - If true, suppresses progress output
    #[cfg(feature = "semantic-search")]
    pub async fn regenerate_all_embeddings(&self, output_json: bool) -> Result<()> {
        // Get all tickets
        let tickets = self.get_all_tickets().await?;
        let total = tickets.len();

        if total == 0 {
            if !output_json {
                println!("No tickets to regenerate embeddings for.");
            }
            return Ok(());
        }

        if !output_json {
            println!("Regenerating embeddings for {} tickets...", total);
        }

        let mut success_count = 0;
        let mut error_count = 0;

        for (i, ticket) in tickets.iter().enumerate() {
            let ticket_id = ticket
                .id
                .as_ref()
                .ok_or_else(|| CacheError::CacheDataIntegrity("Ticket missing ID".to_string()))?;

            // Generate embedding for this ticket
            let title = ticket.title.as_deref().unwrap_or("");
            let body = ticket.body.as_deref();

            match generate_ticket_embedding(title, body) {
                Ok(embedding) => {
                    // Update the embedding in the cache
                    if let Err(e) = self.update_ticket_embedding(ticket_id, &embedding).await {
                        eprintln!(
                            "Warning: failed to update embedding for {}: {}",
                            ticket_id, e
                        );
                        error_count += 1;
                    } else {
                        success_count += 1;
                    }
                }
                Err(e) => {
                    eprintln!(
                        "Warning: failed to generate embedding for {}: {}",
                        ticket_id, e
                    );
                    error_count += 1;
                }
            }

            // Show progress every 10 tickets or at end
            if !output_json && ((i + 1) % 10 == 0 || i == total - 1) {
                println!("  Progress: {}/{}", i + 1, total);
            }
        }

        // Update model version in meta table
        let conn = self.create_connection().await?;
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('embedding_model', ?1)",
            (EMBEDDING_MODEL_NAME,),
        )
        .await?;

        if !output_json {
            println!(
                "Embeddings regenerated successfully ({} succeeded, {} failed).",
                success_count, error_count
            );
        }

        Ok(())
    }
}

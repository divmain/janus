//! Synchronization logic for keeping the cache up-to-date with disk.
//!
//! This module handles:
//! - Scanning directories for changes (additions, modifications, deletions)
//! - Syncing tickets and plans from disk to cache
//! - Generic sync implementation using the CacheableItem trait

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::error::{JanusError as CacheError, Result};
use crate::plan::types::PlanMetadata;
use crate::types::TicketMetadata;

use super::database::TicketCache;
use super::traits::CacheableItem;

impl TicketCache {
    /// Sync both tickets and plans from disk to cache.
    ///
    /// Returns true if any changes were made, false if cache was already up to date.
    pub async fn sync(&mut self) -> Result<bool> {
        let tickets_changed = self.sync_tickets().await?;
        let plans_changed = self.sync_plans().await?;
        Ok(tickets_changed || plans_changed)
    }

    /// Sync tickets from disk to cache.
    ///
    /// Returns true if any changes were made, false if cache was already up to date.
    pub async fn sync_tickets(&mut self) -> Result<bool> {
        self.sync_items::<TicketMetadata>().await
    }

    /// Sync plans from disk to cache.
    ///
    /// Returns true if any changes were made, false if cache was already up to date.
    pub async fn sync_plans(&mut self) -> Result<bool> {
        self.sync_items::<PlanMetadata>().await
    }

    /// Generic sync implementation for any CacheableItem type.
    ///
    /// Scans the item's directory, compares mtimes with cached values,
    /// and updates the cache with any changes.
    async fn sync_items<T: CacheableItem>(&mut self) -> Result<bool> {
        let dir = PathBuf::from(T::directory());

        if !dir.exists() {
            fs::create_dir_all(&dir).map_err(CacheError::Io)?;
            return Ok(false);
        }

        let disk_files = Self::scan_directory_static(&dir)?;
        let cached_mtimes = self.get_cached_mtimes_for::<T>().await?;

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
            return Ok(false);
        }

        // Read and parse items before starting the transaction
        let mut items_to_upsert = Vec::new();
        for id in added.iter().chain(modified.iter()) {
            match T::parse_from_file(id) {
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

        // Use transaction for atomicity
        let tx = self.conn.transaction().await?;

        for (metadata, mtime_ns) in &items_to_upsert {
            metadata.insert_into_cache(&tx, *mtime_ns).await?;
        }

        let delete_sql = format!(
            "DELETE FROM {} WHERE {} = ?1",
            T::table_name(),
            T::id_column()
        );
        for id in &removed {
            tx.execute(&delete_sql, [id.as_str()]).await?;
        }

        tx.commit().await?;

        Ok(true)
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
        let mut rows = self.conn.query(&query, ()).await?;

        while let Some(row) = rows.next().await? {
            let id: String = row.get(0)?;
            let mtime: i64 = row.get(1)?;
            mtimes.insert(id, mtime);
        }

        Ok(mtimes)
    }
}

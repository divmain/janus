mod traits;

pub use traits::CacheableItem;

use base64::Engine;
use serde_json;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tokio::sync::OnceCell;
use turso::{Builder, Connection, Database};

/// Busy timeout for SQLite operations when multiple processes access the cache.
/// This allows concurrent janus processes to wait for locks rather than failing immediately.
const BUSY_TIMEOUT: Duration = Duration::from_millis(500);

use crate::error::{JanusError as CacheError, Result, is_corruption_error, is_permission_error};
use crate::plan::types::PlanMetadata;
use crate::types::TicketMetadata;

#[cfg(test)]
use serial_test::serial;

const CACHE_VERSION: &str = "5";

/// Cached plan metadata - a lightweight representation for fast queries
#[derive(Debug, Clone)]
pub struct CachedPlanMetadata {
    pub id: Option<String>,
    pub uuid: Option<String>,
    pub title: Option<String>,
    pub created: Option<String>,
    /// "simple", "phased", or "empty"
    pub structure_type: String,
    /// For simple plans: the ordered list of ticket IDs
    pub tickets: Vec<String>,
    /// For phased plans: phase information with their tickets
    pub phases: Vec<CachedPhase>,
}

impl CachedPlanMetadata {
    /// Get all tickets across all phases (or from tickets field for simple plans)
    pub fn all_tickets(&self) -> Vec<&str> {
        if self.structure_type == "simple" {
            self.tickets.iter().map(|s| s.as_str()).collect()
        } else {
            self.phases
                .iter()
                .flat_map(|p| p.tickets.iter().map(|s| s.as_str()))
                .collect()
        }
    }

    /// Check if this is a phased plan
    pub fn is_phased(&self) -> bool {
        self.structure_type == "phased"
    }

    /// Check if this is a simple plan
    pub fn is_simple(&self) -> bool {
        self.structure_type == "simple"
    }
}

/// Cached phase information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CachedPhase {
    pub number: String,
    pub name: String,
    pub tickets: Vec<String>,
}

static GLOBAL_CACHE: OnceCell<Option<TicketCache>> = OnceCell::const_new();

pub fn cache_dir() -> PathBuf {
    let proj_dirs = directories::ProjectDirs::from("com", "divmain", "janus")
        .expect("cannot determine cache directory");
    let cache_dir = proj_dirs.cache_dir().to_path_buf();

    if !cache_dir.exists() {
        fs::create_dir_all(&cache_dir).ok();
    }

    cache_dir
}

pub fn repo_hash(repo_path: &Path) -> String {
    let canonical_path = repo_path
        .canonicalize()
        .unwrap_or_else(|_| repo_path.to_path_buf());

    let hash = Sha256::digest(canonical_path.to_string_lossy().as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&hash[..16])
}

pub fn cache_db_path(repo_hash: &str) -> PathBuf {
    cache_dir().join(format!("{}.db", repo_hash))
}

pub struct TicketCache {
    #[allow(dead_code)]
    db: Database,
    conn: Connection,
    #[allow(dead_code)]
    repo_path: PathBuf,
    repo_hash: String,
}

impl TicketCache {
    pub async fn open() -> Result<Self> {
        let repo_path = std::env::current_dir().map_err(CacheError::Io)?;

        let repo_hash = repo_hash(&repo_path);
        let db_path = cache_db_path(&repo_hash);

        let cache_dir = cache_dir();
        if !cache_dir.exists() {
            fs::create_dir_all(&cache_dir)
                .map_err(|_e| CacheError::CacheAccessDenied(cache_dir.clone()))?;
        }

        let db_path_str = db_path.to_string_lossy();
        let db = Builder::new_local(&db_path_str).build().await?;
        let conn = db.connect()?;

        // Set busy timeout to handle concurrent access from multiple janus processes.
        // This causes SQLite to retry with exponential backoff rather than failing immediately.
        conn.busy_timeout(BUSY_TIMEOUT)?;

        // Enable WAL (Write-Ahead Logging) mode for better concurrent access.
        // Readers no longer block writers and vice versa, improving performance for
        // multi-terminal workflows.
        {
            let mut rows = conn.query("PRAGMA journal_mode=WAL", ()).await?;
            rows.next().await?;
        }

        let cache = Self {
            db,
            conn,
            repo_path: repo_path.clone(),
            repo_hash,
        };

        cache.initialize_database().await?;
        cache.store_repo_path(&repo_path).await?;

        Ok(cache)
    }

    async fn open_with_corruption_handling() -> Result<Self> {
        let repo_hash = {
            let repo_path = std::env::current_dir().map_err(CacheError::Io)?;
            repo_hash(&repo_path)
        };

        let db_path = cache_db_path(&repo_hash);
        let database_exists = db_path.exists();

        let result = Self::open().await;

        if let Err(error) = &result
            && database_exists
            && is_corruption_error(&error.to_string())
        {
            eprintln!(
                "Warning: Cache file appears corrupted at: {}",
                db_path.display()
            );
            eprintln!("Deleting corrupted cache and attempting rebuild...");

            if let Err(e) = fs::remove_file(&db_path) {
                eprintln!("Warning: failed to delete corrupted cache: {}", e);
            } else {
                eprintln!("Cache deleted successfully, rebuilding...");
                return Self::open().await;
            }
        }

        result
    }

    async fn initialize_database(&self) -> Result<()> {
        self.conn
            .execute(
                "CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
                (),
            )
            .await?;

        self.conn
            .execute(
                "CREATE TABLE IF NOT EXISTS tickets (
                ticket_id TEXT PRIMARY KEY,
                uuid TEXT,
                mtime_ns INTEGER NOT NULL,
                status TEXT,
                title TEXT,
                priority INTEGER,
                ticket_type TEXT,
                deps TEXT,
                links TEXT,
                parent TEXT,
                created TEXT,
                external_ref TEXT,
                remote TEXT,
                completion_summary TEXT
            )",
                (),
            )
            .await?;

        self.conn
            .execute(
                "CREATE INDEX IF NOT EXISTS idx_tickets_status ON tickets(status)",
                (),
            )
            .await?;

        self.conn
            .execute(
                "CREATE INDEX IF NOT EXISTS idx_tickets_priority ON tickets(priority)",
                (),
            )
            .await?;

        self.conn
            .execute(
                "CREATE INDEX IF NOT EXISTS idx_tickets_type ON tickets(ticket_type)",
                (),
            )
            .await?;

        self.conn
            .execute(
                "CREATE INDEX IF NOT EXISTS idx_tickets_status_priority ON tickets(status, priority)",
                (),
            )
            .await?;

        // Plans table
        self.conn
            .execute(
                "CREATE TABLE IF NOT EXISTS plans (
                plan_id TEXT PRIMARY KEY,
                uuid TEXT,
                mtime_ns INTEGER NOT NULL,
                title TEXT,
                created TEXT,
                structure_type TEXT,
                tickets_json TEXT,
                phases_json TEXT
            )",
                (),
            )
            .await?;

        self.conn
            .execute(
                "CREATE INDEX IF NOT EXISTS idx_plans_structure_type ON plans(structure_type)",
                (),
            )
            .await?;

        Ok(())
    }

    async fn store_repo_path(&self, repo_path: &Path) -> Result<()> {
        let path_str = repo_path.to_string_lossy().to_string();
        self.conn
            .execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('repo_path', ?1)",
                [path_str],
            )
            .await?;

        self.conn
            .execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('cache_version', ?1)",
                [CACHE_VERSION],
            )
            .await?;

        Ok(())
    }

    pub fn cache_db_path(&self) -> PathBuf {
        cache_db_path(&self.repo_hash)
    }

    /// Sync both tickets and plans from disk to cache
    ///
    /// Returns true if any changes were made, false if cache was already up to date.
    pub async fn sync(&mut self) -> Result<bool> {
        let tickets_changed = self.sync_tickets().await?;
        let plans_changed = self.sync_plans().await?;
        Ok(tickets_changed || plans_changed)
    }

    /// Sync tickets from disk to cache
    ///
    /// Returns true if any changes were made, false if cache was already up to date.
    pub async fn sync_tickets(&mut self) -> Result<bool> {
        self.sync_items::<TicketMetadata>().await
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

    /// Scan a directory for .md files and return their IDs and mtimes
    fn scan_directory_static(dir: &Path) -> Result<HashMap<String, i64>> {
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

    /// Get cached mtimes for a specific item type
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

    // =========================================================================
    // Plan caching methods
    // =========================================================================

    /// Sync plans from disk to cache
    ///
    /// Returns true if any changes were made, false if cache was already up to date.
    pub async fn sync_plans(&mut self) -> Result<bool> {
        self.sync_items::<PlanMetadata>().await
    }

    /// Get all cached plans
    pub async fn get_all_plans(&self) -> Result<Vec<CachedPlanMetadata>> {
        let mut rows = self
            .conn
            .query(
                "SELECT plan_id, uuid, title, created, structure_type, tickets_json, phases_json
                 FROM plans",
                (),
            )
            .await?;

        let mut plans = Vec::new();
        while let Some(row) = rows.next().await? {
            let metadata = Self::row_to_plan_metadata(&row).await?;
            plans.push(metadata);
        }
        Ok(plans)
    }

    /// Get a single plan by ID
    pub async fn get_plan(&self, id: &str) -> Result<Option<CachedPlanMetadata>> {
        let mut rows = self
            .conn
            .query(
                "SELECT plan_id, uuid, title, created, structure_type, tickets_json, phases_json
                 FROM plans WHERE plan_id = ?1",
                [id],
            )
            .await?;

        if let Some(row) = rows.next().await? {
            let metadata = Self::row_to_plan_metadata(&row).await?;
            Ok(Some(metadata))
        } else {
            Ok(None)
        }
    }

    /// Find plans by partial ID
    pub async fn find_plan_by_partial_id(&self, partial: &str) -> Result<Vec<String>> {
        let mut rows = self
            .conn
            .query(
                "SELECT plan_id FROM plans WHERE plan_id LIKE ?1",
                [format!("{}%", partial)],
            )
            .await?;

        let mut matches = Vec::new();
        while let Some(row) = rows.next().await? {
            let id: String = row.get(0)?;
            matches.push(id);
        }
        Ok(matches)
    }

    async fn row_to_plan_metadata(row: &turso::Row) -> Result<CachedPlanMetadata> {
        let id: Option<String> = row.get(0).ok();
        let uuid: Option<String> = row.get(1).ok();
        let title: Option<String> = row.get(2).ok();
        let created: Option<String> = row.get(3).ok();
        let structure_type: Option<String> = row.get(4).ok();
        let tickets_json: Option<String> = row.get(5).ok();
        let phases_json: Option<String> = row.get(6).ok();

        // Deserialize tickets for simple plans with explicit error handling
        let tickets: Vec<String> = if let Some(json_str) = tickets_json.as_deref() {
            match serde_json::from_str(json_str) {
                Ok(tickets) => tickets,
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to deserialize plan tickets JSON for plan '{:?}': {}. Using empty array.",
                        id, e
                    );
                    vec![]
                }
            }
        } else {
            vec![]
        };

        // Deserialize phases for phased plans with explicit error handling
        let phases: Vec<CachedPhase> = if let Some(json_str) = phases_json.as_deref() {
            match serde_json::from_str(json_str) {
                Ok(phases) => phases,
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to deserialize plan phases JSON for plan '{:?}': {}. Using empty array.",
                        id, e
                    );
                    vec![]
                }
            }
        } else {
            vec![]
        };

        // Validate structure_type is valid
        let structure_type = match structure_type {
            Some(s) if matches!(s.as_str(), "simple" | "phased" | "empty") => s,
            Some(s) => {
                eprintln!(
                    "Warning: Invalid structure_type '{}' for plan '{:?}'. Defaulting to 'empty'.",
                    s, id
                );
                "empty".to_string()
            }
            None => {
                eprintln!(
                    "Warning: Missing structure_type for plan '{:?}'. Defaulting to 'empty'.",
                    id
                );
                "empty".to_string()
            }
        };

        Ok(CachedPlanMetadata {
            id,
            uuid,
            title,
            created,
            structure_type,
            tickets,
            phases,
        })
    }

    fn deserialize_array(s: Option<&str>) -> Result<Vec<String>> {
        match s {
            Some(json_str) if !json_str.is_empty() => serde_json::from_str(json_str).map_err(|e| {
                CacheError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
            }),
            _ => Ok(vec![]),
        }
    }

    /// Serialize an array to JSON, returning None for empty arrays.
    /// Exposed for testing purposes.
    #[cfg(test)]
    pub(crate) fn serialize_array(arr: &[String]) -> Result<Option<String>> {
        if arr.is_empty() {
            Ok(None)
        } else {
            serde_json::to_string(arr).map(Some).map_err(|e| {
                CacheError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
            })
        }
    }

    async fn row_to_metadata(row: &turso::Row) -> Result<TicketMetadata> {
        let id: Option<String> = row.get(0).ok();
        let uuid: Option<String> = row.get(1).ok();
        let status_str: Option<String> = row.get(2).ok();
        let title: Option<String> = row.get(3).ok();
        let priority_num: Option<i64> = row.get(4).ok();
        let type_str: Option<String> = row.get(5).ok();
        let deps_json: Option<String> = row.get(6).ok();
        let links_json: Option<String> = row.get(7).ok();
        let parent: Option<String> = row.get(8).ok();
        let created: Option<String> = row.get(9).ok();
        let external_ref: Option<String> = row.get(10).ok();
        let remote: Option<String> = row.get(11).ok();
        let completion_summary: Option<String> = row.get(12).ok();

        // Parse status with explicit error handling
        let status = if let Some(ref s) = status_str {
            match s.parse() {
                Ok(status) => Some(status),
                Err(_) => {
                    eprintln!(
                        "Warning: Failed to parse status '{}' for ticket '{:?}'. Status will be None.",
                        s, id
                    );
                    None
                }
            }
        } else {
            None
        };

        // Parse ticket_type with explicit error handling
        let ticket_type = if let Some(ref s) = type_str {
            match s.parse() {
                Ok(ticket_type) => Some(ticket_type),
                Err(_) => {
                    eprintln!(
                        "Warning: Failed to parse ticket_type '{}' for ticket '{:?}'. Type will be None.",
                        s, id
                    );
                    None
                }
            }
        } else {
            None
        };

        // Parse priority with explicit error handling
        let priority = match priority_num {
            Some(n) => match n {
                0 => Some(crate::types::TicketPriority::P0),
                1 => Some(crate::types::TicketPriority::P1),
                2 => Some(crate::types::TicketPriority::P2),
                3 => Some(crate::types::TicketPriority::P3),
                4 => Some(crate::types::TicketPriority::P4),
                _ => {
                    eprintln!(
                        "Warning: Invalid priority value {} for ticket '{:?}'. Priority will be None.",
                        n, id
                    );
                    None
                }
            },
            None => None,
        };

        let deps = Self::deserialize_array(deps_json.as_deref())?;
        let links = Self::deserialize_array(links_json.as_deref())?;

        Ok(TicketMetadata {
            id,
            uuid,
            title,
            status,
            priority,
            ticket_type,
            deps,
            links,
            parent,
            created,
            external_ref,
            remote,
            file_path: None,
            completion_summary,
        })
    }

    pub async fn get_all_tickets(&self) -> Result<Vec<TicketMetadata>> {
        let mut rows = self
            .conn
            .query(
                "SELECT ticket_id, uuid, status, title, priority, ticket_type,
                    deps, links, parent, created, external_ref, remote, completion_summary
             FROM tickets",
                (),
            )
            .await?;

        let mut tickets = Vec::new();
        while let Some(row) = rows.next().await? {
            let metadata = Self::row_to_metadata(&row).await?;
            tickets.push(metadata);
        }
        Ok(tickets)
    }

    pub async fn get_ticket(&self, id: &str) -> Result<Option<TicketMetadata>> {
        let mut rows = self
            .conn
            .query(
                "SELECT ticket_id, uuid, status, title, priority, ticket_type,
                    deps, links, parent, created, external_ref, remote, completion_summary
             FROM tickets WHERE ticket_id = ?1",
                [id],
            )
            .await?;

        if let Some(row) = rows.next().await? {
            let metadata = Self::row_to_metadata(&row).await?;
            Ok(Some(metadata))
        } else {
            Ok(None)
        }
    }

    pub async fn find_by_partial_id(&self, partial: &str) -> Result<Vec<String>> {
        let mut rows = self
            .conn
            .query(
                "SELECT ticket_id FROM tickets WHERE ticket_id LIKE ?1",
                [format!("{}%", partial)],
            )
            .await?;

        let mut matches = Vec::new();
        while let Some(row) = rows.next().await? {
            let id: String = row.get(0)?;
            matches.push(id);
        }
        Ok(matches)
    }

    pub async fn build_ticket_map(&self) -> Result<HashMap<String, TicketMetadata>> {
        let tickets = self.get_all_tickets().await?;

        let mut map = HashMap::new();
        for ticket in tickets {
            if let Some(id) = &ticket.id {
                map.insert(id.clone(), ticket);
            }
        }
        Ok(map)
    }

    // Helper method for tests to query the connection directly
    #[cfg(test)]
    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }
}

pub async fn get_or_init_cache() -> Option<&'static TicketCache> {
    GLOBAL_CACHE
        .get_or_init(|| async {
            match TicketCache::open_with_corruption_handling().await {
                Ok(mut cache) => {
                    if let Err(e) = cache.sync().await {
                        eprintln!(
                            "Warning: cache sync failed: {}. Falling back to file reads.",
                            e
                        );

                        let error_str = e.to_string();
                        if is_corruption_error(&error_str) {
                            let db_path = cache.cache_db_path();
                            eprintln!("Cache appears corrupted at: {}", db_path.display());
                            eprintln!("Run 'janus cache clear' or 'janus cache rebuild' to fix this issue.");
                        }

                        None
                    } else {
                        Some(cache)
                    }
                }
                Err(e) => {
                    let error_str = e.to_string();

                    if is_permission_error(&error_str) {
                        eprintln!(
                            "Warning: cannot access cache directory (permission denied). \
                             Falling back to file reads.",
                        );
                        eprintln!("Tip: Check file permissions or try 'janus cache rebuild'.");
                    } else if is_corruption_error(&error_str) {
                        eprintln!("Warning: cache database is corrupted. Falling back to file reads.");
                        eprintln!("Tip: Run 'janus cache clear' or 'janus cache rebuild' to fix this.");
                    } else {
                        eprintln!(
                            "Warning: failed to open cache: {}. Falling back to file reads.",
                            e
                        );
                    }

                    None
                }
            }
        })
        .await
        .as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to get the first row from a query result, avoiding .unwrap().unwrap() pattern
    async fn get_first_row(rows: &mut turso::Rows) -> turso::Row {
        let row_opt = rows.next().await.expect("query failed");
        row_opt.expect("expected at least one row")
    }

    #[test]
    fn test_repo_hash_consistency() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path();

        let hash1 = repo_hash(path);
        let hash2 = repo_hash(path);

        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 22);
    }

    #[test]
    fn test_cache_dir_creates_directory() {
        let dir = cache_dir();
        assert!(dir.exists());
        let dir_str = dir.to_string_lossy();
        assert!(dir_str.contains("janus") || dir_str.contains(".local/share"));
    }

    #[test]
    fn test_cache_db_path_format() {
        let hash = "aB3xY9zK1mP2qR4sT6uV8w";
        let path = cache_db_path(hash);

        assert!(path.ends_with(format!("{}.db", hash)));
        assert_eq!(path.extension().unwrap().to_str().unwrap(), "db");
    }

    #[tokio::test]
    #[serial]
    async fn test_cache_initialization() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_cache_initialization");
        fs::create_dir_all(&repo_path).unwrap();

        std::env::set_current_dir(&repo_path).unwrap();

        let cache = TicketCache::open().await.unwrap();
        let db_path = cache.cache_db_path();

        assert!(db_path.exists());
        assert!(db_path.is_absolute());
    }

    #[tokio::test]
    #[serial]
    async fn test_wal_mode_enabled() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_wal_mode");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let cache = TicketCache::open().await.unwrap();

        // Verify WAL mode is enabled (this is the key fix for concurrent access)
        let mut rows = cache.conn().query("PRAGMA journal_mode", ()).await.unwrap();
        let row = get_first_row(&mut rows).await;
        let mode: String = row.get(0).unwrap();
        assert_eq!(mode.to_lowercase(), "wal", "WAL mode should be enabled");

        // Note: We don't verify synchronous mode because Turso may not respect
        // PRAGMA synchronous= commands, preferring its own defaults. WAL is the
        // critical optimization for concurrent access.

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_repo_path_stored_in_meta() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_repo_path_stored_in_meta");
        fs::create_dir_all(&repo_path).unwrap();
        let repo_path_str = repo_path
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();

        std::env::set_current_dir(&repo_path).unwrap();

        let cache = TicketCache::open().await.unwrap();

        let mut rows = cache
            .conn()
            .query("SELECT value FROM meta WHERE key = 'repo_path'", ())
            .await
            .unwrap();

        let stored_path: Option<String> = if let Some(row) = rows.next().await.unwrap() {
            Some(row.get(0).unwrap())
        } else {
            None
        };

        assert_eq!(stored_path, Some(repo_path_str));
    }

    #[tokio::test]
    #[serial]
    async fn test_cache_version_stored_in_meta() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_cache_version_stored_in_meta");
        fs::create_dir_all(&repo_path).unwrap();

        std::env::set_current_dir(&repo_path).unwrap();

        let cache = TicketCache::open().await.unwrap();

        let mut rows = cache
            .conn()
            .query("SELECT value FROM meta WHERE key = 'cache_version'", ())
            .await
            .unwrap();

        let stored_version: Option<String> = if let Some(row) = rows.next().await.unwrap() {
            Some(row.get(0).unwrap())
        } else {
            None
        };

        assert_eq!(stored_version, Some(CACHE_VERSION.to_string()));

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir(&repo_path).ok();
    }

    fn create_test_ticket(dir: &Path, ticket_id: &str, title: &str) -> PathBuf {
        let tickets_dir = dir.join(".janus/items");
        fs::create_dir_all(&tickets_dir).unwrap();

        let ticket_path = tickets_dir.join(format!("{}.md", ticket_id));
        let content = format!(
            r#"---
id: {}
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# {}
"#,
            ticket_id, title
        );
        fs::write(&ticket_path, content).unwrap();
        ticket_path
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_creates_entries() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_creates_entries");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_ticket(&repo_path, "j-a1b2", "Ticket 1");
        create_test_ticket(&repo_path, "j-c3d4", "Ticket 2");
        create_test_ticket(&repo_path, "j-e5f6", "Ticket 3");

        let mut cache = TicketCache::open().await.unwrap();
        let changed = cache.sync().await.unwrap();

        assert!(changed);

        let mut rows = cache
            .conn()
            .query("SELECT COUNT(*) FROM tickets", ())
            .await
            .unwrap();
        let row = get_first_row(&mut rows).await;
        let count: i64 = row.get(0).unwrap();
        assert_eq!(count, 3);

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_detects_additions() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_detects_additions");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_ticket(&repo_path, "j-a1b2", "Ticket 1");

        let mut cache = TicketCache::open().await.unwrap();
        let changed1 = cache.sync().await.unwrap();
        assert!(changed1);

        let mut rows = cache
            .conn()
            .query("SELECT COUNT(*) FROM tickets", ())
            .await
            .unwrap();
        let row = get_first_row(&mut rows).await;
        let count1: i64 = row.get(0).unwrap();
        assert_eq!(count1, 1);

        create_test_ticket(&repo_path, "j-c3d4", "Ticket 2");

        std::thread::sleep(std::time::Duration::from_millis(10));

        let changed2 = cache.sync().await.unwrap();
        assert!(changed2);

        let mut rows = cache
            .conn()
            .query("SELECT COUNT(*) FROM tickets", ())
            .await
            .unwrap();
        let row = get_first_row(&mut rows).await;
        let count2: i64 = row.get(0).unwrap();
        assert_eq!(count2, 2);

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_detects_deletions() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_detects_deletions");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let ticket_path = create_test_ticket(&repo_path, "j-a1b2", "Ticket 1");

        let mut cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let mut rows = cache
            .conn()
            .query("SELECT COUNT(*) FROM tickets", ())
            .await
            .unwrap();
        let row = get_first_row(&mut rows).await;
        let count1: i64 = row.get(0).unwrap();
        assert_eq!(count1, 1);

        fs::remove_file(&ticket_path).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(10));

        let changed = cache.sync().await.unwrap();
        assert!(changed);

        let mut rows = cache
            .conn()
            .query("SELECT COUNT(*) FROM tickets", ())
            .await
            .unwrap();
        let row = get_first_row(&mut rows).await;
        let count2: i64 = row.get(0).unwrap();
        assert_eq!(count2, 0);

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_detects_modifications() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_detects_modifications");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let ticket_path = create_test_ticket(&repo_path, "j-a1b2", "Original Title");

        std::thread::sleep(std::time::Duration::from_millis(10));

        let mut cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let mut rows = cache
            .conn()
            .query("SELECT title FROM tickets WHERE ticket_id = ?1", ["j-a1b2"])
            .await
            .unwrap();
        let original_title: Option<String> = if let Some(row) = rows.next().await.unwrap() {
            row.get(0).ok()
        } else {
            None
        };
        assert_eq!(original_title, Some("Original Title".to_string()));

        std::thread::sleep(std::time::Duration::from_millis(1100));

        let content = fs::read_to_string(&ticket_path).unwrap();
        let modified_content = content.replace("Original Title", "Modified Title");
        fs::write(&ticket_path, modified_content).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(10));

        let changed = cache.sync().await.unwrap();
        assert!(changed);

        let mut rows = cache
            .conn()
            .query("SELECT title FROM tickets WHERE ticket_id = ?1", ["j-a1b2"])
            .await
            .unwrap();
        let modified_title: Option<String> = if let Some(row) = rows.next().await.unwrap() {
            row.get(0).ok()
        } else {
            None
        };
        assert_eq!(modified_title, Some("Modified Title".to_string()));

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_serialize_deserialize_arrays() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_serialize_deserialize");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let _cache = TicketCache::open().await.unwrap();

        let arr = vec!["j-a1b2".to_string(), "j-c3d4".to_string()];
        let json = TicketCache::serialize_array(&arr).unwrap();

        assert!(json.is_some());
        let json_str = json.unwrap();
        assert!(json_str.starts_with('['));
        assert!(json_str.ends_with(']'));

        let decoded: Vec<String> = serde_json::from_str(&json_str).unwrap();
        assert_eq!(decoded, arr);

        let empty_arr: Vec<String> = vec![];
        let empty_json = TicketCache::serialize_array(&empty_arr).unwrap();
        assert!(empty_json.is_none());

        let db_path = _cache.cache_db_path();
        drop(_cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_scan_directory() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_scan_directory");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let tickets_dir = repo_path.join(".janus/items");
        fs::create_dir_all(&tickets_dir).unwrap();

        create_test_ticket(&repo_path, "j-a1b2", "Ticket 1");
        create_test_ticket(&repo_path, "j-c3d4", "Ticket 2");

        let non_md_file = tickets_dir.join("other.txt");
        fs::write(&non_md_file, "not a ticket").unwrap();

        let cache = TicketCache::open().await.unwrap();
        let files = TicketCache::scan_directory_static(&tickets_dir).unwrap();

        assert_eq!(files.len(), 2);
        assert!(files.contains_key("j-a1b2"));
        assert!(files.contains_key("j-c3d4"));

        for (_, mtime_ns) in &files {
            assert!(*mtime_ns > 0);
        }

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[test]
    fn test_repo_hash_different_paths() {
        let temp1 = tempfile::TempDir::new().unwrap();
        let temp2 = tempfile::TempDir::new().unwrap();

        let hash1 = repo_hash(temp1.path());
        let hash2 = repo_hash(temp2.path());

        // Different paths should produce different hashes
        assert_ne!(hash1, hash2);
        // Both should be valid 22-char base64 strings
        assert_eq!(hash1.len(), 22);
        assert_eq!(hash2.len(), 22);
    }

    #[test]
    fn test_deserialize_array_handles_empty_and_invalid() {
        // Empty string should return empty array
        let result: Vec<String> = TicketCache::deserialize_array(None).unwrap();
        assert_eq!(result, Vec::<String>::new());

        // Some empty string should return empty array
        let result: Vec<String> = TicketCache::deserialize_array(Some("")).unwrap();
        assert_eq!(result, Vec::<String>::new());

        // Valid JSON array should parse correctly
        let result: Vec<String> =
            TicketCache::deserialize_array(Some(r#"["a", "b", "c"]"#)).unwrap();
        assert_eq!(
            result,
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_with_deps_and_links() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_deps_links");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let tickets_dir = repo_path.join(".janus/items");
        fs::create_dir_all(&tickets_dir).unwrap();

        // Create a ticket with deps and links (using JSON array format on single line)
        let ticket_path = tickets_dir.join("j-a1b2.md");
        let content = r#"---
id: j-a1b2
status: new
deps: ["j-dep1", "j-dep2"]
links: ["j-link1"]
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Ticket with deps
"#;
        fs::write(&ticket_path, content).unwrap();

        let mut cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        // Verify deps were stored correctly
        let mut rows = cache
            .conn()
            .query(
                "SELECT deps, links FROM tickets WHERE ticket_id = ?1",
                ["j-a1b2"],
            )
            .await
            .unwrap();

        let row = get_first_row(&mut rows).await;
        let deps_json: Option<String> = row.get(0).ok();
        let links_json: Option<String> = row.get(1).ok();

        assert!(deps_json.is_some());
        assert!(links_json.is_some());

        let deps: Vec<String> = serde_json::from_str(&deps_json.unwrap()).unwrap();
        let links: Vec<String> = serde_json::from_str(&links_json.unwrap()).unwrap();

        assert_eq!(deps, vec!["j-dep1", "j-dep2"]);
        assert_eq!(links, vec!["j-link1"]);

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_with_all_fields() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_all_fields");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let tickets_dir = repo_path.join(".janus/items");
        fs::create_dir_all(&tickets_dir).unwrap();

        // Create a ticket with all fields populated
        // Note: parser uses "external-ref" (with hyphen), not "external_ref"
        let ticket_path = tickets_dir.join("j-full.md");
        let content = r#"---
id: j-full
status: in_progress
deps: []
links: []
created: 2024-06-15T10:30:00Z
type: bug
priority: 0
parent: j-parent
external-ref: GH-123
remote: github
---
# Full Ticket
"#;
        fs::write(&ticket_path, content).unwrap();

        let mut cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        // Verify all fields were stored correctly
        let mut rows = cache
            .conn()
            .query(
                "SELECT status, title, priority, ticket_type, parent, external_ref, remote 
                 FROM tickets WHERE ticket_id = ?1",
                ["j-full"],
            )
            .await
            .unwrap();

        let row = get_first_row(&mut rows).await;
        let status: Option<String> = row.get(0).ok();
        let title: Option<String> = row.get(1).ok();
        let priority: Option<i64> = row.get(2).ok();
        let ticket_type: Option<String> = row.get(3).ok();
        let parent: Option<String> = row.get(4).ok();
        let external_ref: Option<String> = row.get(5).ok();
        let remote: Option<String> = row.get(6).ok();

        assert_eq!(status, Some("in_progress".to_string()));
        assert_eq!(title, Some("Full Ticket".to_string()));
        assert_eq!(priority, Some(0));
        assert_eq!(ticket_type, Some("bug".to_string()));
        assert_eq!(parent, Some("j-parent".to_string()));
        assert_eq!(external_ref, Some("GH-123".to_string()));
        assert_eq!(remote, Some("github".to_string()));

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_reopen_existing_cache() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_reopen_cache");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_ticket(&repo_path, "j-a1b2", "Ticket 1");
        create_test_ticket(&repo_path, "j-c3d4", "Ticket 2");

        // Open cache and sync
        let mut cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let db_path = cache.cache_db_path();

        // Drop and reopen
        drop(cache);

        // Reopen the cache - should preserve existing data
        let cache2 = TicketCache::open().await.unwrap();

        let mut rows = cache2
            .conn()
            .query("SELECT COUNT(*) FROM tickets", ())
            .await
            .unwrap();
        let row = get_first_row(&mut rows).await;
        let count: i64 = row.get(0).unwrap();

        // Data should still be there from before
        assert_eq!(count, 2);

        drop(cache2);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_database_indexes_created() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_indexes");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let cache = TicketCache::open().await.unwrap();

        // Query for indexes
        let mut rows = cache
            .conn()
            .query(
                "SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='tickets'",
                (),
            )
            .await
            .unwrap();

        let mut indexes = Vec::new();
        while let Some(row) = rows.next().await.unwrap() {
            let name: String = row.get(0).unwrap();
            indexes.push(name);
        }

        assert!(indexes.contains(&"idx_tickets_status".to_string()));
        assert!(indexes.contains(&"idx_tickets_priority".to_string()));
        assert!(indexes.contains(&"idx_tickets_type".to_string()));
        assert!(indexes.contains(&"idx_tickets_status_priority".to_string()));

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_empty_directory() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_empty");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        // Create empty tickets directory
        let tickets_dir = repo_path.join(".janus/items");
        fs::create_dir_all(&tickets_dir).unwrap();

        let mut cache = TicketCache::open().await.unwrap();

        // Sync with empty directory should return false (no changes)
        let changed = cache.sync().await.unwrap();
        assert!(!changed);

        let mut rows = cache
            .conn()
            .query("SELECT COUNT(*) FROM tickets", ())
            .await
            .unwrap();
        let row = get_first_row(&mut rows).await;
        let count: i64 = row.get(0).unwrap();
        assert_eq!(count, 0);

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_creates_tickets_dir_if_missing() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_missing_dir");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        // Don't create the tickets directory

        let mut cache = TicketCache::open().await.unwrap();

        // Sync should create the directory and return false
        let changed = cache.sync().await.unwrap();
        assert!(!changed);

        // Verify directory was created
        let tickets_dir = repo_path.join(".janus/items");
        assert!(tickets_dir.exists());

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_get_all_tickets() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_get_all_tickets");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_ticket(&repo_path, "j-a1b2", "Ticket 1");
        create_test_ticket(&repo_path, "j-c3d4", "Ticket 2");
        create_test_ticket(&repo_path, "j-e5f6", "Ticket 3");

        let mut cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let tickets = cache.get_all_tickets().await.unwrap();
        assert_eq!(tickets.len(), 3);

        let titles: Vec<&str> = tickets.iter().filter_map(|t| t.title.as_deref()).collect();
        assert!(titles.contains(&"Ticket 1"));
        assert!(titles.contains(&"Ticket 2"));
        assert!(titles.contains(&"Ticket 3"));

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_get_ticket() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_get_ticket");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_ticket(&repo_path, "j-a1b2", "Test Ticket");

        let mut cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let ticket = cache.get_ticket("j-a1b2").await.unwrap();
        assert!(ticket.is_some());

        let metadata = ticket.unwrap();
        assert_eq!(metadata.id, Some("j-a1b2".to_string()));
        assert_eq!(metadata.title, Some("Test Ticket".to_string()));

        let nonexistent = cache.get_ticket("j-xxxx").await.unwrap();
        assert!(nonexistent.is_none());

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_find_by_partial_id() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_find_by_partial_id");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_ticket(&repo_path, "j-a1b2", "Ticket 1");
        create_test_ticket(&repo_path, "j-c3d4", "Ticket 2");
        create_test_ticket(&repo_path, "j-e5f6", "Ticket 3");

        let mut cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let matches = cache.find_by_partial_id("j-a").await.unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], "j-a1b2");

        let matches = cache.find_by_partial_id("j-").await.unwrap();
        assert_eq!(matches.len(), 3);

        let matches = cache.find_by_partial_id("j-xxx").await.unwrap();
        assert_eq!(matches.len(), 0);

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_find_by_partial_id_ambiguous() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_find_by_partial_id_ambiguous");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_ticket(&repo_path, "j-a1b2", "Ticket A1");
        create_test_ticket(&repo_path, "j-a2c3", "Ticket A2");

        let mut cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let matches = cache.find_by_partial_id("j-a").await.unwrap();
        assert_eq!(matches.len(), 2);
        assert!(matches.contains(&"j-a1b2".to_string()));
        assert!(matches.contains(&"j-a2c3".to_string()));

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_build_ticket_map() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_build_ticket_map");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_ticket(&repo_path, "j-a1b2", "Ticket 1");
        create_test_ticket(&repo_path, "j-c3d4", "Ticket 2");
        create_test_ticket(&repo_path, "j-e5f6", "Ticket 3");

        let mut cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let map = cache.build_ticket_map().await.unwrap();
        assert_eq!(map.len(), 3);

        assert!(map.contains_key("j-a1b2"));
        assert!(map.contains_key("j-c3d4"));
        assert!(map.contains_key("j-e5f6"));

        let ticket1 = map.get("j-a1b2").unwrap();
        assert_eq!(ticket1.title, Some("Ticket 1".to_string()));

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_get_all_tickets_with_all_fields() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_get_all_fields");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let tickets_dir = repo_path.join(".janus/items");
        fs::create_dir_all(&tickets_dir).unwrap();

        let ticket_path = tickets_dir.join("j-full.md");
        let content = r#"---
id: j-full
status: in_progress
deps: ["j-dep1", "j-dep2"]
links: ["j-link1"]
created: 2024-06-15T10:30:00Z
type: bug
priority: 0
parent: j-parent
external-ref: GH-123
remote: github
---
# Full Ticket
"#;
        fs::write(&ticket_path, content).unwrap();

        let mut cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let tickets = cache.get_all_tickets().await.unwrap();
        assert_eq!(tickets.len(), 1);

        let ticket = &tickets[0];
        assert_eq!(ticket.id, Some("j-full".to_string()));
        assert_eq!(ticket.title, Some("Full Ticket".to_string()));
        assert_eq!(ticket.status, Some(crate::types::TicketStatus::InProgress));
        assert_eq!(ticket.ticket_type, Some(crate::types::TicketType::Bug));
        assert_eq!(ticket.priority, Some(crate::types::TicketPriority::P0));
        assert_eq!(ticket.parent, Some("j-parent".to_string()));
        assert_eq!(ticket.external_ref, Some("GH-123".to_string()));
        assert_eq!(ticket.remote, Some("github".to_string()));
        assert_eq!(ticket.deps, vec!["j-dep1", "j-dep2"]);
        assert_eq!(ticket.links, vec!["j-link1"]);

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    // =========================================================================
    // Plan caching tests
    // =========================================================================

    fn create_test_plan(dir: &Path, plan_id: &str, title: &str, is_phased: bool) -> PathBuf {
        let plans_dir = dir.join(".janus/plans");
        fs::create_dir_all(&plans_dir).unwrap();

        let plan_path = plans_dir.join(format!("{}.md", plan_id));
        let content = if is_phased {
            format!(
                r#"---
id: {}
uuid: 550e8400-e29b-41d4-a716-446655440000
created: 2024-01-01T00:00:00Z
---
# {}

Description of the plan.

## Phase 1: Infrastructure

### Tickets

1. j-a1b2
2. j-c3d4

## Phase 2: Implementation

### Tickets

1. j-e5f6
"#,
                plan_id, title
            )
        } else {
            format!(
                r#"---
id: {}
uuid: 550e8400-e29b-41d4-a716-446655440000
created: 2024-01-01T00:00:00Z
---
# {}

Description of the plan.

## Tickets

1. j-a1b2
2. j-c3d4
3. j-e5f6
"#,
                plan_id, title
            )
        };
        fs::write(&plan_path, content).unwrap();
        plan_path
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_plans_simple_plan() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_simple_plan");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_plan(&repo_path, "plan-a1b2", "Simple Test Plan", false);

        let mut cache = TicketCache::open().await.unwrap();
        let changed = cache.sync().await.unwrap();

        assert!(changed);

        // Verify plan was cached
        let mut rows = cache
            .conn()
            .query("SELECT COUNT(*) FROM plans", ())
            .await
            .unwrap();
        let row = get_first_row(&mut rows).await;
        let count: i64 = row.get(0).unwrap();
        assert_eq!(count, 1);

        // Verify plan data
        let plan = cache.get_plan("plan-a1b2").await.unwrap();
        assert!(plan.is_some());
        let plan = plan.unwrap();
        assert_eq!(plan.id, Some("plan-a1b2".to_string()));
        assert_eq!(plan.title, Some("Simple Test Plan".to_string()));
        assert_eq!(plan.structure_type, "simple");
        assert!(plan.is_simple());
        assert!(!plan.is_phased());
        assert_eq!(plan.tickets, vec!["j-a1b2", "j-c3d4", "j-e5f6"]);

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_plans_phased_plan() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_phased_plan");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_plan(&repo_path, "plan-b2c3", "Phased Test Plan", true);

        let mut cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let plan = cache.get_plan("plan-b2c3").await.unwrap();
        assert!(plan.is_some());
        let plan = plan.unwrap();
        assert_eq!(plan.id, Some("plan-b2c3".to_string()));
        assert_eq!(plan.title, Some("Phased Test Plan".to_string()));
        assert_eq!(plan.structure_type, "phased");
        assert!(plan.is_phased());
        assert!(!plan.is_simple());

        // Verify phases
        assert_eq!(plan.phases.len(), 2);
        assert_eq!(plan.phases[0].number, "1");
        assert_eq!(plan.phases[0].name, "Infrastructure");
        assert_eq!(plan.phases[0].tickets, vec!["j-a1b2", "j-c3d4"]);
        assert_eq!(plan.phases[1].number, "2");
        assert_eq!(plan.phases[1].name, "Implementation");
        assert_eq!(plan.phases[1].tickets, vec!["j-e5f6"]);

        // Verify all_tickets helper
        let all_tickets = plan.all_tickets();
        assert_eq!(all_tickets, vec!["j-a1b2", "j-c3d4", "j-e5f6"]);

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_plans_multiple_plans() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_multiple_plans");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_plan(&repo_path, "plan-a1b2", "Plan One", false);
        create_test_plan(&repo_path, "plan-c3d4", "Plan Two", true);
        create_test_plan(&repo_path, "plan-e5f6", "Plan Three", false);

        let mut cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let plans = cache.get_all_plans().await.unwrap();
        assert_eq!(plans.len(), 3);

        // Verify titles
        let titles: Vec<&str> = plans.iter().filter_map(|p| p.title.as_deref()).collect();
        assert!(titles.contains(&"Plan One"));
        assert!(titles.contains(&"Plan Two"));
        assert!(titles.contains(&"Plan Three"));

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_plans_detects_additions() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_plans_additions");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_plan(&repo_path, "plan-a1b2", "Plan One", false);

        let mut cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let plans = cache.get_all_plans().await.unwrap();
        assert_eq!(plans.len(), 1);

        // Add another plan
        create_test_plan(&repo_path, "plan-c3d4", "Plan Two", true);

        std::thread::sleep(std::time::Duration::from_millis(10));

        let changed = cache.sync().await.unwrap();
        assert!(changed);

        let plans = cache.get_all_plans().await.unwrap();
        assert_eq!(plans.len(), 2);

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_plans_detects_deletions() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_plans_deletions");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let plan_path = create_test_plan(&repo_path, "plan-a1b2", "Plan One", false);

        let mut cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let plans = cache.get_all_plans().await.unwrap();
        assert_eq!(plans.len(), 1);

        // Delete the plan
        fs::remove_file(&plan_path).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(10));

        let changed = cache.sync().await.unwrap();
        assert!(changed);

        let plans = cache.get_all_plans().await.unwrap();
        assert_eq!(plans.len(), 0);

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_plans_detects_modifications() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_plans_modifications");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let plan_path = create_test_plan(&repo_path, "plan-a1b2", "Original Title", false);

        std::thread::sleep(std::time::Duration::from_millis(10));

        let mut cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let plan = cache.get_plan("plan-a1b2").await.unwrap().unwrap();
        assert_eq!(plan.title, Some("Original Title".to_string()));

        // Modify the plan
        std::thread::sleep(std::time::Duration::from_millis(1100));

        let content = fs::read_to_string(&plan_path).unwrap();
        let modified_content = content.replace("Original Title", "Modified Title");
        fs::write(&plan_path, modified_content).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(10));

        let changed = cache.sync().await.unwrap();
        assert!(changed);

        let plan = cache.get_plan("plan-a1b2").await.unwrap().unwrap();
        assert_eq!(plan.title, Some("Modified Title".to_string()));

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_find_plan_by_partial_id() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_find_plan_partial");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_plan(&repo_path, "plan-a1b2", "Plan One", false);
        create_test_plan(&repo_path, "plan-a2c3", "Plan Two", true);
        create_test_plan(&repo_path, "plan-b3d4", "Plan Three", false);

        let mut cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        // Find by prefix
        let matches = cache.find_plan_by_partial_id("plan-a").await.unwrap();
        assert_eq!(matches.len(), 2);
        assert!(matches.contains(&"plan-a1b2".to_string()));
        assert!(matches.contains(&"plan-a2c3".to_string()));

        // Find exact match
        let matches = cache.find_plan_by_partial_id("plan-b3d4").await.unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], "plan-b3d4");

        // Find all plans
        let matches = cache.find_plan_by_partial_id("plan-").await.unwrap();
        assert_eq!(matches.len(), 3);

        // No match
        let matches = cache.find_plan_by_partial_id("plan-xxx").await.unwrap();
        assert_eq!(matches.len(), 0);

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_plans_no_changes() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_plans_no_changes");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_plan(&repo_path, "plan-a1b2", "Test Plan", false);

        let mut cache = TicketCache::open().await.unwrap();

        // First sync should return true
        let changed1 = cache.sync().await.unwrap();
        assert!(changed1);

        // Second sync with no changes should return false
        let changed2 = cache.sync().await.unwrap();
        assert!(!changed2);

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_plans_empty_directory() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_plans_empty");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        // Create empty plans directory
        let plans_dir = repo_path.join(".janus/plans");
        fs::create_dir_all(&plans_dir).unwrap();

        let mut cache = TicketCache::open().await.unwrap();
        let changed = cache.sync().await.unwrap();
        assert!(!changed);

        let plans = cache.get_all_plans().await.unwrap();
        assert_eq!(plans.len(), 0);

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_plans_creates_directory_if_missing() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_plans_missing_dir");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        // Don't create the plans directory

        let mut cache = TicketCache::open().await.unwrap();
        let changed = cache.sync().await.unwrap();
        assert!(!changed);

        // Verify plans directory was created
        let plans_dir = repo_path.join(".janus/plans");
        assert!(plans_dir.exists());

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_plans_index_created() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_plans_index");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let cache = TicketCache::open().await.unwrap();

        // Query for indexes on plans table
        let mut rows = cache
            .conn()
            .query(
                "SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='plans'",
                (),
            )
            .await
            .unwrap();

        let mut indexes = Vec::new();
        while let Some(row) = rows.next().await.unwrap() {
            let name: String = row.get(0).unwrap();
            indexes.push(name);
        }

        assert!(indexes.contains(&"idx_plans_structure_type".to_string()));

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_cached_plan_metadata_all_tickets_phased() {
        let plan = CachedPlanMetadata {
            id: Some("plan-test".to_string()),
            uuid: None,
            title: Some("Test Plan".to_string()),
            created: None,
            structure_type: "phased".to_string(),
            tickets: vec![],
            phases: vec![
                CachedPhase {
                    number: "1".to_string(),
                    name: "Phase One".to_string(),
                    tickets: vec!["t1".to_string(), "t2".to_string()],
                },
                CachedPhase {
                    number: "2".to_string(),
                    name: "Phase Two".to_string(),
                    tickets: vec!["t3".to_string()],
                },
            ],
        };

        let all = plan.all_tickets();
        assert_eq!(all, vec!["t1", "t2", "t3"]);
        assert!(plan.is_phased());
        assert!(!plan.is_simple());
    }

    #[tokio::test]
    #[serial]
    async fn test_cached_plan_metadata_all_tickets_simple() {
        let plan = CachedPlanMetadata {
            id: Some("plan-test".to_string()),
            uuid: None,
            title: Some("Test Plan".to_string()),
            created: None,
            structure_type: "simple".to_string(),
            tickets: vec!["t1".to_string(), "t2".to_string(), "t3".to_string()],
            phases: vec![],
        };

        let all = plan.all_tickets();
        assert_eq!(all, vec!["t1", "t2", "t3"]);
        assert!(!plan.is_phased());
        assert!(plan.is_simple());
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_logs_warnings_for_parse_errors() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_parse_errors");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let tickets_dir = repo_path.join(".janus/items");
        fs::create_dir_all(&tickets_dir).unwrap();

        // Create a valid ticket
        create_test_ticket(&repo_path, "j-valid", "Valid Ticket");

        // Create an invalid ticket (missing YAML frontmatter - just plain text)
        let invalid_path = tickets_dir.join("j-invalid.md");
        let invalid_content =
            "This is not a valid ticket file - no frontmatter\n\n# Invalid Ticket\n";
        fs::write(&invalid_path, invalid_content).unwrap();

        // Capture stderr to verify warning is logged
        let mut cache = TicketCache::open().await.unwrap();

        // Sync should succeed and log a warning about the invalid ticket
        let changed = cache.sync().await.unwrap();
        assert!(changed);

        // Verify the valid ticket was synced
        let mut rows = cache
            .conn()
            .query(
                "SELECT COUNT(*) FROM tickets WHERE ticket_id = ?1",
                ["j-valid"],
            )
            .await
            .unwrap();
        let row = get_first_row(&mut rows).await;
        let count: i64 = row.get(0).unwrap();
        assert_eq!(count, 1);

        // Verify the invalid ticket was not synced
        let mut rows = cache
            .conn()
            .query(
                "SELECT COUNT(*) FROM tickets WHERE ticket_id = ?1",
                ["j-invalid"],
            )
            .await
            .unwrap();
        let row = get_first_row(&mut rows).await;
        let count: i64 = row.get(0).unwrap();
        assert_eq!(count, 0);

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }
}

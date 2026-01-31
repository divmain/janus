//! Database lifecycle management for the cache.
//!
//! This module handles:
//! - Opening and initializing the SQLite database
//! - Schema creation and migrations
//! - Version validation
//! - Corruption detection and recovery

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use turso::{Builder, Connection, Database};

use crate::error::{JanusError as CacheError, Result, is_corruption_error};
use crate::events::log_cache_rebuilt;

use super::paths::{cache_db_path, cache_dir, repo_hash};

/// Busy timeout for SQLite operations when multiple processes access the cache.
/// This allows concurrent janus processes to wait for locks rather than failing immediately.
const BUSY_TIMEOUT: Duration = Duration::from_millis(500);

/// Current cache schema version. Increment when schema changes.
/// This includes feature flags to ensure caches with different features are isolated.
#[cfg(feature = "semantic-search")]
pub(crate) const CACHE_VERSION: &str = "13-semantic";
#[cfg(not(feature = "semantic-search"))]
pub(crate) const CACHE_VERSION: &str = "13";

/// Maximum number of retry attempts when creating database connections.
///
/// This retry logic handles transient errors during connection creation, such as:
/// - Database file lock contention from concurrent processes
/// - Temporary I/O errors
/// - Filesystem issues during WAL file access
///
/// **Limitations:**
/// - Uses fixed delay (no exponential backoff)
/// - Short total timeout (~150ms) may be insufficient for genuine database contention
/// - Does not distinguish between retryable and non-retryable errors
///
/// For operations requiring stronger concurrency guarantees, consider using
/// SQLite's built-in BUSY_TIMEOUT mechanism instead.
const MAX_RETRIES: u32 = 3;

/// Base delay between retry attempts in milliseconds.
///
/// Each retry waits this duration before attempting to reconnect. The total
/// maximum timeout is approximately `BASE_RETRY_DELAY_MS * MAX_RETRIES`.
///
/// **Note:** This value is intentionally conservative (~50ms) to provide
/// a reasonable balance between responsiveness and recovery time. For
/// production environments with higher contention, this may need adjustment.
const BASE_RETRY_DELAY_MS: u64 = 50;

pub struct TicketCache {
    pub(crate) db: Database,
    #[allow(dead_code)]
    pub(crate) repo_path: PathBuf,
    pub(crate) repo_hash: String,
}

impl TicketCache {
    pub async fn open() -> Result<Self> {
        let repo_path = std::env::current_dir().map_err(CacheError::Io)?;

        let repo_hash = repo_hash(&repo_path);
        let db_path = cache_db_path(&repo_hash);

        let cache_directory = cache_dir();
        if !cache_directory.exists() {
            fs::create_dir_all(&cache_directory)
                .map_err(|_e| CacheError::CacheAccessDenied(cache_directory.clone()))?;
        }

        let db_path_str = db_path.to_string_lossy();
        let db = Builder::new_local(&db_path_str).build().await?;

        let conn = db.connect()?;

        conn.busy_timeout(BUSY_TIMEOUT)?;

        {
            let mut rows = conn.query("PRAGMA journal_mode=WAL", ()).await?;
            rows.next().await?;
        }

        let cache = Self {
            db,
            repo_path: repo_path.clone(),
            repo_hash,
        };

        cache.initialize_database(&conn).await?;
        cache.validate_cache_version(&conn).await?;
        cache.store_repo_path(&repo_path, &conn).await?;

        Ok(cache)
    }

    /// Open with automatic corruption handling.
    ///
    /// If the database is corrupted, attempts to delete and rebuild it.
    pub(crate) async fn open_with_corruption_handling() -> Result<Self> {
        let repo_hash_value = {
            let repo_path = std::env::current_dir().map_err(CacheError::Io)?;
            repo_hash(&repo_path)
        };

        let db_path = cache_db_path(&repo_hash_value);
        let database_exists = db_path.exists();

        let result = Self::open().await;

        if let Err(error) = &result
            && database_exists
            && is_corruption_error(error)
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
                let result = Self::open().await;
                if result.is_ok() {
                    log_cache_rebuilt(
                        "corruption_recovery",
                        "automatic_recovery",
                        None,
                        None,
                        Some(serde_json::json!({
                            "previous_error": error.to_string(),
                            "database_existed": true,
                        })),
                    );
                }
                return result;
            }
        }

        result
    }

    async fn initialize_database(&self, conn: &Connection) -> Result<()> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            (),
        )
        .await?;

        conn.execute(
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
                completion_summary TEXT,
                spawned_from TEXT,
                spawn_context TEXT,
                depth INTEGER,
                file_path TEXT,
                triaged INTEGER,
                body TEXT,
                size TEXT
            )",
            (),
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tickets_status ON tickets(status)",
            (),
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tickets_priority ON tickets(priority)",
            (),
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tickets_type ON tickets(ticket_type)",
            (),
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tickets_status_priority ON tickets(status, priority)",
            (),
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tickets_spawned_from ON tickets(spawned_from)",
            (),
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tickets_depth ON tickets(depth)",
            (),
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tickets_size ON tickets(size)",
            (),
        )
        .await?;

        #[cfg(feature = "semantic-search")]
        {
            // Add embedding column for semantic search (F32_BLOB with 384 dimensions)
            // Note: This is a no-op if the column already exists due to IF NOT EXISTS
            conn.execute("ALTER TABLE tickets ADD COLUMN embedding F32_BLOB(384)", ())
                .await
                .ok(); // Ignore errors if column already exists

            // Create DiskANN vector index for fast similarity search
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_tickets_embedding ON tickets(libsql_vector_idx(embedding, 'metric=cosine'))",
                (),
            )
            .await
            .ok(); // Ignore errors if index already exists
        }

        conn.execute(
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

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_plans_structure_type ON plans(structure_type)",
            (),
        )
        .await?;

        Ok(())
    }

    async fn store_repo_path(&self, repo_path: &Path, conn: &Connection) -> Result<()> {
        let path_str = repo_path.to_string_lossy().to_string();
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('repo_path', ?1)",
            [path_str],
        )
        .await?;

        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('cache_version', ?1)",
            [CACHE_VERSION],
        )
        .await?;

        #[cfg(feature = "semantic-search")]
        {
            // Track the embedding model version
            use crate::embedding::model::EMBEDDING_MODEL_NAME;
            conn.execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('embedding_model', ?1)",
                (EMBEDDING_MODEL_NAME,),
            )
            .await?;
        }

        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) async fn get_meta(&self, key: &str) -> Result<Option<String>> {
        let conn = self.create_connection().await?;
        let mut rows = conn
            .query("SELECT value FROM meta WHERE key = ?1", [key])
            .await?;

        match rows.next().await? {
            Some(row) => {
                let value: Option<String> = row.get(0).ok();
                Ok(value)
            }
            None => Ok(None),
        }
    }

    async fn validate_cache_version(&self, conn: &Connection) -> Result<()> {
        if let Some(row) = conn
            .query("SELECT value FROM meta WHERE key = ?1", ["cache_version"])
            .await?
            .next()
            .await?
            && let Ok(stored_version) = row.get::<String>(0)
            && stored_version != CACHE_VERSION
        {
            eprintln!(
                "Cache version outdated (v{} -> v{}), rebuilding automatically...",
                stored_version, CACHE_VERSION
            );
            self.rebuild_schema(conn).await?;
            eprintln!("Cache rebuild complete.");
            log_cache_rebuilt(
                "version_mismatch",
                "automatic_schema_update",
                None,
                None,
                Some(serde_json::json!({
                    "old_version": stored_version,
                    "new_version": CACHE_VERSION,
                })),
            );
        }

        Ok(())
    }

    async fn rebuild_schema(&self, conn: &Connection) -> Result<()> {
        conn.execute("DROP TABLE IF EXISTS tickets", ()).await?;
        conn.execute("DROP TABLE IF EXISTS plans", ()).await?;
        conn.execute("DROP TABLE IF EXISTS meta", ()).await?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            (),
        )
        .await?;

        conn.execute(
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
                completion_summary TEXT,
                spawned_from TEXT,
                spawn_context TEXT,
                depth INTEGER,
                file_path TEXT,
                triaged INTEGER,
                body TEXT,
                size TEXT
            )",
            (),
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tickets_status ON tickets(status)",
            (),
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tickets_priority ON tickets(priority)",
            (),
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tickets_type ON tickets(ticket_type)",
            (),
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tickets_status_priority ON tickets(status, priority)",
            (),
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tickets_spawned_from ON tickets(spawned_from)",
            (),
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tickets_depth ON tickets(depth)",
            (),
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tickets_size ON tickets(size)",
            (),
        )
        .await?;

        #[cfg(feature = "semantic-search")]
        {
            // Add embedding column for semantic search (F32_BLOB with 384 dimensions)
            conn.execute("ALTER TABLE tickets ADD COLUMN embedding F32_BLOB(384)", ())
                .await
                .ok(); // Ignore errors if column already exists

            // Create DiskANN vector index for fast similarity search
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_tickets_embedding ON tickets(libsql_vector_idx(embedding, 'metric=cosine'))",
                (),
            )
            .await
            .ok(); // Ignore errors if index already exists
        }

        conn.execute(
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

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_plans_structure_type ON plans(structure_type)",
            (),
        )
        .await?;

        Ok(())
    }

    pub fn cache_db_path(&self) -> PathBuf {
        cache_db_path(&self.repo_hash)
    }

    pub async fn create_connection(&self) -> Result<Arc<Connection>> {
        let mut last_error = None;

        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                tokio::time::sleep(tokio::time::Duration::from_millis(BASE_RETRY_DELAY_MS)).await;
            }

            match self.db.connect() {
                Ok(conn) => {
                    if let Err(e) = conn.busy_timeout(BUSY_TIMEOUT) {
                        last_error = Some(e);
                        continue;
                    }
                    return Ok(Arc::new(conn));
                }
                Err(e) => {
                    last_error = Some(e);
                }
            }
        }

        if let Some(turso_err) = last_error {
            Err(CacheError::from(turso_err))
        } else {
            Err(CacheError::CacheOther(
                "Unknown error creating cache connection".to_string(),
            ))
        }
    }

    /// Update the embedding for a specific ticket.
    ///
    /// This method updates the embedding column for a ticket in the cache.
    /// It's used during embedding regeneration when model versions change.
    ///
    /// # Arguments
    /// * `ticket_id` - The ID of the ticket to update
    /// * `embedding` - The embedding vector to store
    #[cfg(feature = "semantic-search")]
    pub async fn update_ticket_embedding(&self, ticket_id: &str, embedding: &[f32]) -> Result<()> {
        let blob = embedding_to_blob(embedding);
        let conn = self.create_connection().await?;
        conn.execute(
            "UPDATE tickets SET embedding = ?1 WHERE ticket_id = ?2",
            (blob, ticket_id),
        )
        .await?;
        Ok(())
    }
}

/// Convert embedding vector to byte blob for storage.
/// Each f32 is serialized as 4 little-endian bytes.
#[cfg(feature = "semantic-search")]
fn embedding_to_blob(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
}

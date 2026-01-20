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

use super::paths::{cache_db_path, cache_dir, repo_hash};

/// Busy timeout for SQLite operations when multiple processes access the cache.
/// This allows concurrent janus processes to wait for locks rather than failing immediately.
const BUSY_TIMEOUT: Duration = Duration::from_millis(500);

/// Current cache schema version. Increment when schema changes.
pub(crate) const CACHE_VERSION: &str = "6";

pub struct TicketCache {
    pub(crate) db: Arc<Database>,
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
        let db = Arc::new(Builder::new_local(&db_path_str).build().await?);

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
                return Self::open().await;
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
                depth INTEGER
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
            return Err(CacheError::CacheVersionMismatch {
                expected: CACHE_VERSION.to_string(),
                found: stored_version,
            });
        }

        Ok(())
    }

    pub fn cache_db_path(&self) -> PathBuf {
        cache_db_path(&self.repo_hash)
    }

    pub async fn create_connection(&self) -> Result<Arc<Connection>> {
        let mut last_error = None;

        for attempt in 0..3 {
            if attempt > 0 {
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
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

        let error_msg = last_error
            .map(|e| e.to_string())
            .unwrap_or_else(|| "Unknown error".to_string());
        Err(CacheError::CacheAccessFailed(
            self.cache_db_path(),
            error_msg,
        ))
    }
}

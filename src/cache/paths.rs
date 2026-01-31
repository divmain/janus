//! Path utilities for the cache module.
//!
//! This module provides functions for determining cache file locations.
//! The cache is stored locally within the `.janus` directory of each repository.

use std::path::PathBuf;

use super::database::CACHE_VERSION;

/// Get the path to the cache database file.
///
/// The cache is stored at `.janus/cache-v{VERSION}.db` where VERSION includes
/// the semantic search suffix when that feature is enabled. This ensures:
/// - Each repo has its own isolated cache
/// - Different feature builds don't conflict (e.g., semantic vs non-semantic)
/// - Version changes are handled gracefully without constant rebuilds
pub fn cache_db_path() -> PathBuf {
    PathBuf::from(".janus").join(format!("cache-v{}.db", CACHE_VERSION))
}

/// Delete the cache database and all associated WAL/SHM files.
///
/// SQLite in WAL mode creates additional files alongside the main database:
/// - `.db-wal` - Write-ahead log file
/// - `.db-shm` - Shared memory file
///
/// This function removes all of them to ensure a clean state.
pub fn delete_cache_files(db_path: &PathBuf) -> std::io::Result<()> {
    // Delete main database file
    if db_path.exists() {
        std::fs::remove_file(db_path)?;
    }

    // Delete WAL file
    let wal_path = db_path.with_extension("db-wal");
    if wal_path.exists() {
        std::fs::remove_file(&wal_path)?;
    }

    // Delete SHM file
    let shm_path = db_path.with_extension("db-shm");
    if shm_path.exists() {
        std::fs::remove_file(&shm_path)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_db_path_format() {
        let path = cache_db_path();

        // Should be in .janus directory
        assert!(path.starts_with(".janus"));

        // Should have .db extension
        assert_eq!(path.extension().unwrap().to_str().unwrap(), "db");

        // Should contain version number
        let filename = path.file_name().unwrap().to_str().unwrap();
        assert!(filename.starts_with("cache-v"));

        // Check feature-specific naming
        #[cfg(feature = "semantic-search")]
        assert!(filename.contains("-semantic"));

        #[cfg(not(feature = "semantic-search"))]
        assert!(!filename.contains("-semantic"));
    }

    #[test]
    fn test_delete_cache_files() {
        let temp = tempfile::TempDir::new().unwrap();
        let db_path = temp.path().join("test.db");
        let wal_path = temp.path().join("test.db-wal");
        let shm_path = temp.path().join("test.db-shm");

        // Create the files
        std::fs::write(&db_path, "db").unwrap();
        std::fs::write(&wal_path, "wal").unwrap();
        std::fs::write(&shm_path, "shm").unwrap();

        assert!(db_path.exists());
        assert!(wal_path.exists());
        assert!(shm_path.exists());

        // Delete them
        delete_cache_files(&db_path).unwrap();

        assert!(!db_path.exists());
        assert!(!wal_path.exists());
        assert!(!shm_path.exists());
    }

    #[test]
    fn test_delete_cache_files_partial() {
        let temp = tempfile::TempDir::new().unwrap();
        let db_path = temp.path().join("test.db");

        // Only create the main file, not WAL/SHM
        std::fs::write(&db_path, "db").unwrap();

        // Should succeed even if WAL/SHM don't exist
        delete_cache_files(&db_path).unwrap();

        assert!(!db_path.exists());
    }

    #[test]
    fn test_delete_cache_files_none_exist() {
        let temp = tempfile::TempDir::new().unwrap();
        let db_path = temp.path().join("nonexistent.db");

        // Should succeed even if nothing exists
        delete_cache_files(&db_path).unwrap();
    }
}

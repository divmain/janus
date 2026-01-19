//! Path utilities for the cache module.
//!
//! This module provides functions for determining cache directory locations
//! and computing repository-specific paths.

use base64::Engine;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

/// Get the cache directory for Janus.
///
/// Uses the XDG cache directory on Linux/macOS.
/// Creates the directory if it doesn't exist.
pub fn cache_dir() -> PathBuf {
    let proj_dirs = directories::ProjectDirs::from("com", "divmain", "janus")
        .expect("cannot determine cache directory");
    let cache_dir = proj_dirs.cache_dir().to_path_buf();

    if !cache_dir.exists()
        && let Err(e) = fs::create_dir_all(&cache_dir)
    {
        eprintln!(
            "Warning: failed to create cache directory '{}': {}",
            cache_dir.display(),
            e
        );
    }

    cache_dir
}

/// Compute a hash for a repository path.
///
/// This creates a unique identifier for each repository, used to isolate
/// cache databases per-repository.
pub fn repo_hash(repo_path: &Path) -> String {
    let canonical_path = repo_path
        .canonicalize()
        .unwrap_or_else(|_| repo_path.to_path_buf());

    let hash = Sha256::digest(canonical_path.to_string_lossy().as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&hash[..16])
}

/// Get the path to the cache database file for a given repository hash.
pub fn cache_db_path(repo_hash: &str) -> PathBuf {
    cache_dir().join(format!("{}.db", repo_hash))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

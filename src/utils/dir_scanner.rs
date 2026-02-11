use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Find all markdown files in a directory
///
/// Returns a vector of filenames (e.g., "j-a1b2.md") found in the directory.
/// Returns an empty vector if the directory doesn't exist.
/// Returns an error if the directory exists but cannot be read (permission denied, I/O error, etc.).
///
/// # Arguments
///
/// * `dir_path` - Path to the directory to scan
///
/// # Example
///
/// ```no_run
/// use janus::utils::dir_scanner::find_markdown_files;
/// use std::path::Path;
///
/// let files = find_markdown_files(Path::new(".janus/items"))?;
/// for file in files {
///     println!("Found: {}", file);
/// }
/// # Ok::<(), janus::error::JanusError>(())
/// ```
pub fn find_markdown_files<P: AsRef<Path>>(dir_path: P) -> Result<Vec<String>, std::io::Error> {
    find_markdown_files_from_path(dir_path.as_ref())
}

/// Find all markdown files in a directory (PathBuf variant)
///
/// This is an alias for `find_markdown_files` that explicitly takes a Path reference.
/// Useful when working with PathBuf instances from JANUS_ROOT-aware functions.
pub fn find_markdown_files_from_path(dir_path: &Path) -> Result<Vec<String>, std::io::Error> {
    match fs::read_dir(dir_path) {
        Ok(entries) => Ok(entries
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.ends_with(".md") {
                    Some(name)
                } else {
                    None
                }
            })
            .collect()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e),
    }
}

/// Get the modification time of a file
///
/// Returns the last modified time of the file, or None if the file doesn't exist
/// or metadata can't be read.
///
/// # Arguments
///
/// * `path` - Path to the file
///
/// # Example
///
/// ```no_run
/// use janus::utils::dir_scanner::get_file_mtime;
/// use std::path::Path;
///
/// if let Some(mtime) = get_file_mtime(Path::new("ticket.md")) {
///     println!("Last modified: {:?}", mtime);
/// }
/// ```
pub fn get_file_mtime<P: AsRef<Path>>(path: P) -> Option<SystemTime> {
    fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

/// Scan directory and collect file paths with their modification times
///
/// Returns a vector of tuples containing (filename, full_path, modification_time).
/// Files without accessible modification times are excluded from the results.
/// Returns an empty vector if the directory doesn't exist.
/// Returns an error if the directory exists but cannot be read (permission denied, I/O error, etc.).
///
/// # Arguments
///
/// * `dir_path` - Path to the directory to scan
///
/// # Example
///
/// ```no_run
/// use janus::utils::dir_scanner::scan_with_mtime;
/// use std::path::Path;
///
/// let files = scan_with_mtime(Path::new(".janus/items"))?;
/// for (filename, path, mtime) in files {
///     println!("{}: {:?} (modified: {:?})", filename, path, mtime);
/// }
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn scan_with_mtime<P: AsRef<Path>>(
    dir_path: P,
) -> Result<Vec<(String, PathBuf, SystemTime)>, std::io::Error> {
    let dir_path = dir_path.as_ref();
    let files = find_markdown_files(dir_path)?;
    Ok(files
        .into_iter()
        .filter_map(|filename| {
            let path = dir_path.join(&filename);
            get_file_mtime(&path).map(|mtime| (filename, path, mtime))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_find_markdown_files_empty_dir() {
        let temp_dir = TempDir::new().unwrap();
        let files = find_markdown_files(temp_dir.path()).unwrap();
        assert_eq!(files.len(), 0);
    }

    #[test]
    fn test_find_markdown_files() {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path();

        // Create some test files
        fs::write(dir_path.join("ticket1.md"), "content").unwrap();
        fs::write(dir_path.join("ticket2.md"), "content").unwrap();
        fs::write(dir_path.join("not-markdown.txt"), "content").unwrap();
        fs::write(dir_path.join("readme.MD"), "content").unwrap(); // wrong case

        let mut files = find_markdown_files(dir_path).unwrap();
        files.sort();

        assert_eq!(files.len(), 2);
        assert!(files.contains(&"ticket1.md".to_string()));
        assert!(files.contains(&"ticket2.md".to_string()));
    }

    #[test]
    fn test_find_markdown_files_nonexistent_dir() {
        let files = find_markdown_files("/nonexistent/directory").unwrap();
        assert_eq!(files.len(), 0);
    }

    #[test]
    fn test_get_file_mtime_existing_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");
        fs::write(&file_path, "content").unwrap();

        let mtime = get_file_mtime(&file_path);
        assert!(mtime.is_some());
    }

    #[test]
    fn test_get_file_mtime_nonexistent_file() {
        let mtime = get_file_mtime("/nonexistent/file.md");
        assert!(mtime.is_none());
    }

    #[test]
    fn test_scan_with_mtime() {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path();

        // Create test files
        fs::write(dir_path.join("ticket1.md"), "content1").unwrap();
        fs::write(dir_path.join("ticket2.md"), "content2").unwrap();

        let results = scan_with_mtime(dir_path).unwrap();
        assert_eq!(results.len(), 2);

        for (filename, path, mtime) in results {
            assert!(filename.ends_with(".md"));
            assert!(path.exists());
            assert!(mtime.elapsed().is_ok()); // mtime is in the past
        }
    }

    #[test]
    fn test_scan_with_mtime_empty_dir() {
        let temp_dir = TempDir::new().unwrap();
        let results = scan_with_mtime(temp_dir.path()).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_find_markdown_files_notfound_returns_empty() {
        // Test that NotFound error returns Ok(Vec::new())
        let result = find_markdown_files("/path/that/does/not/exist/at/all");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[test]
    #[cfg(unix)] // Permission tests are platform-specific
    fn test_find_markdown_files_permission_denied_propagates_error() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path().join("no_read_perms");
        fs::create_dir(&dir_path).unwrap();

        // Create a file inside to ensure directory is not empty
        fs::write(dir_path.join("test.md"), "content").unwrap();

        // Remove read permissions
        let mut perms = fs::metadata(&dir_path).unwrap().permissions();
        perms.set_mode(0o000);
        fs::set_permissions(&dir_path, perms).unwrap();

        // Should return an error, not Ok(Vec::new())
        let result = find_markdown_files(&dir_path);

        // Restore permissions so cleanup can work
        let mut perms = fs::metadata(&dir_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&dir_path, perms).unwrap();

        // Verify the error is a permission denied error
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
    }
}

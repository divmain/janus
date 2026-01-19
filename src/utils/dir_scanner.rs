use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// A reusable utility for scanning directories and finding markdown files
///
/// This provides common functionality for both ticket and plan directory scanning,
/// eliminating duplication across the codebase.
pub struct DirScanner;

impl DirScanner {
    /// Find all markdown files in a directory
    ///
    /// Returns a vector of filenames (e.g., "j-a1b2.md") found in the directory.
    /// Returns an empty vector if the directory doesn't exist or can't be read.
    ///
    /// # Arguments
    ///
    /// * `dir_path` - Path to the directory to scan
    ///
    /// # Example
    ///
    /// ```no_run
    /// use janus::utils::DirScanner;
    /// use std::path::Path;
    ///
    /// let files = DirScanner::find_markdown_files(Path::new(".janus/items"));
    /// for file in files {
    ///     println!("Found: {}", file);
    /// }
    /// ```
    pub fn find_markdown_files<P: AsRef<Path>>(dir_path: P) -> Vec<String> {
        fs::read_dir(dir_path)
            .ok()
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter_map(|e| {
                        let name = e.file_name().to_string_lossy().into_owned();
                        if name.ends_with(".md") {
                            Some(name)
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
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
    /// use janus::utils::DirScanner;
    /// use std::path::Path;
    ///
    /// if let Some(mtime) = DirScanner::get_file_mtime(Path::new("ticket.md")) {
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
    ///
    /// # Arguments
    ///
    /// * `dir_path` - Path to the directory to scan
    ///
    /// # Example
    ///
    /// ```no_run
    /// use janus::utils::DirScanner;
    /// use std::path::Path;
    ///
    /// let files = DirScanner::scan_with_mtime(Path::new(".janus/items"));
    /// for (filename, path, mtime) in files {
    ///     println!("{}: {:?} (modified: {:?})", filename, path, mtime);
    /// }
    /// ```
    pub fn scan_with_mtime<P: AsRef<Path>>(dir_path: P) -> Vec<(String, PathBuf, SystemTime)> {
        let dir_path = dir_path.as_ref();
        Self::find_markdown_files(dir_path)
            .into_iter()
            .filter_map(|filename| {
                let path = dir_path.join(&filename);
                Self::get_file_mtime(&path).map(|mtime| (filename, path, mtime))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_find_markdown_files_empty_dir() {
        let temp_dir = TempDir::new().unwrap();
        let files = DirScanner::find_markdown_files(temp_dir.path());
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

        let mut files = DirScanner::find_markdown_files(dir_path);
        files.sort();

        assert_eq!(files.len(), 2);
        assert!(files.contains(&"ticket1.md".to_string()));
        assert!(files.contains(&"ticket2.md".to_string()));
    }

    #[test]
    fn test_find_markdown_files_nonexistent_dir() {
        let files = DirScanner::find_markdown_files("/nonexistent/directory");
        assert_eq!(files.len(), 0);
    }

    #[test]
    fn test_get_file_mtime_existing_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");
        fs::write(&file_path, "content").unwrap();

        let mtime = DirScanner::get_file_mtime(&file_path);
        assert!(mtime.is_some());
    }

    #[test]
    fn test_get_file_mtime_nonexistent_file() {
        let mtime = DirScanner::get_file_mtime("/nonexistent/file.md");
        assert!(mtime.is_none());
    }

    #[test]
    fn test_scan_with_mtime() {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path();

        // Create test files
        fs::write(dir_path.join("ticket1.md"), "content1").unwrap();
        fs::write(dir_path.join("ticket2.md"), "content2").unwrap();

        let results = DirScanner::scan_with_mtime(dir_path);
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
        let results = DirScanner::scan_with_mtime(temp_dir.path());
        assert_eq!(results.len(), 0);
    }
}

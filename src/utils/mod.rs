pub mod dir_scanner;
pub mod id;
pub mod io;
pub mod text;
pub mod validation;

// Re-export text utilities for backward compatibility
pub use text::{truncate_string, wrap_text_lines};

// Re-export ID utilities for backward compatibility
pub use id::{
    generate_hash, generate_id_with_custom_prefix, generate_unique_id_with_prefix, generate_uuid,
    validate_prefix,
};

// Re-export IO utilities for backward compatibility
pub use io::{is_stdin_tty, open_in_editor, read_stdin};

use jiff::Timestamp;
use regex::Regex;
use std::path::Path;
use std::process::Command;
use std::sync::LazyLock;

use crate::error::{JanusError, Result};
use crate::types::{janus_root, tickets_items_dir};
use std::fs;

#[cfg(test)]
use std::path::PathBuf;

// Re-export dir_scanner functions for convenience
pub use dir_scanner::{
    find_markdown_files, find_markdown_files_from_path, get_file_mtime, scan_with_mtime,
};

/// Format a path for display by making it relative to the janus root directory.
///
/// This is used for user-facing output (error messages, CLI output) to avoid
/// exposing sensitive directory structures like usernames and internal paths.
///
/// # Arguments
/// * `path` - The path to format
///
/// # Returns
/// A String containing the relative path, or the original path if it cannot be made relative
///
/// # Example
/// ```rust,ignore
/// let full_path = PathBuf::from("/home/user/project/.janus/items/ticket.md");
/// let display = format_relative_path(&full_path);
/// // display == ".janus/items/ticket.md"
/// ```
pub fn format_relative_path(path: &Path) -> String {
    path.strip_prefix(janus_root())
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

/// Ensure the tickets directory exists
pub fn ensure_dir() -> std::io::Result<()> {
    fs::create_dir_all(tickets_items_dir())?;
    ensure_gitignore();
    Ok(())
}

/// Default contents for the `.janus/.gitignore` file.
///
/// Protects sensitive configuration (API tokens) and large binary files
/// (embeddings) from accidental inclusion in version control.
const GITIGNORE_CONTENTS: &str = "config.yaml\nembeddings/\n";

/// Ensure a `.gitignore` exists in the `.janus/` root directory.
///
/// Creates the file with default entries (config.yaml, embeddings/) only if
/// it does not already exist. This avoids overwriting user customizations.
pub fn ensure_gitignore() {
    let gitignore_path = janus_root().join(".gitignore");
    if !gitignore_path.exists() {
        // Best-effort: don't fail ticket operations if gitignore can't be written
        let _ = fs::write(&gitignore_path, GITIGNORE_CONTENTS);
    }
}

/// Extract an ID from a file path's stem
///
/// This is a shared utility function used by both ticket and plan modules to extract
/// the ID from a file path. It gets the file stem, converts it to a string, checks if
/// it's empty, and returns an error with a formatted message if invalid.
///
/// # Arguments
///
/// * `file_path` - The path to extract the ID from
/// * `entity_type` - The type of entity (e.g., "ticket", "plan") for error messages
///
/// # Returns
///
/// * `Ok(String)` - The extracted ID
/// * `Err(JanusError::InvalidFormat)` - If the path has no valid file stem
pub fn extract_id_from_path(file_path: &Path, entity_type: &str) -> Result<String> {
    let id = file_path
        .file_stem()
        .and_then(|s| {
            let id = s.to_string_lossy().into_owned();
            if id.is_empty() { None } else { Some(id) }
        })
        .ok_or_else(|| {
            JanusError::InvalidFormat(format!(
                "Invalid {} file path: {}",
                entity_type,
                format_relative_path(file_path)
            ))
        })?;

    // Validate the extracted ID
    validate_identifier(&id, entity_type)?;

    Ok(id)
}

/// Validate an identifier string (alphanumeric, hyphens, and underscores only)
///
/// This is a generic validation function used by both ticket ID and prefix validators.
/// It trims whitespace, checks for non-empty strings, and validates that only
/// alphanumeric characters, hyphens, and underscores are present.
///
/// # Arguments
///
/// * `s` - The string to validate
/// * `name` - A descriptive name for the identifier (used in error messages)
///
/// # Returns
///
/// * `Ok(String)` - The trimmed, validated identifier
/// * `Err(JanusError)` - An error describing what went wrong
pub fn validate_identifier(s: &str, name: &str) -> Result<String> {
    let trimmed = s.trim();

    if trimmed.is_empty() {
        return Err(JanusError::ValidationEmpty(name.to_string()));
    }

    if !trimmed
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(JanusError::ValidationInvalidCharacters(
            name.to_string(),
            trimmed.to_string(),
        ));
    }

    Ok(trimmed.to_string())
}

/// Get the git user.name config value
pub fn get_git_user_name() -> Option<String> {
    Command::new("git")
        .args(["config", "user.name"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            } else {
                None
            }
        })
}

/// Get current ISO date string (without milliseconds)
pub fn iso_date() -> String {
    let now = Timestamp::now();
    // Format as ISO 8601 without fractional seconds
    now.strftime("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Validate that a string is safe to use as a filename on the current OS
///
/// This function validates filename safety by checking:
/// - No invalid characters for the target OS (Windows-specific on Windows, general on Unix)
/// - No empty or whitespace-only names
/// - Not a reserved device name (Windows only)
/// - Within reasonable length limits
/// - No path traversal patterns
///
/// # Arguments
///
/// * `name` - The filename to validate (without path)
///
/// # Returns
///
/// * `Ok(())` - If the filename is safe
/// * `Err(JanusError::InvalidFormat)` - If the filename is unsafe
pub fn validate_filename(name: &str) -> Result<()> {
    let trimmed = name.trim();

    if trimmed.is_empty() {
        return Err(JanusError::InvalidFormat(
            "Filename cannot be empty or only whitespace".to_string(),
        ));
    }

    // Reject path traversal patterns
    if trimmed.contains("..") || trimmed.starts_with('/') || trimmed.starts_with('\\') {
        return Err(JanusError::InvalidFormat(
            "Filename cannot contain path traversal patterns".to_string(),
        ));
    }

    // OS-specific validation
    #[cfg(target_os = "windows")]
    {
        // Windows reserved device names (case-insensitive)
        const RESERVED_NAMES: &[&str] = &[
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
            "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ];

        let upper = trimmed.to_uppercase();
        if RESERVED_NAMES.contains(&upper.as_str()) {
            return Err(JanusError::InvalidFormat(format!(
                "Filename '{}' is a reserved device name",
                trimmed
            )));
        }

        // Windows invalid characters
        const INVALID_CHARS: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
        if trimmed.chars().any(|c| INVALID_CHARS.contains(&c)) {
            return Err(JanusError::InvalidFormat(format!(
                "Filename '{}' contains invalid characters for Windows",
                trimmed
            )));
        }

        // Leading/trailing dots/spaces are problematic on Windows
        if trimmed.starts_with('.')
            || trimmed.starts_with(' ')
            || trimmed.ends_with('.')
            || trimmed.ends_with(' ')
        {
            return Err(JanusError::InvalidFormat(format!(
                "Filename '{}' cannot start or end with a dot or space",
                trimmed
            )));
        }

        // Windows MAX_PATH is 260, but allow reasonable margin
        if trimmed.len() > 255 {
            return Err(JanusError::InvalidFormat(format!(
                "Filename '{}' exceeds maximum length (255 characters)",
                trimmed
            )));
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Unix invalid characters (null and /)
        if trimmed.contains('\0') || trimmed.contains('/') {
            return Err(JanusError::InvalidFormat(format!(
                "Filename '{trimmed}' contains invalid characters"
            )));
        }

        // Unix filename length limit (usually 255 bytes)
        if trimmed.len() > 255 {
            return Err(JanusError::InvalidFormat(format!(
                "Filename '{trimmed}' exceeds maximum length (255 characters)"
            )));
        }
    }

    Ok(())
}

/// Regex to parse priority filter (e.g., "p0", "p1", "P2")
static PRIORITY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bp([0-4])\b").expect("priority filter regex should be valid")
});

/// Regex to strip priority shorthand from query
static PRIORITY_SHORTHAND_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bp[0-4]\b").expect("priority shorthand regex should be valid")
});

/// Parse a priority filter from the query (e.g., "p0", "p1", "P2")
pub fn parse_priority_filter(query: &str) -> Option<u8> {
    PRIORITY_RE
        .captures(query)
        .and_then(|c| c.get(1)?.as_str().parse().ok())
}

/// Strip priority shorthand from the query for fuzzy matching
pub fn strip_priority_shorthand(query: &str) -> String {
    PRIORITY_SHORTHAND_RE
        .replace_all(query, "")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::JanusRootGuard;

    #[test]
    fn test_iso_date_format() {
        let date = iso_date();
        // Should match ISO 8601 format
        assert!(date.contains('T'));
        assert!(date.ends_with('Z'));
    }

    #[test]
    fn test_extract_id_from_path_valid_ticket() {
        let path = PathBuf::from("/path/to/j-a1b2.md");
        let id = extract_id_from_path(&path, "ticket").unwrap();
        assert_eq!(id, "j-a1b2");
    }

    #[test]
    fn test_extract_id_from_path_valid_plan() {
        let path = PathBuf::from(".janus/plans/plan-abc123.md");
        let id = extract_id_from_path(&path, "plan").unwrap();
        assert_eq!(id, "plan-abc123");
    }

    #[test]
    fn test_extract_id_from_path_invalid_empty() {
        let path = PathBuf::from("");
        let result = extract_id_from_path(&path, "ticket");
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::InvalidFormat(msg) => {
                assert!(msg.contains("Invalid ticket file path"));
            }
            _ => panic!("Expected InvalidFormat error"),
        }
    }

    #[test]
    fn test_extract_id_from_path_invalid_no_stem() {
        let path = PathBuf::from("/");
        let result = extract_id_from_path(&path, "plan");
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::InvalidFormat(msg) => {
                assert!(msg.contains("Invalid plan file path"));
            }
            _ => panic!("Expected InvalidFormat error"),
        }
    }

    #[test]
    fn test_extract_id_from_path_error_message_includes_entity_type() {
        let path = PathBuf::from("");

        // Test ticket error message
        let ticket_result = extract_id_from_path(&path, "ticket");
        assert!(ticket_result.is_err());
        if let Err(JanusError::InvalidFormat(msg)) = ticket_result {
            assert!(msg.contains("ticket"));
            assert!(!msg.contains("plan"));
        }

        // Test plan error message
        let plan_result = extract_id_from_path(&path, "plan");
        assert!(plan_result.is_err());
        if let Err(JanusError::InvalidFormat(msg)) = plan_result {
            assert!(msg.contains("plan"));
            assert!(!msg.contains("ticket"));
        }
    }

    #[test]
    fn test_validate_filename_valid_names() {
        let valid_names = vec![
            "task-a1b2",
            "my-ticket-123",
            "feature_xyz",
            "bug-Test",
            "a",
            "abc123",
            "my_file",
        ];

        for name in valid_names {
            assert!(
                validate_filename(name).is_ok(),
                "Filename '{name}' should be valid"
            );
        }
    }

    #[test]
    fn test_validate_filename_empty() {
        let result = validate_filename("");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn test_validate_filename_whitespace_only() {
        let result = validate_filename("   ");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_filename_path_traversal() {
        let invalid_names = vec!["../etc/passwd", "..", "../", "/etc/passwd", "dir/.."];

        for name in invalid_names {
            let result = validate_filename(name);
            assert!(
                result.is_err(),
                "Filename '{name}' should be rejected due to path traversal"
            );
            assert!(result.unwrap_err().to_string().contains("path traversal"));
        }
    }

    #[test]
    fn test_validate_filename_too_long() {
        let long_name = "a".repeat(256);
        let result = validate_filename(&long_name);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("maximum length"));
    }

    #[test]
    fn test_validate_filename_max_length_boundary() {
        let max_name = "a".repeat(255);
        assert!(validate_filename(&max_name).is_ok());
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_validate_filename_windows_invalid_chars() {
        let invalid_names = vec![
            "test<file",
            "test>file",
            "test:file",
            "test\"file",
            "test/file",
            "test\\file",
            "test|file",
            "test?file",
            "test*file",
        ];

        for name in invalid_names {
            let result = validate_filename(name);
            assert!(
                result.is_err(),
                "Filename '{}' should be rejected on Windows",
                name
            );
            assert!(result.unwrap_err().to_string().contains("Windows"));
        }
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_validate_filename_windows_reserved_names() {
        let reserved_names = vec![
            "CON", "con", "Con", "PRN", "prn", "AUX", "aux", "NUL", "nul", "COM1", "com1", "Com1",
            "LPT1", "lpt1",
        ];

        for name in reserved_names {
            let result = validate_filename(name);
            assert!(
                result.is_err(),
                "Filename '{}' should be rejected as reserved",
                name
            );
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("reserved device name")
            );
        }
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_validate_filename_windows_leading_trailing_dots_spaces() {
        let invalid_names = vec![".test", "test.", " test", "test ", ".", " "];

        for name in invalid_names {
            let result = validate_filename(name);
            assert!(result.is_err(), "Filename '{}' should be rejected", name);
        }
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn test_validate_filename_unix_invalid_chars() {
        let invalid_names = vec!["test\0file", "test/file", "test\x00"];

        for name in invalid_names {
            let result = validate_filename(name);
            assert!(
                result.is_err(),
                "Filename '{name}' should be rejected on Unix"
            );
        }
    }

    #[test]
    fn test_ensure_gitignore_creates_file() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_gitignore_create");
        let janus_dir = repo_path.join(".janus");
        std::fs::create_dir_all(&janus_dir).unwrap();
        let _guard = JanusRootGuard::new(&janus_dir);

        // Ensure .gitignore doesn't exist yet
        let gitignore_path = janus_dir.join(".gitignore");
        assert!(!gitignore_path.exists());

        // Call ensure_gitignore
        ensure_gitignore();

        // Verify the file was created with expected contents
        assert!(gitignore_path.exists());
        let contents = std::fs::read_to_string(&gitignore_path).unwrap();
        assert!(contents.contains("config.yaml"));
        assert!(contents.contains("embeddings/"));
    }

    #[test]
    fn test_ensure_gitignore_does_not_overwrite_existing() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_gitignore_no_overwrite");
        let janus_dir = repo_path.join(".janus");
        std::fs::create_dir_all(&janus_dir).unwrap();
        let _guard = JanusRootGuard::new(&janus_dir);

        // Create a custom .gitignore
        let gitignore_path = janus_dir.join(".gitignore");
        let custom_content = "# Custom gitignore\nconfig.yaml\nembeddings/\nmy-custom-entry\n";
        std::fs::write(&gitignore_path, custom_content).unwrap();

        // Call ensure_gitignore
        ensure_gitignore();

        // Verify the file was NOT overwritten
        let contents = std::fs::read_to_string(&gitignore_path).unwrap();
        assert_eq!(contents, custom_content);
    }

    #[test]
    fn test_ensure_dir_creates_gitignore() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_ensure_dir_gitignore");
        let janus_dir = repo_path.join(".janus");
        std::fs::create_dir_all(&janus_dir).unwrap();
        let _guard = JanusRootGuard::new(&janus_dir);

        // Call ensure_dir which creates .janus/items/
        ensure_dir().unwrap();

        // Verify .gitignore was also created
        let gitignore_path = janus_dir.join(".gitignore");
        assert!(gitignore_path.exists());
        let contents = std::fs::read_to_string(&gitignore_path).unwrap();
        assert!(contents.contains("config.yaml"));
        assert!(contents.contains("embeddings/"));
    }
}

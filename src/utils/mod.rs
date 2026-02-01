pub mod dir_scanner;

use jiff::Timestamp;
use rand::Rng;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, BufRead};
use std::path::Path;
use std::process::Command;
use uuid::Uuid;

use crate::error::{JanusError, Result};
use crate::types::{janus_root, tickets_items_dir};

/// Ensure the parent directory of a path exists, creating it if necessary.
///
/// This is a DRY helper that encapsulates the common pattern of:
/// - Checking if a path has a parent
/// - Checking if that parent exists
/// - Creating the directory (and all ancestors) if not
///
/// # Arguments
/// * `path` - The path whose parent directory should exist
///
/// # Returns
/// * `Ok(())` - If the parent directory exists or was created successfully
/// * `Err(JanusError)` - If directory creation failed
pub fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent).map_err(|e| {
            JanusError::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to create directory at {}: {}",
                    format_relative_path(parent),
                    e
                ),
            ))
        })?;
    }
    Ok(())
}

#[cfg(test)]
use std::path::PathBuf;

// Re-export DirScanner for convenience
pub use dir_scanner::DirScanner;

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
pub fn ensure_dir() -> io::Result<()> {
    fs::create_dir_all(tickets_items_dir())
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
    file_path
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
        })
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

/// Generate a UUID v4
pub fn generate_uuid() -> String {
    Uuid::new_v4().to_string()
}

/// Generate a unique ticket ID with a custom prefix
pub fn generate_id_with_custom_prefix(custom_prefix: Option<&str>) -> Result<String> {
    match custom_prefix {
        Some(prefix) if !prefix.is_empty() => {
            validate_prefix(prefix)?;
            Ok(generate_unique_id_with_prefix(prefix))
        }
        _ => Ok(generate_unique_id_with_prefix("task")),
    }
}

/// Validate that a prefix is not reserved and is valid
pub fn validate_prefix(prefix: &str) -> Result<()> {
    const RESERVED_PREFIXES: &[&str] = &["plan"];

    if RESERVED_PREFIXES.contains(&prefix) {
        return Err(JanusError::InvalidPrefix(
            prefix.to_string(),
            format!(
                "Prefix '{}' is reserved and cannot be used for tickets",
                prefix
            ),
        ));
    }

    // Use the generic identifier validator, preserving specific error context
    validate_identifier(prefix, "Prefix").map_err(|e| match e {
        JanusError::ValidationEmpty(_) => JanusError::InvalidPrefix(
            prefix.to_string(),
            "Prefix cannot be empty or only whitespace".to_string(),
        ),
        JanusError::ValidationInvalidCharacters(_, value) => JanusError::InvalidPrefix(
            prefix.to_string(),
            format!(
                "Prefix '{}' contains invalid characters. Use only letters, numbers, hyphens, and underscores",
                value
            ),
        ),
        other => other,
    })?;

    Ok(())
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
                "Filename '{}' contains invalid characters",
                trimmed
            )));
        }

        // Unix filename length limit (usually 255 bytes)
        if trimmed.len() > 255 {
            return Err(JanusError::InvalidFormat(format!(
                "Filename '{}' exceeds maximum length (255 characters)",
                trimmed
            )));
        }
    }

    Ok(())
}

/// Generate a unique short ID with collision checking
/// Returns a short ID that does not exist in the tickets directory
pub fn generate_unique_id_with_prefix(prefix: &str) -> String {
    const RETRIES_PER_LENGTH: u32 = 40;
    let tickets_dir = tickets_items_dir();

    for length in 4..=8 {
        for _ in 0..RETRIES_PER_LENGTH {
            let hash = generate_hash(length);
            let candidate = format!("{}-{}", prefix, hash);
            let filename = format!("{}.md", candidate);

            // Validate the ID is file-safe before checking for existence
            if validate_filename(&candidate).is_ok() && !tickets_dir.join(&filename).exists() {
                return candidate;
            }
        }
    }

    panic!(
        "Failed to generate unique ID after trying hash lengths 4-8 with {} retries each",
        RETRIES_PER_LENGTH
    );
}

/// Generate a random hex hash of the specified length
///
/// Uses SHA-256 to hash random bytes and returns the first `length` hex characters.
/// This is used for generating unique IDs for tickets and plans.
pub fn generate_hash(length: usize) -> String {
    let random_bytes: [u8; 16] = rand::rng().random();
    let mut hasher = Sha256::new();
    hasher.update(random_bytes);
    let hash = format!("{:x}", hasher.finalize());
    hash[..length].to_string()
}

/// Get current ISO date string (without milliseconds)
pub fn iso_date() -> String {
    let now = Timestamp::now();
    // Format as ISO 8601 without fractional seconds
    now.strftime("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Read all input from stdin (for piped input)
pub fn read_stdin() -> io::Result<String> {
    let stdin = io::stdin();
    let mut lines = Vec::new();
    for line in stdin.lock().lines() {
        lines.push(line?);
    }
    Ok(lines.join("\n").trim().to_string())
}

/// Check if stdin is a TTY (interactive)
pub fn is_stdin_tty() -> bool {
    atty_check()
}

/// Truncate a string to a maximum length, handling multi-byte characters properly.
/// Appends "..." if truncated.
pub fn truncate_string(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else if max_len <= 3 {
        s.chars().take(max_len).collect()
    } else {
        let truncated: String = s.chars().take(max_len.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}

/// Wrap text into multiple lines, breaking at word boundaries.
///
/// Returns up to `max_lines` lines, with "..." appended to the last line
/// if the text was truncated. Each line will be at most `width` characters.
///
/// If a single word is longer than `width`, it will be broken mid-word.
pub fn wrap_text_lines(text: &str, width: usize, max_lines: usize) -> Vec<String> {
    if width == 0 || max_lines == 0 {
        return vec![];
    }

    let text = text.trim();
    if text.is_empty() {
        return vec![];
    }

    let mut lines: Vec<String> = Vec::new();
    let mut current_line = String::new();

    for word in text.split_whitespace() {
        let word_len = word.chars().count();
        let current_len = current_line.chars().count();

        if current_len == 0 {
            // Starting a new line
            if word_len <= width {
                current_line = word.to_string();
            } else {
                // Word is longer than width, need to break it
                let mut chars = word.chars();
                while chars.as_str().chars().count() > 0 {
                    let chunk: String = chars.by_ref().take(width).collect();
                    if chunk.is_empty() {
                        break;
                    }

                    if lines.len() + 1 >= max_lines && chars.as_str().chars().count() > 0 {
                        // This is the last allowed line and there's more text
                        lines.push(truncate_string(&chunk, width));
                        return add_ellipsis_if_truncated(lines, true, width);
                    }

                    if chars.as_str().chars().count() > 0 {
                        lines.push(chunk);
                        if lines.len() >= max_lines {
                            return add_ellipsis_if_truncated(lines, true, width);
                        }
                    } else {
                        current_line = chunk;
                    }
                }
            }
        } else if current_len + 1 + word_len <= width {
            // Word fits on current line with a space
            current_line.push(' ');
            current_line.push_str(word);
        } else {
            // Need to start a new line
            lines.push(current_line);

            if lines.len() >= max_lines {
                // We've hit the max lines, and there's more text
                return add_ellipsis_if_truncated(lines, true, width);
            }

            if word_len <= width {
                current_line = word.to_string();
            } else {
                // Word is longer than width, need to break it
                current_line = String::new();
                let mut chars = word.chars();
                while chars.as_str().chars().count() > 0 {
                    let chunk: String = chars.by_ref().take(width).collect();
                    if chunk.is_empty() {
                        break;
                    }

                    if lines.len() + 1 >= max_lines && chars.as_str().chars().count() > 0 {
                        lines.push(truncate_string(&chunk, width));
                        return add_ellipsis_if_truncated(lines, true, width);
                    }

                    if chars.as_str().chars().count() > 0 {
                        lines.push(chunk);
                        if lines.len() >= max_lines {
                            return add_ellipsis_if_truncated(lines, true, width);
                        }
                    } else {
                        current_line = chunk;
                    }
                }
            }
        }
    }

    // Add the last line if non-empty
    if !current_line.is_empty() {
        lines.push(current_line);
    }

    lines
}

/// Helper to add ellipsis to the last line if text was truncated
fn add_ellipsis_if_truncated(mut lines: Vec<String>, truncated: bool, width: usize) -> Vec<String> {
    if truncated && !lines.is_empty() {
        let last_idx = lines.len() - 1;
        let last_line = &lines[last_idx];
        let last_len = last_line.chars().count();

        if last_len + 3 <= width {
            // Room for "..."
            lines[last_idx] = format!("{}...", last_line);
        } else if last_len >= 3 {
            // Need to truncate the last line to fit "..."
            let truncated: String = last_line.chars().take(width.saturating_sub(3)).collect();
            lines[last_idx] = format!("{}...", truncated);
        }
    }
    lines
}

/// Open a file in the user's preferred editor ($EDITOR, defaulting to vi)
///
/// Executes the editor through a shell to support EDITOR values with arguments
/// (e.g., "subl -w", "code --wait").
///
/// # Security Note
///
/// This function intentionally uses shell execution (`sh -c`) to interpret the
/// `$EDITOR` environment variable, which allows arbitrary command execution.
/// This is by design and follows Unix conventions used by git, mercurial, and
/// other CLI tools.
///
/// The `$EDITOR` variable is user-controlled configuration, not untrusted input.
/// If an attacker can modify a user's environment variables (e.g., via a
/// compromised `.bashrc`), they already have code execution in every shell
/// session—the editor invocation adds no additional attack surface.
///
/// The file path argument is safely passed using shell positional parameters
/// (`$1`) to prevent path-based injection.
pub fn open_in_editor(path: &Path) -> io::Result<()> {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

    let status = Command::new("sh")
        .arg("-c")
        .arg(format!("{} \"$1\"", editor))
        .arg("--")
        .arg(path)
        .status()?;

    if !status.success() {
        eprintln!("Editor exited with code {:?}", status.code());
    }

    Ok(())
}

#[cfg(unix)]
fn atty_check() -> bool {
    use std::os::unix::io::AsRawFd;
    unsafe { libc::isatty(std::io::stdin().as_raw_fd()) != 0 }
}

#[cfg(not(unix))]
fn atty_check() -> bool {
    // On non-Unix, assume it's a TTY
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn test_generate_unique_id_with_prefix_format() {
        let id = generate_unique_id_with_prefix("task");
        // Should start with prefix "task-"
        assert!(id.starts_with("task-"));
        // Should contain a dash
        assert!(id.contains('-'));
        // The hash part should be 4 characters
        let parts: Vec<&str> = id.rsplitn(2, '-').collect();
        assert_eq!(parts[0].len(), 4);
    }

    #[test]
    fn test_iso_date_format() {
        let date = iso_date();
        // Should match ISO 8601 format
        assert!(date.contains('T'));
        assert!(date.ends_with('Z'));
    }

    #[test]
    fn test_truncate_string_short() {
        assert_eq!(truncate_string("Hello", 10), "Hello");
    }

    #[test]
    fn test_truncate_string_exact() {
        assert_eq!(truncate_string("Hello", 5), "Hello");
    }

    #[test]
    fn test_truncate_string_long() {
        assert_eq!(truncate_string("Hello World", 8), "Hello...");
    }

    #[test]
    fn test_truncate_string_very_short_max() {
        assert_eq!(truncate_string("Hello World", 3), "Hel");
    }

    #[test]
    fn test_truncate_string_multibyte() {
        // Japanese text: "Hello World"
        let japanese = "こんにちは世界";
        let truncated = truncate_string(japanese, 5);
        assert_eq!(truncated, "こん...");
    }

    #[test]
    fn test_truncate_string_emoji() {
        let emoji = "Test 🎉🎊🎈 emoji";
        let truncated = truncate_string(emoji, 10);
        // Each emoji counts as 1 char, so 10 chars = "Test 🎉🎊" + "..." = 7 + 3 = 10
        assert_eq!(truncated, "Test 🎉🎊...");
    }

    #[test]
    fn test_generate_uuid_format() {
        let uuid = generate_uuid();
        // Should be a valid UUID v4 format
        let uuid_parts: Vec<&str> = uuid.split('-').collect();
        assert_eq!(uuid_parts.len(), 5);
        assert_eq!(uuid_parts[0].len(), 8);
        assert_eq!(uuid_parts[1].len(), 4);
        assert_eq!(uuid_parts[2].len(), 4);
        assert_eq!(uuid_parts[3].len(), 4);
        assert_eq!(uuid_parts[4].len(), 12);
    }

    #[test]
    fn test_generate_uuid_unique() {
        let uuid1 = generate_uuid();
        let uuid2 = generate_uuid();
        assert_ne!(uuid1, uuid2);
    }

    #[test]
    fn test_generate_id_with_custom_prefix() {
        let id = generate_id_with_custom_prefix(Some("perf")).unwrap();
        assert!(id.starts_with("perf-"));
        // Should contain a dash and 4-character hash
        let parts: Vec<&str> = id.rsplitn(2, '-').collect();
        assert_eq!(parts[0].len(), 4);
    }

    #[test]
    fn test_generate_id_without_custom_prefix() {
        let id = generate_id_with_custom_prefix(None).unwrap();
        // Should start with default prefix "task-"
        assert!(id.starts_with("task-"));
        // Should contain a dash
        assert!(id.contains('-'));
        // The hash part should be 4 characters
        let parts: Vec<&str> = id.rsplitn(2, '-').collect();
        assert_eq!(parts[0].len(), 4);
    }

    #[test]
    fn test_generate_id_with_empty_prefix_uses_default() {
        let id = generate_id_with_custom_prefix(Some("")).unwrap();
        // Should start with default prefix "task-"
        assert!(id.starts_with("task-"));
        // Should contain a dash and 4-character hash
        assert!(id.contains('-'));
        let parts: Vec<&str> = id.rsplitn(2, '-').collect();
        assert_eq!(parts[0].len(), 4);
    }

    #[test]
    fn test_generate_id_with_hyphen_prefix() {
        let id = generate_id_with_custom_prefix(Some("my-prefix")).unwrap();
        assert!(id.starts_with("my-prefix-"));
    }

    #[test]
    fn test_generate_id_with_underscore_prefix() {
        let id = generate_id_with_custom_prefix(Some("my_prefix")).unwrap();
        assert!(id.starts_with("my_prefix-"));
    }

    #[test]
    fn test_generate_id_with_numeric_prefix() {
        let id = generate_id_with_custom_prefix(Some("abc123")).unwrap();
        assert!(id.starts_with("abc123-"));
    }

    #[test]
    fn test_reserved_prefix_rejected() {
        let result = generate_id_with_custom_prefix(Some("plan"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("reserved"));
    }

    #[test]
    fn test_invalid_prefix_characters_rejected() {
        let invalid_prefixes = vec![
            "invalid/prefix",
            "invalid@prefix",
            "invalid prefix",
            "invalid.prefix",
        ];

        for prefix in invalid_prefixes {
            let result = generate_id_with_custom_prefix(Some(prefix));
            assert!(result.is_err(), "Prefix '{}' should be rejected", prefix);
            let err = result.unwrap_err();
            assert!(
                err.to_string().contains("invalid characters"),
                "Error for '{}' should mention invalid characters",
                prefix
            );
        }
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
                "Filename '{}' should be valid",
                name
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
                "Filename '{}' should be rejected due to path traversal",
                name
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
                "Filename '{}' should be rejected on Unix",
                name
            );
        }
    }

    #[test]
    #[serial]
    fn test_generated_id_is_file_safe() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_generated_id_safe");
        std::fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let prefixes = vec!["task", "bug", "feature", "my-prefix", "test_under"];

        for prefix in prefixes {
            let id = generate_unique_id_with_prefix(prefix);
            assert!(
                validate_filename(&id).is_ok(),
                "Generated ID '{}' should be file-safe",
                id
            );
            assert!(
                id.starts_with(prefix),
                "ID '{}' should start with prefix '{}'",
                id,
                prefix
            );
            assert!(id.contains('-'), "ID '{}' should contain a hyphen", id);
        }
    }

    #[test]
    fn test_wrap_text_lines_single_line() {
        let result = wrap_text_lines("Hello world", 20, 3);
        assert_eq!(result, vec!["Hello world"]);
    }

    #[test]
    fn test_wrap_text_lines_wraps_at_word_boundary() {
        let result = wrap_text_lines("Hello wonderful world", 12, 3);
        assert_eq!(result, vec!["Hello", "wonderful", "world"]);
    }

    #[test]
    fn test_wrap_text_lines_truncates_with_ellipsis() {
        let result = wrap_text_lines(
            "Line one is here and line two is here and line three is here and line four",
            15,
            2,
        );
        assert_eq!(result.len(), 2);
        assert!(result[1].ends_with("..."));
    }

    #[test]
    fn test_wrap_text_lines_long_word() {
        let result = wrap_text_lines("Supercalifragilisticexpialidocious", 10, 5);
        assert!(result.len() > 1);
        // Long word should be broken
    }

    #[test]
    fn test_wrap_text_lines_empty_input() {
        assert!(wrap_text_lines("", 10, 3).is_empty());
        assert!(wrap_text_lines("   ", 10, 3).is_empty());
    }

    #[test]
    fn test_wrap_text_lines_zero_width() {
        assert!(wrap_text_lines("Hello", 0, 3).is_empty());
    }

    #[test]
    fn test_wrap_text_lines_zero_max_lines() {
        assert!(wrap_text_lines("Hello", 10, 0).is_empty());
    }
}

use jiff::Timestamp;
use rand::Rng;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, BufRead};
use std::path::Path;
use std::process::Command;
use uuid::Uuid;

use crate::error::JanusError;
use crate::types::TICKETS_ITEMS_DIR;

/// Ensure the tickets directory exists
pub fn ensure_dir() -> io::Result<()> {
    fs::create_dir_all(TICKETS_ITEMS_DIR)
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
pub fn generate_id_with_custom_prefix(custom_prefix: Option<&str>) -> Result<String, JanusError> {
    match custom_prefix {
        Some(prefix) if !prefix.is_empty() => {
            validate_prefix(prefix)?;
            Ok(generate_unique_id_with_prefix(prefix))
        }
        _ => Ok(generate_unique_id_with_prefix("task")),
    }
}

/// Validate that a prefix is not reserved and is valid
pub fn validate_prefix(prefix: &str) -> Result<(), JanusError> {
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

    // Check that prefix contains only valid characters (alphanumeric, hyphens, underscores)
    if !prefix
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(JanusError::InvalidPrefix(
            prefix.to_string(),
            format!(
                "Prefix '{}' contains invalid characters. Use only letters, numbers, hyphens, and underscores",
                prefix
            ),
        ));
    }

    Ok(())
}

/// Generate a unique short ID with collision checking
/// Returns a short ID that does not exist in the tickets directory
pub fn generate_unique_id_with_prefix(prefix: &str) -> String {
    const RETRIES_PER_LENGTH: u32 = 40;
    let tickets_dir = Path::new(TICKETS_ITEMS_DIR);

    for length in 4..=8 {
        for _ in 0..RETRIES_PER_LENGTH {
            let hash = generate_hash(length);
            let candidate = format!("{}-{}", prefix, hash);
            let filename = format!("{}.md", candidate);

            if !tickets_dir.join(&filename).exists() {
                return candidate;
            }
        }
    }

    panic!(
        "Failed to generate unique ID after trying hash lengths 4-8 with {} retries each",
        RETRIES_PER_LENGTH
    );
}

/// Generate a unique ticket ID with collision checking for a given prefix
pub fn generate_unique_id(prefix: &str) -> String {
    generate_unique_id_with_prefix(prefix)
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
}

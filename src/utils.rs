use jiff::Zoned;
use rand::Rng;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, BufRead};
use std::process::Command;

use crate::types::TICKETS_DIR;

/// Ensure the tickets directory exists
pub fn ensure_dir() -> io::Result<()> {
    fs::create_dir_all(TICKETS_DIR)
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

/// Generate a unique ticket ID based on directory name and random hash
pub fn generate_id() -> String {
    let dir_name = std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_default();

    // Generate prefix from directory name (first letter of each word)
    let prefix: String = dir_name
        .replace(['-', '_'], " ")
        .split_whitespace()
        .filter_map(|word| word.chars().next())
        .collect();

    let prefix = if prefix.is_empty() {
        dir_name.chars().take(3).collect()
    } else {
        prefix
    };

    // Generate random hash
    let random_bytes: [u8; 16] = rand::rng().random();
    let mut hasher = Sha256::new();
    hasher.update(random_bytes);
    let hash = format!("{:x}", hasher.finalize());

    format!("{}-{}", prefix, &hash[..4])
}

/// Get current ISO date string (without milliseconds)
pub fn iso_date() -> String {
    let now = Zoned::now();
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
    fn test_generate_id_format() {
        let id = generate_id();
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
}

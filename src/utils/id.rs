use rand::Rng;
use uuid::Uuid;

use crate::error::{JanusError, Result};
use crate::types::tickets_items_dir;

use super::validate_filename;

/// Generate a random hex hash of the specified length
///
/// Generates random bytes and hex-encodes them directly, returning the first
/// `length` hex characters. This is used for generating unique IDs for tickets
/// and plans.
pub fn generate_hash(length: usize) -> String {
    // Each byte produces 2 hex characters, so we need ceil(length / 2) bytes
    let num_bytes = length.div_ceil(2);
    let mut buf = vec![0u8; num_bytes];
    rand::rng().fill(&mut buf[..]);
    let hex: String = buf.iter().map(|b| format!("{b:02x}")).collect();
    hex[..length].to_string()
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
            generate_unique_id_with_prefix(prefix)
        }
        _ => generate_unique_id_with_prefix("task"),
    }
}

/// Validate that a prefix is not reserved and is valid
pub fn validate_prefix(prefix: &str) -> Result<()> {
    const RESERVED_PREFIXES: &[&str] = &["plan"];

    if RESERVED_PREFIXES.contains(&prefix) {
        return Err(JanusError::InvalidPrefix(
            prefix.to_string(),
            format!("Prefix '{prefix}' is reserved and cannot be used for tickets"),
        ));
    }

    // Validate prefix format: non-empty and only alphanumeric, hyphens, underscores
    let trimmed = prefix.trim();
    if trimmed.is_empty() {
        return Err(JanusError::InvalidPrefix(
            prefix.to_string(),
            "Prefix cannot be empty or only whitespace".to_string(),
        ));
    }

    if !trimmed
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(JanusError::InvalidPrefix(
            prefix.to_string(),
            format!(
                "Prefix '{trimmed}' contains invalid characters. Use only letters, numbers, hyphens, and underscores"
            ),
        ));
    }

    Ok(())
}

/// Generate a unique short ID with collision checking
/// Returns a short ID that does not exist in the tickets directory
pub fn generate_unique_id_with_prefix(prefix: &str) -> Result<String> {
    const RETRIES_PER_LENGTH: u32 = 40;
    let tickets_dir = tickets_items_dir();

    for length in 4..=8 {
        for _ in 0..RETRIES_PER_LENGTH {
            let hash = generate_hash(length);
            let candidate = format!("{prefix}-{hash}");
            let filename = format!("{candidate}.md");

            // Validate the ID is file-safe before checking for existence
            if validate_filename(&candidate).is_ok() && !tickets_dir.join(&filename).exists() {
                return Ok(candidate);
            }
        }
    }

    Err(JanusError::IdGenerationFailed(format!(
        "Failed to generate unique ID after trying hash lengths 4-8 with {RETRIES_PER_LENGTH} retries each"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::JanusRootGuard;

    #[test]
    fn test_generate_unique_id_with_prefix_format() {
        let id = generate_unique_id_with_prefix("task").unwrap();
        // Should start with prefix "task-"
        assert!(id.starts_with("task-"));
        // Should contain a dash
        assert!(id.contains('-'));
        // The hash part should be 4 characters
        let parts: Vec<&str> = id.rsplitn(2, '-').collect();
        assert_eq!(parts[0].len(), 4);
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
            assert!(result.is_err(), "Prefix '{prefix}' should be rejected");
            let err = result.unwrap_err();
            assert!(
                err.to_string().contains("invalid characters"),
                "Error for '{prefix}' should mention invalid characters"
            );
        }
    }

    #[test]
    fn test_generated_id_is_file_safe() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_generated_id_safe");
        let janus_dir = repo_path.join(".janus");
        std::fs::create_dir_all(&janus_dir).unwrap();
        let _guard = JanusRootGuard::new(&janus_dir);

        let prefixes = vec!["task", "bug", "feature", "my-prefix", "test_under"];

        for prefix in prefixes {
            let id = generate_unique_id_with_prefix(prefix).unwrap();
            assert!(
                validate_filename(&id).is_ok(),
                "Generated ID '{id}' should be file-safe"
            );
            assert!(
                id.starts_with(prefix),
                "ID '{id}' should start with prefix '{prefix}'"
            );
            assert!(id.contains('-'), "ID '{id}' should contain a hyphen");
        }
    }
}

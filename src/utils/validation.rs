//! ID validation utilities
//!
//! This module provides shared validation logic for entity IDs (tickets, plans, etc.)
//! to prevent code duplication across modules. The validation ensures IDs are safe
//! for filesystem use and contain only valid characters.

/// Validate that an ID is safe for filesystem use.
///
/// This function checks:
/// 1. Not empty or whitespace-only
/// 2. No path traversal characters (`/`, `\`, `..`)
/// 3. Only alphanumeric characters, hyphens, and underscores
///
/// # Arguments
///
/// * `id` - The ID string to validate
///
/// # Returns
///
/// * `Ok(())` - If the ID is valid
/// * `Err(String)` - Error message describing the validation failure
///
/// # Examples
///
/// ```
/// use janus::utils::validation::validate_safe_id;
///
/// assert!(validate_safe_id("j-a1b2").is_ok());
/// assert!(validate_safe_id("plan-abc123").is_ok());
/// assert!(validate_safe_id("../../../etc/passwd").is_err());
/// assert!(validate_safe_id("invalid@id").is_err());
/// ```
pub fn validate_safe_id(id: &str) -> Result<(), String> {
    use super::validate_identifier;

    // Delegate empty-string and character-set checks to validate_identifier.
    // This covers: empty/whitespace rejection, and ensures only alphanumeric,
    // hyphens, and underscores are present (which also implicitly prevents
    // path separators like '/', '\', and '.').
    validate_identifier(id, "ID").map_err(|e| match e {
        crate::error::JanusError::ValidationEmpty(_) => {
            "ID cannot be empty or only whitespace".to_string()
        }
        crate::error::JanusError::ValidationInvalidCharacters(_, _) => {
            "ID can only contain alphanumeric characters, hyphens, and underscores".to_string()
        }
        other => other.to_string(),
    })?;

    // Defense-in-depth: explicitly reject path traversal patterns.
    // The character-set check above already prevents '/', '\', and '.',
    // but these checks guard against future changes to validate_identifier
    // that might relax the allowed character set.
    let trimmed = id.trim();
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains("..") {
        return Err("ID cannot contain path separators or traversal sequences".to_string());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_safe_id_empty() {
        assert!(validate_safe_id("").is_err());
        assert!(validate_safe_id("   ").is_err());
    }

    #[test]
    fn test_validate_safe_id_path_traversal() {
        // Path traversal should be rejected
        assert!(validate_safe_id("../etc/passwd").is_err());
        assert!(validate_safe_id("ticket/../other").is_err());
        assert!(validate_safe_id("ticket\\..\\other").is_err());
        assert!(validate_safe_id("..").is_err());
        assert!(validate_safe_id("../").is_err());
    }

    #[test]
    fn test_validate_safe_id_path_separators() {
        assert!(validate_safe_id("a/b").is_err());
        assert!(validate_safe_id("a\\b").is_err());
        assert!(validate_safe_id("/root").is_err());
        assert!(validate_safe_id("C:\\file").is_err());
    }

    #[test]
    fn test_validate_safe_id_special_chars() {
        // Special characters should be rejected
        assert!(validate_safe_id("j@b1").is_err());
        assert!(validate_safe_id("j#b1").is_err());
        assert!(validate_safe_id("j$b1").is_err());
        assert!(validate_safe_id("j%b1").is_err());
        assert!(validate_safe_id("j.b1").is_err());
        assert!(validate_safe_id("j b1").is_err());
    }

    #[test]
    fn test_validate_safe_id_valid() {
        // Valid IDs should pass
        assert!(validate_safe_id("j-a1b2").is_ok());
        assert!(validate_safe_id("j_a1b2").is_ok());
        assert!(validate_safe_id("ticket123").is_ok());
        assert!(validate_safe_id("TICKET-ABC").is_ok());
        assert!(validate_safe_id("plan-abc123").is_ok());
        assert!(validate_safe_id("my_entity").is_ok());
        assert!(validate_safe_id("12345").is_ok());
    }

    #[test]
    fn test_validate_safe_id_whitespace_trimmed() {
        // Whitespace around valid IDs is OK (trimmed during check)
        assert!(validate_safe_id("  j-a1b2  ").is_ok());
    }
}

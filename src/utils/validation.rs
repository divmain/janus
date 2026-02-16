//! Shared validation functions for titles and text content.
//!
//! This module provides centralized validation rules to ensure consistency
//! across all entry points (CLI, MCP, remote sync, etc.).

use crate::error::{JanusError, Result};

// ============================================================================
// Constants
// ============================================================================

/// Maximum length for ticket titles (in characters).
pub const MAX_TICKET_TITLE_LENGTH: usize = 200;

/// Maximum length for plan titles (in characters).
pub const MAX_PLAN_TITLE_LENGTH: usize = 200;

/// Maximum length for descriptions, notes, and summaries (in characters).
pub const MAX_DESCRIPTION_LENGTH: usize = 5000;

// ============================================================================
// Title Validation
// ============================================================================

/// Validates a ticket title according to shared rules.
///
/// Rules:
/// - Must not be empty or whitespace-only after trimming
/// - Must not exceed MAX_TICKET_TITLE_LENGTH characters
/// - Must not contain control characters (including newlines)
///
/// # Arguments
/// * `title` - The title to validate
///
/// # Returns
/// * `Ok(())` if valid
/// * `Err(JanusError::EmptyTitle)` if empty/whitespace-only
/// * `Err(JanusError::TicketTitleTooLong)` if too long
/// * `Err(JanusError::InvalidInput)` if contains control characters
pub fn validate_ticket_title(title: &str) -> Result<()> {
    validate_title_base(
        title,
        MAX_TICKET_TITLE_LENGTH,
        JanusError::EmptyTitle,
        |max, actual| JanusError::TicketTitleTooLong { max, actual },
    )
}

/// Validates a plan title according to shared rules.
///
/// Rules:
/// - Must not be empty or whitespace-only after trimming
/// - Must not exceed MAX_PLAN_TITLE_LENGTH characters
/// - Must not contain control characters (including newlines)
///
/// # Arguments
/// * `title` - The title to validate
///
/// # Returns
/// * `Ok(())` if valid
/// * `Err(JanusError::EmptyPlanTitle)` if empty/whitespace-only
/// * `Err(JanusError::PlanTitleTooLong)` if too long
/// * `Err(JanusError::InvalidInput)` if contains control characters
pub fn validate_plan_title(title: &str) -> Result<()> {
    validate_title_base(
        title,
        MAX_PLAN_TITLE_LENGTH,
        JanusError::EmptyPlanTitle,
        |max, actual| JanusError::PlanTitleTooLong { max, actual },
    )
}

/// Validates a title for MCP requests, returning String errors.
///
/// This is a thin wrapper around the common rules that returns String errors
/// suitable for MCP error responses.
///
/// Rules:
/// - Must not be empty
/// - Must not exceed MAX_TICKET_TITLE_LENGTH characters
/// - Must not contain control characters (including newlines)
///
/// # Arguments
/// * `title` - The title to validate
///
/// # Returns
/// * `Ok(())` if valid
/// * `Err(String)` with descriptive message if invalid
pub fn validate_title_for_mcp(title: &str) -> std::result::Result<(), String> {
    if title.is_empty() {
        return Err("Title cannot be empty".to_string());
    }

    if title.len() > MAX_TICKET_TITLE_LENGTH {
        return Err(format!(
            "Title too long: {} characters (max: {})",
            title.len(),
            MAX_TICKET_TITLE_LENGTH
        ));
    }

    // Check for control characters (including newlines)
    if title.chars().any(|c| c.is_control()) {
        return Err("Title contains invalid control characters".to_string());
    }

    Ok(())
}

/// Base validation function for titles.
///
/// # Arguments
/// * `title` - The title to validate
/// * `max_length` - Maximum allowed length
/// * `empty_error` - Error to return if title is empty/whitespace-only
/// * `too_long_error` - Function to create error when title is too long
fn validate_title_base<E1, E2>(
    title: &str,
    max_length: usize,
    empty_error: E1,
    too_long_error: E2,
) -> Result<()>
where
    E1: Into<JanusError>,
    E2: FnOnce(usize, usize) -> JanusError,
{
    let trimmed = title.trim();

    if trimmed.is_empty() {
        return Err(empty_error.into());
    }

    if trimmed.len() > max_length {
        return Err(too_long_error(max_length, trimmed.len()));
    }

    // Check for control characters (including newlines)
    if trimmed.chars().any(|c| c.is_control()) {
        return Err(JanusError::InvalidInput(
            "Title cannot contain control characters or newlines".to_string(),
        ));
    }

    Ok(())
}

// ============================================================================
// Description/Text Validation
// ============================================================================

/// Validates a description/note string.
///
/// Rules:
/// - Must not exceed MAX_DESCRIPTION_LENGTH characters
/// - May contain newlines and carriage returns
/// - Must not contain other control characters
///
/// # Arguments
/// * `text` - The text to validate
/// * `field_name` - Name of the field for error messages
///
/// # Returns
/// * `Ok(())` if valid
/// * `Err(String)` with descriptive message if invalid
pub fn validate_description(text: &str, field_name: &str) -> std::result::Result<(), String> {
    if text.len() > MAX_DESCRIPTION_LENGTH {
        return Err(format!(
            "{} too long: {} characters (max: {})",
            field_name,
            text.len(),
            MAX_DESCRIPTION_LENGTH
        ));
    }

    // Allow newlines but reject other control characters
    if text
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\r')
    {
        return Err(format!("{field_name} contains invalid control characters"));
    }

    Ok(())
}

/// Validates a note (non-empty version of description validation).
///
/// # Arguments
/// * `note` - The note text to validate
///
/// # Returns
/// * `Ok(())` if valid
/// * `Err(String)` with descriptive message if invalid
pub fn validate_note(note: &str) -> std::result::Result<(), String> {
    if note.is_empty() {
        return Err("Note cannot be empty".to_string());
    }
    validate_description(note, "Note")
}

/// Validates an optional summary.
///
/// # Arguments
/// * `summary` - Optional summary text
///
/// # Returns
/// * `Ok(())` if valid or None
/// * `Err(String)` with descriptive message if Some and invalid
pub fn validate_optional_summary(summary: Option<&str>) -> std::result::Result<(), String> {
    if let Some(text) = summary {
        if text.len() > MAX_DESCRIPTION_LENGTH {
            return Err(format!(
                "Summary too long: {} characters (max: {})",
                text.len(),
                MAX_DESCRIPTION_LENGTH
            ));
        }
        // Allow newlines but reject other control characters
        if text
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\r')
        {
            return Err("Summary contains invalid control characters".to_string());
        }
    }
    Ok(())
}

/// Maximum length for remote titles after sanitization.
pub const MAX_REMOTE_TITLE_LENGTH: usize = 200;

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================================
    // Ticket Title Validation Tests
    // ============================================================================

    #[test]
    fn test_validate_ticket_title_empty() {
        let result = validate_ticket_title("");
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::EmptyTitle => {}
            _ => panic!("Expected EmptyTitle error"),
        }
    }

    #[test]
    fn test_validate_ticket_title_whitespace_only() {
        let result = validate_ticket_title("   \n\t  ");
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::EmptyTitle => {}
            _ => panic!("Expected EmptyTitle error"),
        }
    }

    #[test]
    fn test_validate_ticket_title_too_long() {
        let long_title = "a".repeat(MAX_TICKET_TITLE_LENGTH + 1);
        let result = validate_ticket_title(&long_title);
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::TicketTitleTooLong { max, actual } => {
                assert_eq!(max, MAX_TICKET_TITLE_LENGTH);
                assert_eq!(actual, MAX_TICKET_TITLE_LENGTH + 1);
            }
            _ => panic!("Expected TicketTitleTooLong error"),
        }
    }

    #[test]
    fn test_validate_ticket_title_max_length() {
        let max_title = "a".repeat(MAX_TICKET_TITLE_LENGTH);
        let result = validate_ticket_title(&max_title);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_ticket_title_control_chars() {
        let result = validate_ticket_title("Title\x00with\x01nulls");
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::InvalidInput(msg) => {
                assert!(msg.contains("control characters"));
            }
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[test]
    fn test_validate_ticket_title_newline() {
        let result = validate_ticket_title("Title\nwith newline");
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::InvalidInput(msg) => {
                assert!(msg.contains("control characters"));
            }
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[test]
    fn test_validate_ticket_title_valid() {
        let result = validate_ticket_title("Valid Ticket Title");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_ticket_title_with_punctuation() {
        let result = validate_ticket_title("Fix: Handle edge-case (urgent)!");
        assert!(result.is_ok());
    }

    // ============================================================================
    // Plan Title Validation Tests
    // ============================================================================

    #[test]
    fn test_validate_plan_title_empty() {
        let result = validate_plan_title("");
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::EmptyPlanTitle => {}
            _ => panic!("Expected EmptyPlanTitle error"),
        }
    }

    #[test]
    fn test_validate_plan_title_too_long() {
        let long_title = "a".repeat(MAX_PLAN_TITLE_LENGTH + 1);
        let result = validate_plan_title(&long_title);
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::PlanTitleTooLong { max, actual } => {
                assert_eq!(max, MAX_PLAN_TITLE_LENGTH);
                assert_eq!(actual, MAX_PLAN_TITLE_LENGTH + 1);
            }
            _ => panic!("Expected PlanTitleTooLong error"),
        }
    }

    #[test]
    fn test_validate_plan_title_valid() {
        let result = validate_plan_title("Valid Plan Title");
        assert!(result.is_ok());
    }

    // ============================================================================
    // MCP Title Validation Tests
    // ============================================================================

    #[test]
    fn test_validate_title_for_mcp_empty() {
        let result = validate_title_for_mcp("");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[test]
    fn test_validate_title_for_mcp_whitespace_only() {
        // Note: MCP validation does NOT trim, so whitespace-only is not empty
        let result = validate_title_for_mcp("   ");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_title_for_mcp_too_long() {
        let long_title = "a".repeat(MAX_TICKET_TITLE_LENGTH + 1);
        let result = validate_title_for_mcp(&long_title);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("too long"));
    }

    #[test]
    fn test_validate_title_for_mcp_control_chars() {
        let result = validate_title_for_mcp("Title\x00with null");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("control characters"));
    }

    #[test]
    fn test_validate_title_for_mcp_newline() {
        let result = validate_title_for_mcp("Title\nwith newline");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("control characters"));
    }

    #[test]
    fn test_validate_title_for_mcp_valid() {
        let result = validate_title_for_mcp("Valid Title");
        assert!(result.is_ok());
    }

    // ============================================================================
    // Description Validation Tests
    // ============================================================================

    #[test]
    fn test_validate_description_too_long() {
        let long_desc = "a".repeat(MAX_DESCRIPTION_LENGTH + 1);
        let result = validate_description(&long_desc, "Description");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("too long"));
    }

    #[test]
    fn test_validate_description_max_length() {
        let max_desc = "a".repeat(MAX_DESCRIPTION_LENGTH);
        let result = validate_description(&max_desc, "Description");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_description_newlines_allowed() {
        let result = validate_description("Line 1\nLine 2\r\nLine 3", "Description");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_description_control_chars() {
        let result = validate_description("Desc\x00with\x01nulls", "Description");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("control characters"));
    }

    // ============================================================================
    // Note Validation Tests
    // ============================================================================

    #[test]
    fn test_validate_note_empty() {
        let result = validate_note("");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[test]
    fn test_validate_note_valid() {
        let result = validate_note("This is a valid note.");
        assert!(result.is_ok());
    }

    // ============================================================================
    // Optional Summary Validation Tests
    // ============================================================================

    #[test]
    fn test_validate_optional_summary_none() {
        let result = validate_optional_summary(None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_optional_summary_valid() {
        let result = validate_optional_summary(Some("Valid summary"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_optional_summary_too_long() {
        let long_summary = "a".repeat(MAX_DESCRIPTION_LENGTH + 1);
        let result = validate_optional_summary(Some(&long_summary));
        assert!(result.is_err());
    }
}

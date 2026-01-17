//! Form validation module for TUI forms
//!
//! This module provides validation logic for various form types in the TUI.

use crate::types::{TicketPriority, TicketStatus, TicketType};

/// Result of form validation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationResult {
    /// Whether validation passed
    pub is_valid: bool,
    /// Error message if validation failed
    pub error: Option<String>,
}

impl ValidationResult {
    /// Create a successful validation result
    pub fn success() -> Self {
        Self {
            is_valid: true,
            error: None,
        }
    }

    /// Create a failed validation result with an error message
    pub fn failure(message: String) -> Self {
        Self {
            is_valid: false,
            error: Some(message),
        }
    }
}

/// Validator for ticket edit forms
pub struct TicketFormValidator;

impl TicketFormValidator {
    /// Validate a ticket form
    ///
    /// Checks that required fields are present and valid.
    pub fn validate(
        title: &str,
        _status: TicketStatus,
        _ticket_type: TicketType,
        _priority: TicketPriority,
        _body: &str,
    ) -> ValidationResult {
        if title.trim().is_empty() {
            return ValidationResult::failure("Title cannot be empty".to_string());
        }
        ValidationResult::success()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_success() {
        let result = TicketFormValidator::validate(
            "Test Title",
            TicketStatus::New,
            TicketType::Task,
            TicketPriority::P2,
            "Body",
        );
        assert!(result.is_valid);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_validation_empty_title() {
        let result = TicketFormValidator::validate(
            "",
            TicketStatus::New,
            TicketType::Task,
            TicketPriority::P2,
            "Body",
        );
        assert!(!result.is_valid);
        assert_eq!(result.error, Some("Title cannot be empty".to_string()));
    }

    #[test]
    fn test_validation_whitespace_title() {
        let result = TicketFormValidator::validate(
            "   ",
            TicketStatus::New,
            TicketType::Task,
            TicketPriority::P2,
            "Body",
        );
        assert!(!result.is_valid);
        assert_eq!(result.error, Some("Title cannot be empty".to_string()));
    }

    #[test]
    fn test_validation_with_body() {
        let result = TicketFormValidator::validate(
            "Title",
            TicketStatus::InProgress,
            TicketType::Feature,
            TicketPriority::P1,
            "Multi\nline\nbody",
        );
        assert!(result.is_valid);
    }
}

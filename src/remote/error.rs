//! Shared error handling for remote providers.
//!
//! This module provides common types and utilities for converting provider-specific
//! errors into Janus errors, reducing duplication between GitHub and Linear providers.

use std::fmt;
use std::time::Duration;

use crate::error::JanusError;

use super::AsHttpError;

/// Generic API error that can be used by any remote provider.
///
/// This provides a common structure for wrapping provider-specific errors
/// while preserving HTTP status information for retry logic.
#[derive(Debug)]
pub struct ApiError {
    /// HTTP status code, if available
    pub status: Option<reqwest::StatusCode>,
    /// Retry-After header value in seconds, if available
    pub retry_after: Option<u64>,
    /// Human-readable error message
    pub message: String,
    /// Provider name for context (e.g., "GitHub", "Linear")
    pub provider: &'static str,
}

impl ApiError {
    /// Create a new API error with the given message and provider.
    pub fn new(message: impl Into<String>, provider: &'static str) -> Self {
        Self {
            status: None,
            retry_after: None,
            message: message.into(),
            provider,
        }
    }

    /// Create a new API error with HTTP status information.
    pub fn with_status(
        message: impl Into<String>,
        provider: &'static str,
        status: reqwest::StatusCode,
    ) -> Self {
        Self {
            status: Some(status),
            retry_after: None,
            message: message.into(),
            provider,
        }
    }

    /// Set the retry-after value.
    pub fn with_retry_after(mut self, seconds: u64) -> Self {
        self.retry_after = Some(seconds);
        self
    }

    /// Convert this error to a JanusError using the standard pattern.
    ///
    /// This follows the common pattern for all providers:
    /// 1. Check if rate limited -> return RateLimited error
    /// 2. Otherwise return Api error with formatted message
    pub fn to_janus_error(&self) -> JanusError {
        if self.is_rate_limited() {
            return if let Some(duration) = self.get_retry_after() {
                JanusError::RateLimited(duration.as_secs())
            } else {
                JanusError::RateLimited(60)
            };
        }

        JanusError::Api(format!("{} API error: {}", self.provider, self.message))
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl AsHttpError for ApiError {
    fn as_http_error(&self) -> Option<(reqwest::StatusCode, Option<u64>)> {
        self.status.map(|s| (s, self.retry_after))
    }

    fn is_transient(&self) -> bool {
        if let Some(status) = self.status {
            return status.is_server_error();
        }
        false
    }

    fn is_rate_limited(&self) -> bool {
        if let Some(status) = self.status {
            return status.as_u16() == 429;
        }
        false
    }
}

/// Convert an API error to JanusError.
impl From<ApiError> for JanusError {
    fn from(error: ApiError) -> Self {
        error.to_janus_error()
    }
}

/// Build a detailed error message from an octocrab GitHub error.
///
/// This extracts structured error information from octocrab's error types
/// to create a user-friendly error message.
pub fn build_github_error_message(error: &octocrab::Error) -> String {
    match error {
        octocrab::Error::GitHub { source, .. } => {
            let status = source.status_code;
            let status_text = status.canonical_reason().unwrap_or("Unknown");
            let mut message = format!(
                "GitHub API error ({} {}): {}",
                status.as_u16(),
                status_text,
                source.message
            );

            if let Some(errors) = &source.errors
                && !errors.is_empty()
            {
                message.push_str("\n\nErrors:");
                for error in errors {
                    message.push_str(&format!("\n- {error}"));
                }
            }

            if let Some(doc_url) = &source.documentation_url {
                message.push_str(&format!("\n\nDocumentation URL: {doc_url}"));
            }

            message
        }
        octocrab::Error::Http { source, .. } => format!("HTTP error: {source}"),
        octocrab::Error::Service { source, .. } => format!("Service error: {source}"),
        octocrab::Error::Serde { source, .. } => format!("Serialization error: {source}"),
        octocrab::Error::Json { source, .. } => {
            format!("JSON error in {}: {}", source.path(), source.inner())
        }
        octocrab::Error::JWT { source, .. } => format!("JWT error: {source}"),
        _ => format!("GitHub API error: {error}"),
    }
}

/// Check if an octocrab error is rate limited.
///
/// Returns true if the error indicates rate limiting (403 or 429 status).
pub fn is_github_rate_limited(error: &octocrab::Error) -> bool {
    if let octocrab::Error::GitHub { source, .. } = error {
        let status = source.status_code.as_u16();
        return status == 403 || status == 429;
    }

    let error_msg = error.to_string().to_lowercase();
    error_msg.contains("rate limit") || error_msg.contains("api rate limit exceeded")
}

/// Get retry duration for a GitHub error.
///
/// Returns Some(Duration) if rate limited, None otherwise.
/// Defaults to 60 seconds if no specific retry-after value is available.
pub fn get_github_retry_after(error: &octocrab::Error) -> Option<Duration> {
    if !is_github_rate_limited(error) {
        return None;
    }

    // Try to get status-specific retry info
    if let octocrab::Error::GitHub { source, .. } = error {
        let status = source.status_code.as_u16();
        if status == 403 || status == 429 {
            // Default to 60 seconds for rate limit errors
            return Some(Duration::from_secs(60));
        }
    }

    Some(Duration::from_secs(60))
}

/// Check if a GitHub error is transient (retryable).
///
/// Returns true for server errors and network-related errors.
pub fn is_github_transient(error: &octocrab::Error) -> bool {
    if let octocrab::Error::GitHub { source, .. } = error {
        let status = source.status_code;
        if status.is_server_error() {
            return true;
        }
    }

    let error_msg = error.to_string().to_lowercase();
    error_msg.contains("timed out")
        || error_msg.contains("timeout")
        || error_msg.contains("connection")
        || error_msg.contains("network")
        || error_msg.contains("service unavailable")
}

/// Get HTTP status code from an octocrab error, if available.
pub fn get_github_status_code(error: &octocrab::Error) -> Option<reqwest::StatusCode> {
    if let octocrab::Error::GitHub { source, .. } = error {
        return reqwest::StatusCode::from_u16(source.status_code.as_u16()).ok();
    }
    None
}

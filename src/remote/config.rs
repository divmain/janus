//! Remote-specific configuration types.
//!
//! This module contains types specific to remote issue sync (platforms, default
//! remote settings). The main `Config` struct and non-remote types like
//! `HooksConfig` and `SemanticSearchConfig` live in `crate::config`.

use serde::{Deserialize, Serialize};

use crate::error::{JanusError, Result};

// Re-export everything from the top-level config module for backward compatibility.
// This allows existing code using `crate::remote::config::Config` to continue working.
pub use crate::config::{
    AuthConfig, Config, GitHubAuth, HooksConfig, LinearAuth, SemanticSearchConfig,
};

/// Default remote configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultRemote {
    /// Platform type (github or linear)
    pub platform: Platform,
    /// Organization/owner name
    pub org: String,
    /// Default repository (for GitHub only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
}

/// Supported remote platforms
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    GitHub,
    Linear,
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Platform::GitHub => write!(f, "github"),
            Platform::Linear => write!(f, "linear"),
        }
    }
}

impl std::str::FromStr for Platform {
    type Err = JanusError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "github" => Ok(Platform::GitHub),
            "linear" => Ok(Platform::Linear),
            _ => Err(JanusError::Config(format!(
                "unknown platform '{s}', expected 'github' or 'linear'"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_display() {
        assert_eq!(Platform::GitHub.to_string(), "github");
        assert_eq!(Platform::Linear.to_string(), "linear");
    }

    #[test]
    fn test_platform_from_str() {
        assert_eq!("github".parse::<Platform>().unwrap(), Platform::GitHub);
        assert_eq!("GitHub".parse::<Platform>().unwrap(), Platform::GitHub);
        assert_eq!("linear".parse::<Platform>().unwrap(), Platform::Linear);
        assert_eq!("Linear".parse::<Platform>().unwrap(), Platform::Linear);
        assert!("invalid".parse::<Platform>().is_err());
    }
}

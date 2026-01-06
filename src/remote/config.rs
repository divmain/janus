//! Configuration handling for remote issue sync.
//!
//! Configuration is stored in `.janus/config.yaml` and includes:
//! - Default remote platform and organization
//! - Authentication tokens for GitHub and Linear

use std::env;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{JanusError, Result};
use crate::types::TICKETS_DIR;

/// Main configuration structure
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    /// Default remote platform and organization
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_remote: Option<DefaultRemote>,

    /// Authentication tokens
    #[serde(default)]
    pub auth: AuthConfig,
}

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
                "unknown platform '{}', expected 'github' or 'linear'",
                s
            ))),
        }
    }
}

/// Authentication configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuthConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github: Option<GitHubAuth>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linear: Option<LinearAuth>,
}

/// GitHub authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubAuth {
    pub token: String,
}

/// Linear authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearAuth {
    pub api_key: String,
}

impl Config {
    /// Get the path to the config file
    pub fn config_path() -> PathBuf {
        PathBuf::from(TICKETS_DIR).join("config.yaml")
    }

    /// Load configuration from file, or return default if not found
    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        if !path.exists() {
            return Ok(Config::default());
        }

        let content = fs::read_to_string(&path)?;
        let config: Config = serde_yaml_ng::from_str(&content)?;
        Ok(config)
    }

    /// Save configuration to file
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();

        // Ensure .janus directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = serde_yaml_ng::to_string(self)?;
        fs::write(&path, content)?;
        Ok(())
    }

    /// Get GitHub token from config or environment variable
    pub fn github_token(&self) -> Option<String> {
        // First check environment variable
        if let Ok(token) = env::var("GITHUB_TOKEN")
            && !token.is_empty()
        {
            return Some(token);
        }

        // Fall back to config file
        self.auth.github.as_ref().map(|g| g.token.clone())
    }

    /// Get Linear API key from config or environment variable
    pub fn linear_api_key(&self) -> Option<String> {
        // First check environment variable
        if let Ok(key) = env::var("LINEAR_API_KEY")
            && !key.is_empty()
        {
            return Some(key);
        }

        // Fall back to config file
        self.auth.linear.as_ref().map(|l| l.api_key.clone())
    }

    /// Set GitHub token
    pub fn set_github_token(&mut self, token: String) {
        self.auth.github = Some(GitHubAuth { token });
    }

    /// Set Linear API key
    pub fn set_linear_api_key(&mut self, api_key: String) {
        self.auth.linear = Some(LinearAuth { api_key });
    }

    /// Set default remote
    pub fn set_default_remote(&mut self, platform: Platform, org: String, repo: Option<String>) {
        self.default_remote = Some(DefaultRemote {
            platform,
            org,
            repo,
        });
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

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert!(config.default_remote.is_none());
        assert!(config.auth.github.is_none());
        assert!(config.auth.linear.is_none());
    }

    #[test]
    fn test_config_serialization() {
        let mut config = Config::default();
        config.set_github_token("ghp_test123".to_string());
        config.set_default_remote(
            Platform::GitHub,
            "myorg".to_string(),
            Some("myrepo".to_string()),
        );

        let yaml = serde_yaml_ng::to_string(&config).unwrap();
        let parsed: Config = serde_yaml_ng::from_str(&yaml).unwrap();

        assert_eq!(parsed.github_token(), Some("ghp_test123".to_string()));
        let default = parsed.default_remote.unwrap();
        assert_eq!(default.platform, Platform::GitHub);
        assert_eq!(default.org, "myorg");
        assert_eq!(default.repo, Some("myrepo".to_string()));
    }
}

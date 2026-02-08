//! Configuration handling for remote issue sync.
//!
//! Configuration is stored in `.janus/config.yaml` and includes:
//! - Default remote platform and organization
//! - Authentication tokens for GitHub and Linear

use std::collections::HashMap;
use std::env;
use std::fmt;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{JanusError, Result};
use crate::types::janus_root;

/// Main configuration structure
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    /// Default remote platform and organization
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_remote: Option<DefaultRemote>,

    /// Authentication tokens
    #[serde(default)]
    pub auth: AuthConfig,

    /// Hooks configuration
    #[serde(default, skip_serializing_if = "HooksConfig::is_default")]
    pub hooks: HooksConfig,

    /// Semantic search configuration
    #[serde(default, skip_serializing_if = "SemanticSearchConfig::is_default")]
    pub semantic_search: SemanticSearchConfig,
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
                "unknown platform '{s}', expected 'github' or 'linear'"
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
#[derive(Clone, Serialize, Deserialize)]
pub struct GitHubAuth {
    pub token: String,
}

impl fmt::Debug for GitHubAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GitHubAuth")
            .field("token", &"[REDACTED]")
            .finish()
    }
}

/// Linear authentication
#[derive(Clone, Serialize, Deserialize)]
pub struct LinearAuth {
    pub api_key: String,
}

impl fmt::Debug for LinearAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinearAuth")
            .field("api_key", &"[REDACTED]")
            .finish()
    }
}

/// Hooks configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HooksConfig {
    /// Whether hooks are enabled (default: true)
    #[serde(default = "default_hooks_enabled")]
    pub enabled: bool,

    /// Timeout in seconds for hook scripts (default: 30, 0 = no timeout)
    #[serde(default = "default_hooks_timeout")]
    pub timeout: u64,

    /// Mapping of event names to script paths (relative to .janus/hooks/)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub scripts: HashMap<String, String>,
}

/// Semantic search configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticSearchConfig {
    /// Whether semantic search is enabled (default: true)
    #[serde(default = "default_semantic_search_enabled")]
    pub enabled: bool,
}

fn default_semantic_search_enabled() -> bool {
    true
}

impl Default for SemanticSearchConfig {
    fn default() -> Self {
        Self {
            enabled: default_semantic_search_enabled(),
        }
    }
}

impl SemanticSearchConfig {
    /// Check if this config has default values
    pub fn is_default(&self) -> bool {
        self.enabled == default_semantic_search_enabled()
    }
}

fn default_hooks_enabled() -> bool {
    true
}

fn default_hooks_timeout() -> u64 {
    30
}

impl Default for HooksConfig {
    fn default() -> Self {
        Self {
            enabled: default_hooks_enabled(),
            timeout: default_hooks_timeout(),
            scripts: HashMap::new(),
        }
    }
}

impl HooksConfig {
    /// Check if this config is the default (for serialization skip)
    pub fn is_default(&self) -> bool {
        self.enabled == default_hooks_enabled()
            && self.timeout == default_hooks_timeout()
            && self.scripts.is_empty()
    }

    /// Get the script path for a given event name
    pub fn get_script(&self, event_name: &str) -> Option<&String> {
        self.scripts.get(event_name)
    }
}

impl Config {
    /// Get the path to the config file
    pub fn config_path() -> PathBuf {
        janus_root().join("config.yaml")
    }

    /// Load configuration from file, or return default if not found
    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        if !path.exists() {
            return Ok(Config::default());
        }

        let content = fs::read_to_string(&path).map_err(|e| {
            JanusError::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to read config at {}: {}",
                    crate::utils::format_relative_path(&path),
                    e
                ),
            ))
        })?;
        let config: Config = serde_yaml_ng::from_str(&content)?;
        Ok(config)
    }

    /// Save configuration to file
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();

        // Ensure .janus directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                JanusError::Io(std::io::Error::new(
                    e.kind(),
                    format!(
                        "Failed to create directory for config at {}: {}",
                        crate::utils::format_relative_path(parent),
                        e
                    ),
                ))
            })?;
        }

        let content = serde_yaml_ng::to_string(self)?;
        fs::write(&path, content).map_err(|e| {
            JanusError::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to write config at {}: {}",
                    crate::utils::format_relative_path(&path),
                    e
                ),
            ))
        })?;

        // Set restrictive permissions on Unix (owner read/write only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&path, permissions).map_err(|e| {
                JanusError::Io(std::io::Error::new(
                    e.kind(),
                    format!(
                        "Failed to set permissions on config at {}: {}",
                        crate::utils::format_relative_path(&path),
                        e
                    ),
                ))
            })?;
        }

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

    /// Check if semantic search is enabled
    pub fn semantic_search_enabled(&self) -> bool {
        self.semantic_search.enabled
    }

    /// Set semantic search enabled status
    pub fn set_semantic_search_enabled(&mut self, enabled: bool) {
        self.semantic_search.enabled = enabled;
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

    #[test]
    fn test_config_semantic_search_default() {
        // Test that configs without semantic_search field default to enabled
        let yaml_without_semantic = r#"
default_remote:
  platform: github
  org: myorg
"#;

        let config: Config = serde_yaml_ng::from_str(yaml_without_semantic).unwrap();
        assert!(config.semantic_search_enabled());
    }

    #[test]
    fn test_config_semantic_search_explicit_false() {
        // Test that explicit false is respected
        let yaml_with_disabled = r#"
semantic_search:
  enabled: false
"#;

        let config: Config = serde_yaml_ng::from_str(yaml_with_disabled).unwrap();
        assert!(!config.semantic_search_enabled());
    }

    #[test]
    fn test_config_semantic_search_explicit_true() {
        // Test that explicit true works
        let yaml_with_enabled = r#"
semantic_search:
  enabled: true
"#;

        let config: Config = serde_yaml_ng::from_str(yaml_with_enabled).unwrap();
        assert!(config.semantic_search_enabled());
    }

    #[test]
    fn test_config_semantic_search_roundtrip() {
        // Test that semantic search setting persists through serialization
        let mut config = Config::default();
        assert!(config.semantic_search_enabled()); // Default is enabled

        // Disable and save
        config.set_semantic_search_enabled(false);
        let yaml = serde_yaml_ng::to_string(&config).unwrap();

        // Load and verify
        let loaded: Config = serde_yaml_ng::from_str(&yaml).unwrap();
        assert!(!loaded.semantic_search_enabled());
    }

    #[test]
    fn test_config_default_semantic_search_is_enabled() {
        let config = Config::default();
        assert!(config.semantic_search_enabled());
    }
}

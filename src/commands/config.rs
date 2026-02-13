//! Configuration commands for managing Janus settings.
//!
//! - `config set`: Set a configuration value
//! - `config show`: Display current configuration

use owo_colors::OwoColorize;
use serde_json::json;

use super::CommandOutput;
use crate::cli::OutputOptions;
use crate::config::Config;
use crate::error::{JanusError, Result};
use crate::remote::Platform;

/// Validate a config key and convert underscore notation to dot notation suggestion
fn validate_config_key(key: &str) -> Result<&str> {
    // Allow keys that are explicitly valid (some have underscores in section names)
    if key == "semantic_search.enabled" {
        return Ok(key);
    }

    // Check if key uses underscore notation that should be dot notation
    // Only the first underscore should be replaced with a dot (e.g., linear_api_key -> linear.api_key)
    if let Some(pos) = key.find('_') {
        let dot_version = format!("{}.{}", &key[..pos], &key[pos + 1..]);
        return Err(JanusError::Config(format!(
            "invalid config key '{key}'. Use dot notation: '{dot_version}'"
        )));
    }
    Ok(key)
}

/// Mask a sensitive value by showing only the first 2 and last 2 characters
fn mask_sensitive_value(value: &str) -> String {
    let char_count = value.chars().count();
    if char_count > 4 {
        let first: String = value.chars().take(2).collect();
        let last: String = value.chars().skip(char_count - 2).collect();
        format!("{first}...{last}")
    } else {
        "****".to_string()
    }
}

/// Show current configuration
pub fn cmd_config_show(output: OutputOptions) -> Result<()> {
    let config = Config::load()?;

    let default_remote_json = config.default_remote.as_ref().map(|d| {
        json!({
            "platform": d.platform.to_string(),
            "org": d.org,
            "repo": d.repo,
        })
    });

    let github_configured = config.github_token().is_some();
    let linear_configured = config.linear_api_key().is_some();

    // Build JSON output
    let json_output = json!({
        "default_remote": default_remote_json,
        "auth": {
            "github_token_configured": github_configured,
            "linear_api_key_configured": linear_configured,
        },
        "semantic_search": {
            "enabled": config.semantic_search_enabled(),
        },
        "config_file": Config::config_path().to_string_lossy(),
    });

    // Build text output
    let mut text_output = String::new();

    text_output.push_str(&format!("{}\n\n", "Configuration:".cyan().bold()));

    // Default remote
    if let Some(ref default) = config.default_remote {
        text_output.push_str(&format!("{}:\n", "default_remote".cyan()));
        text_output.push_str(&format!("  platform: {}\n", default.platform));
        text_output.push_str(&format!("  org: {}\n", default.org));
        if let Some(ref repo) = default.repo {
            text_output.push_str(&format!("  repo: {repo}\n"));
        }
    } else {
        text_output.push_str(&format!(
            "{}: {}\n",
            "default_remote".cyan(),
            "not configured".dimmed()
        ));
    }

    text_output.push('\n');

    // Auth status (don't show actual tokens)
    text_output.push_str(&format!("{}:\n", "auth".cyan()));

    let github_status = if github_configured {
        "configured".green().to_string()
    } else {
        "not configured".dimmed().to_string()
    };
    let linear_status = if linear_configured {
        "configured".green().to_string()
    } else {
        "not configured".dimmed().to_string()
    };

    text_output.push_str(&format!("  github.token: {github_status}\n"));
    text_output.push_str(&format!("  linear.api_key: {linear_status}\n"));

    text_output.push('\n');

    // Semantic search status
    text_output.push_str(&format!("{}:\n", "semantic_search".cyan()));
    text_output.push_str(&format!(
        "  enabled: {}\n",
        config.semantic_search_enabled()
    ));

    text_output.push('\n');
    text_output.push_str(&format!(
        "{}",
        format!("Config file: {}", Config::config_path().display()).dimmed()
    ));

    CommandOutput::new(json_output)
        .with_text(text_output)
        .print(output)
}

/// Set a configuration value
pub fn cmd_config_set(key: &str, value: &str, output: OutputOptions) -> Result<()> {
    validate_config_key(key)?;

    let mut config = Config::load()?;

    let (json_output, text_output) = match key {
        "github.token" => {
            config.set_github_token(value.to_string());
            config.save()?;
            let json = json!({
                "action": "config_set",
                "key": key,
                "success": true,
            });
            let text = format!("Set {}", "github.token".cyan());
            (json, text)
        }
        "linear.api_key" => {
            config.set_linear_api_key(value.to_string());
            config.save()?;
            let json = json!({
                "action": "config_set",
                "key": key,
                "success": true,
            });
            let text = format!("Set {}", "linear.api_key".cyan());
            (json, text)
        }
        "default.remote" => {
            // Format: "platform:org" or "platform:org/repo"
            let (platform, rest) = parse_default_remote(value)?;
            let (org, repo) = if let Some(idx) = rest.find('/') {
                (rest[..idx].to_string(), Some(rest[idx + 1..].to_string()))
            } else {
                (rest.to_string(), None)
            };

            config.set_default_remote(platform, org.clone(), repo.clone());
            config.save()?;

            let json = json!({
                "action": "config_set",
                "key": key,
                "value": value,
                "success": true,
            });

            let text = if let Some(r) = repo {
                format!(
                    "Set {} to {}:{}/{}",
                    "default.remote".cyan(),
                    platform,
                    org,
                    r
                )
            } else {
                format!("Set {} to {}:{}", "default.remote".cyan(), platform, org)
            };
            (json, text)
        }
        "semantic_search.enabled" => {
            let enabled = value.parse::<bool>().map_err(|_| {
                JanusError::Config(format!(
                    "invalid value '{value}' for semantic_search.enabled. Expected: true or false"
                ))
            })?;
            config.set_semantic_search_enabled(enabled);
            config.save()?;
            let json = json!({
                "action": "config_set",
                "key": key,
                "value": enabled,
                "success": true,
            });
            let text = format!("Set {} to {}", "semantic_search.enabled".cyan(), enabled);
            (json, text)
        }
        _ => {
            return Err(JanusError::Config(format!(
                "unknown config key '{key}'. Valid keys: github.token, linear.api_key, default.remote, semantic_search.enabled"
            )));
        }
    };

    CommandOutput::new(json_output)
        .with_text(text_output)
        .print(output)
}

/// Parse a default_remote value like "github:myorg/myrepo" or "linear:myorg"
fn parse_default_remote(value: &str) -> Result<(Platform, String)> {
    let parts: Vec<&str> = value.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(JanusError::Config(format!(
            "invalid default_remote format '{value}'. Expected: platform:org or platform:org/repo"
        )));
    }

    let platform: Platform = parts[0].parse()?;
    let rest = parts[1].to_string();

    if rest.is_empty() {
        return Err(JanusError::Config(
            "org cannot be empty in default_remote".to_string(),
        ));
    }

    Ok((platform, rest))
}

/// Get a specific configuration value
pub fn cmd_config_get(key: &str, output: OutputOptions) -> Result<()> {
    validate_config_key(key)?;

    let config = Config::load()?;

    let (json_output, text_output) = match key {
        "github.token" => {
            if let Some(token) = config.github_token() {
                let masked = mask_sensitive_value(&token);
                let json = json!({
                    "key": key,
                    "value": masked,
                    "configured": true,
                    "masked": true,
                });
                let text = format!("{masked} (masked - showing first 2 and last 2 characters)");
                (json, text)
            } else {
                return Err(JanusError::Config("github.token not set".to_string()));
            }
        }
        "linear.api_key" => {
            if let Some(api_key) = config.linear_api_key() {
                let masked = mask_sensitive_value(&api_key);
                let json = json!({
                    "key": key,
                    "value": masked,
                    "configured": true,
                    "masked": true,
                });
                let text = format!("{masked} (masked - showing first 2 and last 2 characters)");
                (json, text)
            } else {
                return Err(JanusError::Config("linear.api_key not set".to_string()));
            }
        }
        "default.remote" => {
            if let Some(ref default) = config.default_remote {
                let value = if let Some(ref repo) = default.repo {
                    format!("{}:{}/{}", default.platform, default.org, repo)
                } else {
                    format!("{}:{}", default.platform, default.org)
                };
                let json = json!({
                    "key": key,
                    "value": value,
                    "configured": true,
                });
                let text = value;
                (json, text)
            } else {
                return Err(JanusError::Config("default.remote not set".to_string()));
            }
        }
        "semantic_search.enabled" => {
            let enabled = config.semantic_search_enabled();
            let json = json!({
                "key": key,
                "value": enabled,
                "configured": true,
            });
            let text = enabled.to_string();
            (json, text)
        }
        _ => {
            return Err(JanusError::Config(format!(
                "unknown config key '{key}'. Valid keys: github.token, linear.api_key, default.remote, semantic_search.enabled"
            )));
        }
    };

    CommandOutput::new(json_output)
        .with_text(text_output)
        .print(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_sensitive_value_ascii() {
        assert_eq!(mask_sensitive_value("abcdef"), "ab...ef");
        assert_eq!(mask_sensitive_value("12345678"), "12...78");
    }

    #[test]
    fn test_mask_sensitive_value_short() {
        assert_eq!(mask_sensitive_value("abcd"), "****");
        assert_eq!(mask_sensitive_value("abc"), "****");
        assert_eq!(mask_sensitive_value("ab"), "****");
        assert_eq!(mask_sensitive_value("a"), "****");
        assert_eq!(mask_sensitive_value(""), "****");
    }

    #[test]
    fn test_mask_sensitive_value_exactly_five_chars() {
        assert_eq!(mask_sensitive_value("abcde"), "ab...de");
    }

    #[test]
    fn test_mask_sensitive_value_multibyte_utf8() {
        // Each emoji is 4 bytes in UTF-8
        // "🔑🔒🔓🔐🗝" = 5 chars, well over 4 bytes
        assert_eq!(mask_sensitive_value("🔑🔒🔓🔐🗝"), "🔑🔒...🔐🗝");

        // Mix of ASCII and multi-byte: "aé🔑cd" = 5 chars
        assert_eq!(mask_sensitive_value("aé🔑cd"), "aé...cd");

        // Multi-byte chars at the boundaries: "émañ日本語ok" = 9 chars
        assert_eq!(mask_sensitive_value("émañ日本語ok"), "ém...ok");

        // All multi-byte, short (4 chars) → masked
        assert_eq!(mask_sensitive_value("éàöü"), "****");
    }

    #[test]
    fn test_mask_sensitive_value_two_byte_chars() {
        // "ñéàöüî" = 6 chars, each 2 bytes in UTF-8
        assert_eq!(mask_sensitive_value("ñéàöüî"), "ñé...üî");
    }

    #[test]
    fn test_mask_sensitive_value_three_byte_chars() {
        // CJK characters are 3 bytes each
        // "日本語中文" = 5 chars
        assert_eq!(mask_sensitive_value("日本語中文"), "日本...中文");
    }
}

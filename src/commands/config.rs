//! Configuration commands for managing Janus settings.
//!
//! - `config set`: Set a configuration value
//! - `config show`: Display current configuration

use owo_colors::OwoColorize;
use serde_json::json;

use super::print_json;
use crate::error::{JanusError, Result};
use crate::remote::config::{Config, Platform};

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
pub fn cmd_config_show(output_json: bool) -> Result<()> {
    let config = Config::load()?;

    if output_json {
        let default_remote_json = config.default_remote.as_ref().map(|d| {
            json!({
                "platform": d.platform.to_string(),
                "org": d.org,
                "repo": d.repo,
            })
        });

        print_json(&json!({
            "default_remote": default_remote_json,
            "auth": {
                "github_token_configured": config.github_token().is_some(),
                "linear_api_key_configured": config.linear_api_key().is_some(),
            },
            "semantic_search": {
                "enabled": config.semantic_search_enabled(),
            },
            "config_file": Config::config_path().to_string_lossy(),
        }))?;
        return Ok(());
    }

    println!("{}", "Configuration:".cyan().bold());
    println!();

    // Default remote
    if let Some(ref default) = config.default_remote {
        println!("{}:", "default_remote".cyan());
        println!("  platform: {}", default.platform);
        println!("  org: {}", default.org);
        if let Some(ref repo) = default.repo {
            println!("  repo: {repo}");
        }
    } else {
        println!("{}: {}", "default_remote".cyan(), "not configured".dimmed());
    }

    println!();

    // Auth status (don't show actual tokens)
    println!("{}:", "auth".cyan());

    let github_configured = config.github_token().is_some();
    let linear_configured = config.linear_api_key().is_some();

    println!(
        "  github.token: {}",
        if github_configured {
            "configured".green().to_string()
        } else {
            "not configured".dimmed().to_string()
        }
    );
    println!(
        "  linear.api_key: {}",
        if linear_configured {
            "configured".green().to_string()
        } else {
            "not configured".dimmed().to_string()
        }
    );

    println!();

    // Semantic search status
    println!("{}:", "semantic_search".cyan());
    println!("  enabled: {}", config.semantic_search_enabled());

    println!();
    println!(
        "{}",
        format!("Config file: {}", Config::config_path().display()).dimmed()
    );

    Ok(())
}

/// Set a configuration value
pub fn cmd_config_set(key: &str, value: &str, output_json: bool) -> Result<()> {
    validate_config_key(key)?;

    let mut config = Config::load()?;

    match key {
        "github.token" => {
            config.set_github_token(value.to_string());
            config.save()?;
            if output_json {
                print_json(&json!({
                    "action": "config_set",
                    "key": key,
                    "success": true,
                }))?;
            } else {
                println!("Set {}", "github.token".cyan());
            }
        }
        "linear.api_key" => {
            config.set_linear_api_key(value.to_string());
            config.save()?;
            if output_json {
                print_json(&json!({
                    "action": "config_set",
                    "key": key,
                    "success": true,
                }))?;
            } else {
                println!("Set {}", "linear.api_key".cyan());
            }
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

            if output_json {
                print_json(&json!({
                    "action": "config_set",
                    "key": key,
                    "value": value,
                    "success": true,
                }))?;
            } else if let Some(r) = repo {
                println!(
                    "Set {} to {}:{}/{}",
                    "default.remote".cyan(),
                    platform,
                    org,
                    r
                );
            } else {
                println!("Set {} to {}:{}", "default.remote".cyan(), platform, org);
            }
        }
        "semantic_search.enabled" => {
            let enabled = value.parse::<bool>().map_err(|_| {
                JanusError::Config(format!(
                    "invalid value '{value}' for semantic_search.enabled. Expected: true or false"
                ))
            })?;
            config.set_semantic_search_enabled(enabled);
            config.save()?;
            if output_json {
                print_json(&json!({
                    "action": "config_set",
                    "key": key,
                    "value": enabled,
                    "success": true,
                }))?;
            } else {
                println!("Set {} to {}", "semantic_search.enabled".cyan(), enabled);
            }
        }
        _ => {
            return Err(JanusError::Config(format!(
                "unknown config key '{key}'. Valid keys: github.token, linear.api_key, default.remote, semantic_search.enabled"
            )));
        }
    }

    Ok(())
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
pub fn cmd_config_get(key: &str, output_json: bool) -> Result<()> {
    validate_config_key(key)?;

    let config = Config::load()?;

    match key {
        "github.token" => {
            if let Some(token) = config.github_token() {
                let masked = mask_sensitive_value(&token);
                if output_json {
                    let output = json!({
                        "key": key,
                        "value": masked,
                        "configured": true,
                        "masked": true,
                    });
                    println!("{}", serde_json::to_string_pretty(&output)?);
                } else {
                    println!("{masked} (masked - showing first 2 and last 2 characters)");
                }
            } else {
                return Err(JanusError::Config("github.token not set".to_string()));
            }
        }
        "linear.api_key" => {
            if let Some(api_key) = config.linear_api_key() {
                let masked = mask_sensitive_value(&api_key);
                if output_json {
                    print_json(&json!({
                        "key": key,
                        "value": masked,
                        "configured": true,
                        "masked": true,
                    }))?;
                } else {
                    println!("{masked} (masked - showing first 2 and last 2 characters)");
                }
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
                if output_json {
                    print_json(&json!({
                        "key": key,
                        "value": value,
                        "configured": true,
                    }))?;
                } else {
                    println!("{value}");
                }
            } else {
                return Err(JanusError::Config("default.remote not set".to_string()));
            }
        }
        "semantic_search.enabled" => {
            let enabled = config.semantic_search_enabled();
            if output_json {
                print_json(&json!({
                    "key": key,
                    "value": enabled,
                    "configured": true,
                }))?;
            } else {
                println!("{enabled}");
            }
        }
        _ => {
            return Err(JanusError::Config(format!(
                "unknown config key '{key}'. Valid keys: github.token, linear.api_key, default.remote, semantic_search.enabled"
            )));
        }
    }

    Ok(())
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

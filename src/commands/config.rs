//! Configuration commands for managing Janus settings.
//!
//! - `config set`: Set a configuration value
//! - `config show`: Display current configuration

use owo_colors::OwoColorize;
use serde_json::json;

use super::print_json;
use crate::error::{JanusError, Result};
use crate::remote::config::{Config, Platform};

/// Mask a sensitive value by showing only the first 2 and last 2 characters
fn mask_sensitive_value(value: &str) -> String {
    if value.len() > 4 {
        format!("{}...{}", &value[..2], &value[value.len() - 2..])
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
            println!("  repo: {}", repo);
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
    println!(
        "{}",
        format!("Config file: {}", Config::config_path().display()).dimmed()
    );

    Ok(())
}

/// Set a configuration value
pub fn cmd_config_set(key: &str, value: &str, output_json: bool) -> Result<()> {
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
        "default_remote" => {
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
                    "default_remote".cyan(),
                    platform,
                    org,
                    r
                );
            } else {
                println!("Set {} to {}:{}", "default_remote".cyan(), platform, org);
            }
        }
        _ => {
            return Err(JanusError::Config(format!(
                "unknown config key '{}'. Valid keys: github.token, linear.api_key, default_remote",
                key
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
            "invalid default_remote format '{}'. Expected: platform:org or platform:org/repo",
            value
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
                    println!(
                        "{} (masked - showing first 2 and last 2 characters)",
                        masked
                    );
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
                    println!(
                        "{} (masked - showing first 2 and last 2 characters)",
                        masked
                    );
                }
            } else {
                return Err(JanusError::Config("linear.api_key not set".to_string()));
            }
        }
        "default_remote" => {
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
                    println!("{}", value);
                }
            } else {
                return Err(JanusError::Config("default_remote not set".to_string()));
            }
        }
        _ => {
            return Err(JanusError::Config(format!(
                "unknown config key '{}'. Valid keys: github.token, linear.api_key, default_remote",
                key
            )));
        }
    }

    Ok(())
}

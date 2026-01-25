//! Interactive user prompting components
//!
//! This module provides reusable components for user interaction,
//! separating CLI prompting logic from business logic.

use std::io::{self, Write};

use crate::error::Result;

/// Prompt user for yes/no confirmation
///
/// # Arguments
/// * `prompt` - The prompt message to display (without [y/N] suffix)
///
/// # Returns
/// * `true` if user confirms with 'y' or 'Y'
/// * `false` otherwise
///
/// # Example
/// ```no_run
/// # use janus::commands::interactive::confirm;
/// let confirmed = confirm("Delete this file").unwrap();
/// if confirmed {
///     // proceed with deletion
/// }
/// ```
pub fn confirm(prompt: &str) -> Result<bool> {
    print!("{}? [y/N] ", prompt);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    Ok(input.trim().eq_ignore_ascii_case("y"))
}

/// Prompt user to select from a list of options
///
/// # Arguments
/// * `prompt` - The prompt message
/// * `options` - Slice of option descriptions
/// * `default` - Optional index of default option (0-based)
///
/// # Returns
/// * Index of selected option
///
/// # Example
/// ```no_run
/// # use janus::commands::interactive::select_option;
/// let options = ["Replace", "Abort", "Skip"];
/// let choice = select_option("Choose an action", &options, Some(0)).unwrap();
/// ```
pub fn select_option(prompt: &str, options: &[&str], default: Option<usize>) -> Result<usize> {
    loop {
        if let Some(idx) = default {
            print!("{} [0-{}] (default {}): ", prompt, options.len() - 1, idx);
        } else {
            print!("{} [0-{}]: ", prompt, options.len() - 1);
        }
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        let input = input.trim();

        if input.is_empty() {
            if let Some(idx) = default {
                return Ok(idx);
            }
            println!("Please enter a number.");
            continue;
        }

        if let Ok(idx) = input.parse::<usize>()
            && idx < options.len()
        {
            return Ok(idx);
        }

        println!(
            "Invalid input. Please enter a number between 0 and {}.",
            options.len() - 1
        );
    }
}

/// Prompt user for text input
///
/// # Arguments
/// * `prompt` - The prompt message
/// * `default` - Optional default value if user just presses Enter
///
/// # Returns
/// * The user's input (or default if provided and user pressed Enter)
///
/// # Example
/// ```no_run
/// # use janus::commands::interactive::prompt_text;
/// let name = prompt_text("Enter your name", Some("Guest")).unwrap();
/// ```
pub fn prompt_text(prompt: &str, default: Option<&str>) -> Result<String> {
    if let Some(d) = default {
        print!("{} [{}]: ", prompt, d);
    } else {
        print!("{}: ", prompt);
    }
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let input = input.trim();

    if input.is_empty()
        && let Some(d) = default
    {
        return Ok(d.to_string());
    }

    Ok(input.to_string())
}

/// Prompt user for a choice with single-character shortcuts
///
/// # Arguments
/// * `prompt` - The prompt message
/// * `choices` - Slice of choices
/// * `default` - Optional default choice (first matching character)
///
/// # Returns
/// * Index of selected choice
///
/// # Example
/// ```no_run
/// # use janus::commands::interactive::prompt_choice;
/// let choices = [("l", "Local to remote"), ("r", "Remote to local"), ("s", "Skip")];
/// let choice = prompt_choice("Sync direction", &choices, Some("l")).unwrap();
/// ```
pub fn prompt_choice(
    prompt: &str,
    choices: &[(&str, &str)],
    default: Option<&str>,
) -> Result<usize> {
    loop {
        let default_str = default
            .map(|d| format!(" (default {})", d))
            .unwrap_or_default();
        print!("{}{}: ", prompt, default_str);
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        let input = input.trim().to_lowercase();

        if input.is_empty() {
            if let Some(d) = default {
                for (idx, (key, _)) in choices.iter().enumerate() {
                    if key.eq_ignore_ascii_case(d) {
                        return Ok(idx);
                    }
                }
            }
            println!("Please enter a valid choice.");
            continue;
        }

        for (idx, (key, _desc)) in choices.iter().enumerate() {
            if key.eq_ignore_ascii_case(&input) || key.starts_with(&input) {
                return Ok(idx);
            }
        }

        let valid_choices: Vec<_> = choices.iter().map(|(k, _)| *k).collect();
        println!(
            "Invalid input. Please enter one of: {}.",
            valid_choices.join(", ")
        );
    }
}

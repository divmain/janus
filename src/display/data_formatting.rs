/// Format options for ticket display
#[derive(Default)]
pub struct FormatOptions {
    pub show_priority: bool,
    pub suffix: Option<String>,
}

/// Format a date string for display
///
/// Extracts just the date part (YYYY-MM-DD) from an ISO datetime string.
/// If the string is too short, returns it unchanged.
///
/// # Examples
///
/// ```
/// use janus::display::format_date_for_display;
///
/// assert_eq!(format_date_for_display("2024-01-15T10:30:00Z"), "2024-01-15");
/// assert_eq!(format_date_for_display("2024-01-15"), "2024-01-15");
/// assert_eq!(format_date_for_display("short"), "short");
/// ```
pub fn format_date_for_display(date_str: &str) -> String {
    if date_str.len() >= 10 {
        date_str[..10].to_string()
    } else {
        date_str.to_string()
    }
}

/// Format dependencies for display
pub fn format_deps(deps: &[String]) -> String {
    let deps_str = deps.join(", ");
    if deps_str.is_empty() {
        " <- []".to_string()
    } else {
        format!(" <- [{deps_str}]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_date_for_display() {
        assert_eq!(
            format_date_for_display("2024-01-15T10:30:00Z"),
            "2024-01-15"
        );
        assert_eq!(format_date_for_display("2024-01-15"), "2024-01-15");
        assert_eq!(format_date_for_display("short"), "short");
        assert_eq!(format_date_for_display(""), "");
    }
}

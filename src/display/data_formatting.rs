use std::ops::Range;

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

/// Extract body content from ticket file (everything after frontmatter and title)
///
/// Returns `None` if frontmatter is not found or malformed.
/// Skips the title line (starts with #) if present.
///
/// # Arguments
///
/// * `content` - The full ticket file content including YAML frontmatter
///
/// # Examples
///
/// ```
/// use janus::display::extract_ticket_body;
///
/// let content = r#"---
/// id: test
/// status: new
/// ---
/// # Test Title
///
/// This is the body.
/// With multiple lines.
/// "#;
///
/// let body = extract_ticket_body(content);
/// assert!(body.is_some());
/// assert!(body.unwrap().contains("This is the body"));
/// ```
pub fn extract_ticket_body(content: &str) -> Option<String> {
    use crate::parser::{TITLE_RE, split_frontmatter};

    let (_frontmatter, body_with_title) = split_frontmatter(content).ok()?;

    let title_re = TITLE_RE.clone();
    let body = title_re.replace(&body_with_title, "").to_string();

    Some(body.trim().to_string())
}

/// Extract title range from ticket file content
///
/// Returns the byte range of the title line (the first H1 after frontmatter)
/// Returns `None` if no title is found.
///
/// # Arguments
///
/// * `content` - The full ticket file content including YAML frontmatter
pub fn extract_title_range(content: &str) -> Option<Range<usize>> {
    use crate::parser::split_frontmatter;

    let (_frontmatter, body_with_title) = split_frontmatter(content).ok()?;

    let title_re = crate::parser::TITLE_RE.clone();
    let title_captures = title_re.captures(&body_with_title)?;
    let title_text = title_captures.get(1)?.as_str();

    let title_with_hash = format!("# {title_text}");
    let start = content.find(&title_with_hash)?;
    let end = start + title_with_hash.len();

    Some(start..end)
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

/// Sort tickets by priority (ascending) then by ID
pub fn sort_by_priority(tickets: &mut [crate::types::TicketMetadata]) {
    tickets.sort_by(|a, b| {
        let pa = a.priority_num();
        let pb = b.priority_num();
        if pa != pb {
            pa.cmp(&pb)
        } else {
            a.id.cmp(&b.id)
        }
    });
}

/// Sort tickets by creation date (newest first) then by ID
pub fn sort_by_created(tickets: &mut [crate::types::TicketMetadata]) {
    tickets.sort_by(|a, b| match (&a.created, &b.created) {
        (Some(date_a), Some(date_b)) => date_b.cmp(date_a),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.id.cmp(&b.id),
    });
}

/// Sort tickets by ID (alphabetical)
pub fn sort_by_id(tickets: &mut [crate::types::TicketMetadata]) {
    tickets.sort_by(|a, b| a.id.cmp(&b.id));
}

/// Sort tickets by the specified field
pub fn sort_tickets_by(tickets: &mut [crate::types::TicketMetadata], sort_by: &str) {
    match sort_by {
        "created" => sort_by_created(tickets),
        "id" => sort_by_id(tickets),
        "priority" => sort_by_priority(tickets),
        _ => sort_by_priority(tickets),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{TicketMetadata, TicketPriority};

    #[test]
    fn test_sort_by_priority() {
        let mut tickets = vec![
            TicketMetadata {
                id: Some("j-3".to_string()),
                priority: Some(TicketPriority::P3),
                ..Default::default()
            },
            TicketMetadata {
                id: Some("j-1".to_string()),
                priority: Some(TicketPriority::P0),
                ..Default::default()
            },
            TicketMetadata {
                id: Some("j-2".to_string()),
                priority: Some(TicketPriority::P1),
                ..Default::default()
            },
        ];

        sort_by_priority(&mut tickets);

        assert_eq!(tickets[0].id, Some("j-1".to_string()));
        assert_eq!(tickets[1].id, Some("j-2".to_string()));
        assert_eq!(tickets[2].id, Some("j-3".to_string()));
    }

    #[test]
    fn test_sort_by_created() {
        let mut tickets = vec![
            TicketMetadata {
                id: Some("j-old".to_string()),
                created: Some("2024-01-01T00:00:00Z".to_string()),
                ..Default::default()
            },
            TicketMetadata {
                id: Some("j-new".to_string()),
                created: Some("2024-12-01T00:00:00Z".to_string()),
                ..Default::default()
            },
            TicketMetadata {
                id: Some("j-mid".to_string()),
                created: Some("2024-06-01T00:00:00Z".to_string()),
                ..Default::default()
            },
        ];

        sort_by_created(&mut tickets);

        assert_eq!(tickets[0].id, Some("j-new".to_string()));
        assert_eq!(tickets[1].id, Some("j-mid".to_string()));
        assert_eq!(tickets[2].id, Some("j-old".to_string()));
    }

    #[test]
    fn test_sort_by_id() {
        let mut tickets = vec![
            TicketMetadata {
                id: Some("j-zebra".to_string()),
                ..Default::default()
            },
            TicketMetadata {
                id: Some("j-alpha".to_string()),
                ..Default::default()
            },
            TicketMetadata {
                id: Some("j-middle".to_string()),
                ..Default::default()
            },
        ];

        sort_by_id(&mut tickets);

        assert_eq!(tickets[0].id, Some("j-alpha".to_string()));
        assert_eq!(tickets[1].id, Some("j-middle".to_string()));
        assert_eq!(tickets[2].id, Some("j-zebra".to_string()));
    }

    #[test]
    fn test_sort_tickets_by_all_options() {
        let mut tickets1 = vec![
            TicketMetadata {
                id: Some("j-3".to_string()),
                priority: Some(TicketPriority::P3),
                ..Default::default()
            },
            TicketMetadata {
                id: Some("j-1".to_string()),
                priority: Some(TicketPriority::P0),
                ..Default::default()
            },
        ];
        sort_tickets_by(&mut tickets1, "priority");
        assert_eq!(tickets1[0].id, Some("j-1".to_string()));

        let mut tickets2 = vec![
            TicketMetadata {
                id: Some("j-old".to_string()),
                created: Some("2024-01-01T00:00:00Z".to_string()),
                ..Default::default()
            },
            TicketMetadata {
                id: Some("j-new".to_string()),
                created: Some("2024-12-01T00:00:00Z".to_string()),
                ..Default::default()
            },
        ];
        sort_tickets_by(&mut tickets2, "created");
        assert_eq!(tickets2[0].id, Some("j-new".to_string()));

        let mut tickets3 = vec![
            TicketMetadata {
                id: Some("j-zebra".to_string()),
                ..Default::default()
            },
            TicketMetadata {
                id: Some("j-alpha".to_string()),
                ..Default::default()
            },
        ];
        sort_tickets_by(&mut tickets3, "id");
        assert_eq!(tickets3[0].id, Some("j-alpha".to_string()));

        let mut tickets4 = vec![
            TicketMetadata {
                id: Some("j-3".to_string()),
                priority: Some(TicketPriority::P3),
                ..Default::default()
            },
            TicketMetadata {
                id: Some("j-1".to_string()),
                priority: Some(TicketPriority::P0),
                ..Default::default()
            },
        ];
        sort_tickets_by(&mut tickets4, "invalid_option");
        assert_eq!(tickets4[0].id, Some("j-1".to_string()));
    }

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

    #[test]
    fn test_extract_ticket_body() {
        let content = r#"---
id: test
status: new
---
# Test Title

This is the body.
With multiple lines.
"#;
        let body = extract_ticket_body(content).unwrap();
        assert!(body.contains("This is the body"));
        assert!(body.contains("With multiple lines"));
        assert!(!body.contains("Test Title"));
    }

    #[test]
    fn test_extract_ticket_body_no_title() {
        let content = r#"---
id: test
---
No title here, just body.
"#;
        let body = extract_ticket_body(content).unwrap();
        assert!(body.contains("No title here"));
    }

    #[test]
    fn test_extract_ticket_body_no_frontmatter() {
        let content = "No frontmatter here";
        assert!(extract_ticket_body(content).is_none());
    }

    #[test]
    fn test_extract_title_range() {
        let content = r#"---
id: test
---
# My Ticket Title

Body content
"#;
        let range = extract_title_range(content);
        assert!(range.is_some());
        let title = &content[range.unwrap()];
        assert_eq!(title, "# My Ticket Title");
    }

    #[test]
    fn test_extract_title_range_no_title() {
        let content = r#"---
id: test
---
Body without title
"#;
        assert!(extract_title_range(content).is_none());
    }
}

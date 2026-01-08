use std::fs;
use std::path::PathBuf;

use serde_json::json;

use crate::error::Result;
use crate::types::{TICKETS_ITEMS_DIR, TicketPriority, TicketType};
use crate::utils::{
    ensure_dir, generate_id_with_custom_prefix, generate_uuid, get_git_user_name, iso_date,
};

/// Options for creating a new ticket
pub struct CreateOptions {
    pub title: String,
    pub description: Option<String>,
    pub design: Option<String>,
    pub acceptance: Option<String>,
    pub priority: TicketPriority,
    pub ticket_type: TicketType,
    pub assignee: Option<String>,
    pub external_ref: Option<String>,
    pub parent: Option<String>,
    pub prefix: Option<String>,
}

impl Default for CreateOptions {
    fn default() -> Self {
        CreateOptions {
            title: "Untitled".to_string(),
            description: None,
            design: None,
            acceptance: None,
            priority: TicketPriority::P2,
            ticket_type: TicketType::Task,
            assignee: None,
            external_ref: None,
            parent: None,
            prefix: None,
        }
    }
}

/// Create a new ticket and print its ID
pub fn cmd_create(options: CreateOptions, output_json: bool) -> Result<()> {
    ensure_dir()?;

    let assignee = options.assignee.or_else(get_git_user_name);
    let id = generate_id_with_custom_prefix(options.prefix.as_deref())?;
    let uuid = generate_uuid();
    let now = iso_date();

    // Build frontmatter
    let mut frontmatter_lines = vec![
        "---".to_string(),
        format!("id: {}", id),
        format!("uuid: {}", uuid),
        "status: new".to_string(),
        "deps: []".to_string(),
        "links: []".to_string(),
        format!("created: {}", now),
        format!("type: {}", options.ticket_type),
        format!("priority: {}", options.priority),
    ];

    if let Some(ref a) = assignee {
        frontmatter_lines.push(format!("assignee: {}", a));
    }
    if let Some(ref ext) = options.external_ref {
        frontmatter_lines.push(format!("external-ref: {}", ext));
    }
    if let Some(ref parent) = options.parent {
        frontmatter_lines.push(format!("parent: {}", parent));
    }

    frontmatter_lines.push("---".to_string());

    let frontmatter = frontmatter_lines.join("\n");

    // Build body sections
    let mut sections = vec![format!("# {}", options.title)];

    if let Some(ref desc) = options.description {
        sections.push(format!("\n{}", desc));
    }
    if let Some(ref design) = options.design {
        sections.push(format!("\n## Design\n\n{}", design));
    }
    if let Some(ref acceptance) = options.acceptance {
        sections.push(format!("\n## Acceptance Criteria\n\n{}", acceptance));
    }

    let body = sections.join("\n");
    let content = format!("{}\n{}\n", frontmatter, body);

    let file_path = PathBuf::from(TICKETS_ITEMS_DIR).join(format!("{}.md", id));
    fs::create_dir_all(TICKETS_ITEMS_DIR)?;
    fs::write(&file_path, content)?;

    if output_json {
        let output = json!({
            "id": id,
            "uuid": uuid,
            "title": options.title,
            "status": "new",
            "type": options.ticket_type.to_string(),
            "priority": options.priority.as_num(),
            "assignee": assignee,
            "created": now,
            "file_path": file_path.to_string_lossy(),
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", id);
    }
    Ok(())
}

use serde_json::json;

use super::CommandOutput;
use crate::error::Result;
use crate::ticket::TicketBuilder;
use crate::types::{TicketPriority, TicketType};

/// Options for creating a new ticket
pub struct CreateOptions {
    pub title: String,
    pub description: Option<String>,
    pub design: Option<String>,
    pub acceptance: Option<String>,
    pub priority: TicketPriority,
    pub ticket_type: TicketType,
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
            external_ref: None,
            parent: None,
            prefix: None,
        }
    }
}

/// Create a new ticket and print its ID
pub fn cmd_create(options: CreateOptions, output_json: bool) -> Result<()> {
    let (id, file_path) = TicketBuilder::new(&options.title)
        .description(options.description.as_deref())
        .design(options.design.as_deref())
        .acceptance(options.acceptance.as_deref())
        .prefix(options.prefix.as_deref())
        .ticket_type(options.ticket_type.to_string())
        .priority(options.priority.as_num().to_string())
        .external_ref(options.external_ref.as_deref())
        .parent(options.parent.as_deref())
        .run_hooks(true)
        .build()?;

    CommandOutput::new(json!({
        "id": id,
        "title": options.title,
        "status": "new",
        "type": options.ticket_type.to_string(),
        "priority": options.priority.as_num(),
        "file_path": file_path.to_string_lossy(),
    }))
    .with_text(&id)
    .print(output_json)
}

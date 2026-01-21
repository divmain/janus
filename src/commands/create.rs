use serde_json::json;

use super::CommandOutput;
use crate::error::Result;
use crate::events::log_ticket_created;
use crate::ticket::{TicketBuilder, parse_ticket};
use crate::types::{TicketPriority, TicketType, tickets_items_dir};

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
    pub spawned_from: Option<String>,
    pub spawn_context: Option<String>,
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
            spawned_from: None,
            spawn_context: None,
        }
    }
}

/// Compute the depth for a spawned ticket based on the parent's depth.
/// Returns None if no spawned_from is provided, or parent.depth + 1 otherwise.
/// If spawned_from is provided but the parent can't be found or read, defaults to depth 1.
fn compute_depth(spawned_from: Option<&str>) -> Option<u32> {
    let spawned_from_id = spawned_from?;

    // Try to find and read the parent ticket from disk
    let parent_path = tickets_items_dir().join(format!("{}.md", spawned_from_id));

    if let Ok(content) = std::fs::read_to_string(&parent_path)
        && let Ok(parent_meta) = parse_ticket(&content)
    {
        // If parent has a depth, add 1; otherwise this is depth 1 (parent is implicitly depth 0)
        return Some(parent_meta.depth.unwrap_or(0) + 1);
    }

    // If we can't find the parent, still set depth to 1 (parent is implicitly depth 0)
    Some(1)
}

/// Create a new ticket and print its ID
pub fn cmd_create(options: CreateOptions, output_json: bool) -> Result<()> {
    // Auto-compute depth if spawned_from is provided
    let depth = compute_depth(options.spawned_from.as_deref());

    let (id, file_path) = TicketBuilder::new(&options.title)
        .description(options.description.as_deref())
        .design(options.design.as_deref())
        .acceptance(options.acceptance.as_deref())
        .prefix(options.prefix.as_deref())
        .ticket_type(options.ticket_type.to_string())
        .priority(options.priority.as_num().to_string())
        .external_ref(options.external_ref.as_deref())
        .parent(options.parent.as_deref())
        .spawned_from(options.spawned_from.as_deref())
        .spawn_context(options.spawn_context.as_deref())
        .depth(depth)
        .run_hooks(true)
        .build()?;

    // Log the event
    log_ticket_created(
        &id,
        &options.title,
        &options.ticket_type.to_string(),
        options.priority.as_num(),
        options.spawned_from.as_deref(),
    );

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

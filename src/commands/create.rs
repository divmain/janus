use serde_json::json;

use super::CommandOutput;
use crate::error::{JanusError, Result};
use crate::events::log_ticket_created;
use crate::ticket::{TicketBuilder, parse_ticket};
use crate::types::{TicketPriority, TicketType, tickets_items_dir};

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
pub fn cmd_create(
    title: String,
    description: Option<String>,
    design: Option<String>,
    acceptance: Option<String>,
    priority: TicketPriority,
    ticket_type: TicketType,
    external_ref: Option<String>,
    parent: Option<String>,
    prefix: Option<String>,
    spawned_from: Option<String>,
    spawn_context: Option<String>,
    output_json: bool,
) -> Result<()> {
    // Validate that title is not empty or only whitespace
    if title.trim().is_empty() {
        return Err(JanusError::EmptyTitle);
    }

    // Auto-compute depth if spawned_from is provided
    let depth = compute_depth(spawned_from.as_deref());

    let (id, file_path) = TicketBuilder::new(&title)
        .description(description.as_deref())
        .design(design.as_deref())
        .acceptance(acceptance.as_deref())
        .prefix(prefix.as_deref())
        .ticket_type(ticket_type.to_string())
        .priority(priority.as_num().to_string())
        .external_ref(external_ref.as_deref())
        .parent(parent.as_deref())
        .spawned_from(spawned_from.as_deref())
        .spawn_context(spawn_context.as_deref())
        .depth(depth)
        .run_hooks(true)
        .build()?;

    // Log the event
    log_ticket_created(
        &id,
        &title,
        &ticket_type.to_string(),
        priority.as_num(),
        spawned_from.as_deref(),
    );

    CommandOutput::new(json!({
        "id": id,
        "title": title,
        "status": "new",
        "type": ticket_type.to_string(),
        "priority": priority.as_num(),
        "file_path": file_path.to_string_lossy(),
    }))
    .with_text(&id)
    .print(output_json)
}

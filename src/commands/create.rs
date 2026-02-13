use serde_json::json;

use super::CommandOutput;
use crate::cli::OutputOptions;
use crate::error::{JanusError, Result};
use crate::events::log_ticket_created;
use crate::ticket::{Ticket, TicketBuilder, parse_ticket};
use crate::types::{TicketPriority, TicketSize, TicketType, tickets_items_dir};

/// Options for the `create` command, bundling all parameters.
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
    pub size: Option<TicketSize>,
    pub output: OutputOptions,
}

/// Compute the depth for a spawned ticket based on the parent's resolved canonical ID.
/// Returns None if no spawned_from is provided, or parent.depth + 1 otherwise.
/// If the parent ticket can't be read, prints a warning to stderr and defaults to depth 1.
fn compute_depth(canonical_id: Option<&str>) -> Option<u32> {
    let id = canonical_id?;

    // Try to find and read the parent ticket from disk
    let parent_path = tickets_items_dir().join(format!("{id}.md"));

    if let Ok(content) = std::fs::read_to_string(&parent_path)
        && let Ok(parent_meta) = parse_ticket(&content)
    {
        // If parent has a depth, add 1; otherwise this is depth 1 (parent is implicitly depth 0)
        return Some(parent_meta.depth.unwrap_or(0) + 1);
    }

    // If we can't read the parent, warn and default to depth 1
    eprintln!(
        "Warning: could not read parent ticket '{id}' for depth calculation; defaulting to depth 1"
    );
    Some(1)
}

/// Create a new ticket and print its ID
pub async fn cmd_create(opts: CreateOptions) -> Result<()> {
    let CreateOptions {
        title,
        description,
        design,
        acceptance,
        priority,
        ticket_type,
        external_ref,
        parent,
        prefix,
        spawned_from,
        spawn_context,
        size,
        output,
    } = opts;

    // Validate that title is not empty or only whitespace
    if title.trim().is_empty() {
        return Err(JanusError::EmptyTitle);
    }

    // Validate that title does not exceed maximum length
    const MAX_TITLE_LENGTH: usize = 500;
    if title.len() > MAX_TITLE_LENGTH {
        return Err(JanusError::TicketTitleTooLong {
            max: MAX_TITLE_LENGTH,
            actual: title.len(),
        });
    }

    // Resolve spawned_from to canonical ticket ID if provided
    let resolved_spawned_from = if let Some(ref partial_id) = spawned_from {
        Some(Ticket::resolve_partial_id(partial_id).await?)
    } else {
        None
    };

    // Auto-compute depth if spawned_from is provided
    let depth = compute_depth(resolved_spawned_from.as_deref());

    let (id, file_path) = TicketBuilder::new(&title)
        .description(description.as_deref())
        .design(design.as_deref())
        .acceptance(acceptance.as_deref())
        .prefix(prefix.as_deref())
        .ticket_type(ticket_type)
        .priority(priority)
        .external_ref(external_ref.as_deref())
        .parent(parent.as_deref())
        .spawned_from(resolved_spawned_from.as_deref())
        .spawn_context(spawn_context.as_deref())
        .depth(depth)
        .size(size)
        .run_hooks(true)
        .build()?;

    // Log the event
    log_ticket_created(
        &id,
        &title,
        &ticket_type.to_string(),
        priority.as_num(),
        resolved_spawned_from.as_deref(),
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
    .print(output)
}

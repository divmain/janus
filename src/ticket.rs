use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::cache;
use crate::error::{JanusError, Result};
use crate::hooks::{HookContext, HookEvent, ItemType, run_post_hooks, run_pre_hooks};
use crate::parser::parse_ticket_content;
use crate::types::{TICKETS_ITEMS_DIR, TicketMetadata};
use crate::utils;

/// Find all ticket files in the tickets directory
fn find_tickets() -> Vec<String> {
    fs::read_dir(TICKETS_ITEMS_DIR)
        .ok()
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    if name.ends_with(".md") {
                        Some(name)
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Find a ticket file by partial ID
pub async fn find_ticket_by_id(partial_id: &str) -> Result<PathBuf> {
    // Try cache first
    if let Some(cache) = cache::get_or_init_cache().await {
        // Exact match check - file exists?
        let exact_match_path = PathBuf::from(TICKETS_ITEMS_DIR).join(format!("{}.md", partial_id));
        if exact_match_path.exists() {
            return Ok(exact_match_path);
        }

        // Partial match via cache
        if let Ok(matches) = cache.find_by_partial_id(partial_id).await {
            match matches.len() {
                0 => {}
                1 => {
                    let filename = format!("{}.md", &matches[0]);
                    return Ok(PathBuf::from(TICKETS_ITEMS_DIR).join(filename));
                }
                _ => return Err(JanusError::AmbiguousId(partial_id.to_string())),
            }
        }
    }

    // FALLBACK: Original file-based implementation
    let files = find_tickets();

    // Check for exact match first
    let exact_name = format!("{}.md", partial_id);
    if files.iter().any(|f| f == &exact_name) {
        return Ok(PathBuf::from(TICKETS_ITEMS_DIR).join(&exact_name));
    }

    // Then check for partial matches
    let matches: Vec<_> = files.iter().filter(|f| f.contains(partial_id)).collect();

    match matches.len() {
        0 => Err(JanusError::TicketNotFound(partial_id.to_string())),
        1 => Ok(PathBuf::from(TICKETS_ITEMS_DIR).join(matches[0])),
        _ => Err(JanusError::AmbiguousId(partial_id.to_string())),
    }
}

/// Find a ticket file by partial ID (sync wrapper for backward compatibility)
pub fn find_ticket_by_id_sync(partial_id: &str) -> Result<PathBuf> {
    // Try to use cache if we're in a tokio runtime, otherwise use fallback
    use tokio::runtime::Handle;

    // Check if we're already in a tokio runtime
    if Handle::try_current().is_err() {
        // Not in a tokio runtime, create one
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| JanusError::Other(format!("Failed to create tokio runtime: {}", e)))?;
        return rt.block_on(find_ticket_by_id(partial_id));
    }

    // We're in a tokio runtime, cannot use block_on
    // This shouldn't happen if called from async functions properly
    // Fall back to file-based implementation
    let files = find_tickets();

    let exact_name = format!("{}.md", partial_id);
    if files.iter().any(|f| f == &exact_name) {
        return Ok(PathBuf::from(TICKETS_ITEMS_DIR).join(&exact_name));
    }

    let matches: Vec<_> = files.iter().filter(|f| f.contains(partial_id)).collect();

    match matches.len() {
        0 => Err(JanusError::TicketNotFound(partial_id.to_string())),
        1 => Ok(PathBuf::from(TICKETS_ITEMS_DIR).join(matches[0])),
        _ => Err(JanusError::AmbiguousId(partial_id.to_string())),
    }
}

/// A ticket handle for reading and writing ticket files
pub struct Ticket {
    pub file_path: PathBuf,
    pub id: String,
}

impl Ticket {
    /// Find a ticket by its (partial) ID
    pub fn find(partial_id: &str) -> Result<Self> {
        let file_path = find_ticket_by_id_sync(partial_id)?;
        Ok(Ticket::new(file_path))
    }

    /// Find a ticket by its (partial) ID (async version)
    pub async fn find_async(partial_id: &str) -> Result<Self> {
        let file_path = find_ticket_by_id(partial_id).await?;
        Ok(Ticket::new(file_path))
    }

    /// Create a ticket handle for a given file path
    pub fn new(file_path: PathBuf) -> Self {
        let id = file_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ticket { file_path, id }
    }

    /// Read and parse the ticket's metadata
    pub fn read(&self) -> Result<TicketMetadata> {
        let content = fs::read_to_string(&self.file_path)?;
        let mut metadata = parse_ticket_content(&content)?;
        metadata.file_path = Some(self.file_path.clone());
        Ok(metadata)
    }

    /// Read the raw content of the ticket file
    pub fn read_content(&self) -> Result<String> {
        Ok(fs::read_to_string(&self.file_path)?)
    }

    /// Execute a write operation surrounded by hooks
    ///
    /// This method encapsulates the standard hook lifecycle:
    /// 1. Run PreWrite hook
    /// 2. Execute the provided operation
    /// 3. Run PostWrite hook
    /// 4. Run TicketUpdated hook (or a custom post-hook event)
    fn with_write_hooks<F>(
        &self,
        context: HookContext,
        operation: F,
        post_hook_event: Option<HookEvent>,
    ) -> Result<()>
    where
        F: FnOnce() -> Result<()>,
    {
        run_pre_hooks(HookEvent::PreWrite, &context)?;

        operation()?;

        run_post_hooks(HookEvent::PostWrite, &context);
        if let Some(event) = post_hook_event {
            run_post_hooks(event, &context);
        }

        Ok(())
    }

    /// Write content to the ticket file
    ///
    /// This method triggers `PreWrite` hook before writing, and `PostWrite` + `TicketUpdated`
    /// hooks after successful write.
    pub fn write(&self, content: &str) -> Result<()> {
        let context = HookContext::new()
            .with_item_type(ItemType::Ticket)
            .with_item_id(&self.id)
            .with_file_path(&self.file_path);

        self.with_write_hooks(
            context,
            || {
                if let Some(parent) = self.file_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&self.file_path, content)?;
                Ok(())
            },
            Some(HookEvent::TicketUpdated),
        )
    }

    /// Update a single field in the frontmatter
    ///
    /// This method triggers `PreWrite` hook before writing, and `PostWrite` + `TicketUpdated`
    /// hooks after successful write. The hook context includes field_name, old_value, and new_value.
    pub fn update_field(&self, field: &str, value: &str) -> Result<()> {
        let content = fs::read_to_string(&self.file_path)?;
        let field_pattern = Regex::new(&format!(r"(?m)^{}:\s*.*$", regex::escape(field))).unwrap();

        let old_value = field_pattern.find(&content).map(|m| {
            m.as_str()
                .split(':')
                .nth(1)
                .map(|v| v.trim().to_string())
                .unwrap_or_default()
        });

        let mut context = HookContext::new()
            .with_item_type(ItemType::Ticket)
            .with_item_id(&self.id)
            .with_file_path(&self.file_path)
            .with_field_name(field)
            .with_new_value(value);

        if let Some(ref old_val) = old_value {
            context = context.with_old_value(old_val);
        }

        self.with_write_hooks(
            context,
            || {
                let new_content = if field_pattern.is_match(&content) {
                    field_pattern
                        .replace(&content, format!("{}: {}", field, value))
                        .into_owned()
                } else {
                    content.replacen("---\n", &format!("---\n{}: {}\n", field, value), 1)
                };

                fs::write(&self.file_path, new_content)?;
                Ok(())
            },
            Some(HookEvent::TicketUpdated),
        )
    }

    /// Remove a field from the frontmatter
    ///
    /// This method triggers `PreWrite` hook before writing, and `PostWrite` + `TicketUpdated`
    /// hooks after successful write. The hook context includes field_name and old_value.
    pub fn remove_field(&self, field: &str) -> Result<()> {
        let content = fs::read_to_string(&self.file_path)?;
        let field_pattern =
            Regex::new(&format!(r"(?m)^{}:\s*.*\n?", regex::escape(field))).unwrap();

        let old_value = field_pattern.find(&content).map(|m| {
            m.as_str()
                .split(':')
                .nth(1)
                .map(|v| v.trim().trim_end_matches('\n').to_string())
                .unwrap_or_default()
        });

        let mut context = HookContext::new()
            .with_item_type(ItemType::Ticket)
            .with_item_id(&self.id)
            .with_file_path(&self.file_path)
            .with_field_name(field);

        if let Some(ref old_val) = old_value {
            context = context.with_old_value(old_val);
        }

        self.with_write_hooks(
            context,
            || {
                let new_content = field_pattern.replace(&content, "").into_owned();
                fs::write(&self.file_path, new_content)?;
                Ok(())
            },
            Some(HookEvent::TicketUpdated),
        )
    }

    /// Add a value to an array field (deps, links)
    ///
    /// This method triggers `PreWrite` hook before writing, and `PostWrite` + `TicketUpdated`
    /// hooks after successful write. The hook context includes field_name and new_value.
    pub fn add_to_array_field(&self, field: &str, value: &str) -> Result<bool> {
        let metadata = self.read()?;
        let current_array = match field {
            "deps" => &metadata.deps,
            "links" => &metadata.links,
            _ => return Err(JanusError::Other(format!("unknown array field: {}", field))),
        };

        if current_array.contains(&value.to_string()) {
            return Ok(false);
        }

        let context = HookContext::new()
            .with_item_type(ItemType::Ticket)
            .with_item_id(&self.id)
            .with_file_path(&self.file_path)
            .with_field_name(field)
            .with_new_value(value);

        self.with_write_hooks(
            context,
            || {
                let mut new_array = current_array.clone();
                new_array.push(value.to_string());
                self.update_field_internal(field, &serde_json::to_string(&new_array)?)
            },
            Some(HookEvent::TicketUpdated),
        )?;

        Ok(true)
    }

    /// Remove a value from an array field (deps, links)
    ///
    /// This method triggers `PreWrite` hook before writing, and `PostWrite` + `TicketUpdated`
    /// hooks after successful write. The hook context includes field_name and old_value.
    pub fn remove_from_array_field(&self, field: &str, value: &str) -> Result<bool> {
        let metadata = self.read()?;
        let current_array = match field {
            "deps" => &metadata.deps,
            "links" => &metadata.links,
            _ => return Err(JanusError::Other(format!("unknown array field: {}", field))),
        };

        if !current_array.contains(&value.to_string()) {
            return Ok(false);
        }

        let context = HookContext::new()
            .with_item_type(ItemType::Ticket)
            .with_item_id(&self.id)
            .with_file_path(&self.file_path)
            .with_field_name(field)
            .with_old_value(value);

        self.with_write_hooks(
            context,
            || {
                let new_array: Vec<_> = current_array
                    .iter()
                    .filter(|v| v.as_str() != value)
                    .collect();
                let json_value = if new_array.is_empty() {
                    "[]".to_string()
                } else {
                    serde_json::to_string(&new_array)?
                };

                self.update_field_internal(field, &json_value)
            },
            Some(HookEvent::TicketUpdated),
        )?;

        Ok(true)
    }

    /// Internal method to update a field without triggering hooks.
    /// Used by array field methods which handle their own hook calls.
    fn update_field_internal(&self, field: &str, value: &str) -> Result<()> {
        let content = fs::read_to_string(&self.file_path)?;
        let field_pattern = Regex::new(&format!(r"(?m)^{}:\s*.*$", regex::escape(field))).unwrap();

        let new_content = if field_pattern.is_match(&content) {
            field_pattern
                .replace(&content, format!("{}: {}", field, value))
                .into_owned()
        } else {
            // Add field after opening ---
            content.replacen("---\n", &format!("---\n{}: {}\n", field, value), 1)
        };

        fs::write(&self.file_path, new_content)?;
        Ok(())
    }
}

/// Get all tickets from the tickets directory (original implementation, used as fallback)
pub fn get_all_tickets_from_disk() -> Vec<TicketMetadata> {
    let files = find_tickets();
    let mut tickets = Vec::new();

    for file in files {
        let file_path = PathBuf::from(TICKETS_ITEMS_DIR).join(&file);
        match fs::read_to_string(&file_path) {
            Ok(content) => match parse_ticket_content(&content) {
                Ok(mut metadata) => {
                    metadata.id = Some(file.strip_suffix(".md").unwrap_or(&file).to_string());
                    metadata.file_path = Some(file_path);
                    tickets.push(metadata);
                }
                Err(e) => {
                    eprintln!("Warning: failed to parse {}: {}", file, e);
                }
            },
            Err(e) => {
                eprintln!("Warning: failed to read {}: {}", file, e);
            }
        }
    }

    tickets
}

/// Get all tickets from the tickets directory
pub async fn get_all_tickets() -> Vec<TicketMetadata> {
    // Try cache first
    if let Some(cache) = cache::get_or_init_cache().await {
        if let Ok(tickets) = cache.get_all_tickets().await {
            return tickets;
        }
        eprintln!("Warning: cache read failed, falling back to file reads");
    }

    // FALLBACK: Original implementation
    get_all_tickets_from_disk()
}

/// Get all tickets (sync wrapper for backward compatibility)
pub fn get_all_tickets_sync() -> Vec<TicketMetadata> {
    use tokio::runtime::Handle;

    if Handle::try_current().is_err() {
        let rt = tokio::runtime::Runtime::new().ok();
        if let Some(rt) = rt {
            return rt.block_on(get_all_tickets());
        }
    }

    // Fallback to disk reads
    get_all_tickets_from_disk()
}

/// Build a map of all tickets by ID
pub async fn build_ticket_map() -> HashMap<String, TicketMetadata> {
    // Try cache first
    if let Some(cache) = cache::get_or_init_cache().await {
        if let Ok(map) = cache.build_ticket_map().await {
            return map;
        }
        eprintln!("Warning: cache read failed, falling back to file reads");
    }

    // FALLBACK: Use get_all_tickets which has cache logic
    get_all_tickets()
        .await
        .into_iter()
        .filter_map(|t| t.id.clone().map(|id| (id, t)))
        .collect()
}

/// Build a map of all tickets by ID (sync wrapper for backward compatibility)
pub fn build_ticket_map_sync() -> HashMap<String, TicketMetadata> {
    use tokio::runtime::Handle;

    if Handle::try_current().is_err() {
        let rt = tokio::runtime::Runtime::new().ok();
        if let Some(rt) = rt {
            return rt.block_on(build_ticket_map());
        }
    }

    // Fallback to disk reads
    get_all_tickets_from_disk()
        .into_iter()
        .filter_map(|t| t.id.clone().map(|id| (id, t)))
        .collect()
}

/// Get file stats (modification time) for a path
pub fn get_file_mtime(path: &Path) -> Option<std::time::SystemTime> {
    fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

/// Builder for creating new tickets with customizable fields
pub struct TicketBuilder {
    title: String,
    description: Option<String>,
    design: Option<String>,
    acceptance: Option<String>,
    prefix: Option<String>,
    ticket_type: Option<String>,
    status: Option<String>,
    priority: Option<String>,
    external_ref: Option<String>,
    parent: Option<String>,
    remote: Option<String>,
    include_uuid: bool,
    uuid: Option<String>,
    created: Option<String>,
    run_hooks: bool,
}

impl TicketBuilder {
    /// Create a new ticket builder with a title
    pub fn new(title: impl Into<String>) -> Self {
        TicketBuilder {
            title: title.into(),
            description: None,
            design: None,
            acceptance: None,
            prefix: None,
            ticket_type: None,
            status: None,
            priority: None,
            external_ref: None,
            parent: None,
            remote: None,
            include_uuid: true,
            uuid: None,
            created: None,
            run_hooks: true,
        }
    }

    /// Set the description
    pub fn description(mut self, desc: Option<impl Into<String>>) -> Self {
        self.description = desc.map(|d| d.into());
        self
    }

    /// Set the design section
    pub fn design(mut self, design: Option<impl Into<String>>) -> Self {
        self.design = design.map(|d| d.into());
        self
    }

    /// Set the acceptance criteria section
    pub fn acceptance(mut self, acceptance: Option<impl Into<String>>) -> Self {
        self.acceptance = acceptance.map(|a| a.into());
        self
    }

    /// Set the custom prefix for ticket ID
    pub fn prefix(mut self, prefix: Option<impl Into<String>>) -> Self {
        self.prefix = prefix.map(|p| p.into());
        self
    }

    /// Set the ticket type
    pub fn ticket_type(mut self, ticket_type: impl Into<String>) -> Self {
        self.ticket_type = Some(ticket_type.into());
        self
    }

    /// Set the status
    pub fn status(mut self, status: impl Into<String>) -> Self {
        self.status = Some(status.into());
        self
    }

    /// Set the priority
    pub fn priority(mut self, priority: impl Into<String>) -> Self {
        self.priority = Some(priority.into());
        self
    }

    /// Set the external reference
    pub fn external_ref(mut self, external_ref: Option<impl Into<String>>) -> Self {
        self.external_ref = external_ref.map(|r| r.into());
        self
    }

    /// Set the parent ticket
    pub fn parent(mut self, parent: Option<impl Into<String>>) -> Self {
        self.parent = parent.map(|p| p.into());
        self
    }

    /// Set the remote reference
    pub fn remote(mut self, remote: Option<impl Into<String>>) -> Self {
        self.remote = remote.map(|r| r.into());
        self
    }

    /// Set whether to include a UUID (true by default)
    pub fn include_uuid(mut self, include_uuid: bool) -> Self {
        self.include_uuid = include_uuid;
        self
    }

    /// Set the UUID (optional, will generate if include_uuid is true and not set)
    pub fn uuid(mut self, uuid: Option<impl Into<String>>) -> Self {
        self.uuid = uuid.map(|u| u.into());
        self
    }

    /// Set the creation timestamp (optional, will use current time if not set)
    pub fn created(mut self, created: Option<impl Into<String>>) -> Self {
        self.created = created.map(|c| c.into());
        self
    }

    /// Enable or disable hook execution (enabled by default)
    pub fn run_hooks(mut self, run_hooks: bool) -> Self {
        self.run_hooks = run_hooks;
        self
    }

    /// Build the ticket and write it to disk
    pub fn build(self) -> Result<(String, PathBuf)> {
        utils::ensure_dir()?;

        let id = utils::generate_id_with_custom_prefix(self.prefix.as_deref())?;
        let uuid = if self.include_uuid {
            Some(self.uuid.unwrap_or_else(utils::generate_uuid))
        } else {
            self.uuid
        };
        let now = self.created.unwrap_or_else(utils::iso_date);
        let status = self.status.unwrap_or_else(|| "new".to_string());
        let ticket_type = self.ticket_type.unwrap_or_else(|| "task".to_string());
        let priority = self.priority.unwrap_or_else(|| "2".to_string());

        let mut frontmatter_lines = vec![
            "---".to_string(),
            format!("id: {}", id),
            format!("status: {}", status),
            "deps: []".to_string(),
            "links: []".to_string(),
            format!("created: {}", now),
            format!("type: {}", ticket_type),
            format!("priority: {}", priority),
        ];

        if let Some(ref uuid_val) = uuid {
            frontmatter_lines.insert(2, format!("uuid: {}", uuid_val));
        }

        if let Some(ref ext) = self.external_ref {
            frontmatter_lines.push(format!("external-ref: {}", ext));
        }
        if let Some(ref parent) = self.parent {
            frontmatter_lines.push(format!("parent: {}", parent));
        }
        if let Some(ref remote) = self.remote {
            frontmatter_lines.push(format!("remote: {}", remote));
        }

        frontmatter_lines.push("---".to_string());

        let frontmatter = frontmatter_lines.join("\n");

        let mut sections = vec![format!("# {}", self.title)];

        if let Some(ref desc) = self.description {
            sections.push(format!("\n{}", desc));
        }
        if let Some(ref design) = self.design {
            sections.push(format!("\n## Design\n\n{}", design));
        }
        if let Some(ref acceptance) = self.acceptance {
            sections.push(format!("\n## Acceptance Criteria\n\n{}", acceptance));
        }

        let body = sections.join("\n");
        let content = format!("{}\n{}\n", frontmatter, body);

        let file_path = PathBuf::from(TICKETS_ITEMS_DIR).join(format!("{}.md", id));

        if self.run_hooks {
            let context = HookContext::new()
                .with_item_type(ItemType::Ticket)
                .with_item_id(&id)
                .with_file_path(&file_path);

            run_pre_hooks(HookEvent::PreWrite, &context)?;

            fs::create_dir_all(TICKETS_ITEMS_DIR)?;
            fs::write(&file_path, &content)?;

            run_post_hooks(HookEvent::PostWrite, &context);
            run_post_hooks(HookEvent::TicketCreated, &context);
        } else {
            fs::create_dir_all(TICKETS_ITEMS_DIR)?;
            fs::write(&file_path, &content)?;
        }

        Ok((id, file_path))
    }
}

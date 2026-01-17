use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::cache;
use crate::error::{JanusError, Result};
use crate::hooks::{HookContext, HookEvent, ItemType, run_post_hooks, run_pre_hooks};
use crate::parser::parse_ticket_content;
use crate::types::{
    IMMUTABLE_TICKET_FIELDS, TICKETS_ITEMS_DIR, TicketMetadata, VALID_TICKET_FIELDS,
};
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
    if let Some(cache) = cache::get_or_init_cache().await {
        let exact_match_path = PathBuf::from(TICKETS_ITEMS_DIR).join(format!("{}.md", partial_id));
        if exact_match_path.exists() {
            return Ok(exact_match_path);
        }

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

/// Find a ticket file by partial ID (sync wrapper for backward compatibility)
pub fn find_ticket_by_id_sync(partial_id: &str) -> Result<PathBuf> {
    use tokio::runtime::Handle;

    if Handle::try_current().is_err() {
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| JanusError::Other(format!("Failed to create tokio runtime: {}", e)))?;
        return rt.block_on(find_ticket_by_id(partial_id));
    }

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

/// A lightweight locator that holds only path and ID information
#[derive(Debug, Clone)]
pub struct TicketLocator {
    pub file_path: PathBuf,
    pub id: String,
}

impl TicketLocator {
    pub fn new(file_path: PathBuf) -> Self {
        let id = file_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        TicketLocator { file_path, id }
    }

    pub fn find(partial_id: &str) -> Result<Self> {
        let file_path = find_ticket_by_id_sync(partial_id)?;
        Ok(TicketLocator::new(file_path))
    }

    pub async fn find_async(partial_id: &str) -> Result<Self> {
        let file_path = find_ticket_by_id(partial_id).await?;
        Ok(TicketLocator::new(file_path))
    }
}

/// Handles file I/O operations for tickets
#[derive(Clone)]
pub struct TicketFile {
    locator: TicketLocator,
}

impl TicketFile {
    pub fn new(locator: TicketLocator) -> Self {
        TicketFile { locator }
    }

    pub fn from_path(file_path: PathBuf) -> Self {
        TicketFile {
            locator: TicketLocator::new(file_path),
        }
    }

    pub fn read_raw(&self) -> Result<String> {
        Ok(fs::read_to_string(&self.locator.file_path)?)
    }

    pub fn write_raw(&self, content: &str) -> Result<()> {
        if let Some(parent) = self.locator.file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.locator.file_path, content)?;
        Ok(())
    }

    pub fn file_path(&self) -> &PathBuf {
        &self.locator.file_path
    }

    pub fn id(&self) -> &str {
        &self.locator.id
    }

    pub fn locator(&self) -> &TicketLocator {
        &self.locator
    }
}

/// Handles content parsing and serialization for tickets
pub struct TicketContent;

impl TicketContent {
    pub fn parse(raw_content: &str) -> Result<TicketMetadata> {
        parse_ticket_content(raw_content)
    }

    pub fn update_field(raw_content: &str, field: &str, value: &str) -> Result<String> {
        let field_pattern = Regex::new(&format!(r"(?m)^{}:\s*.*$", regex::escape(field))).unwrap();

        if field_pattern.is_match(raw_content) {
            Ok(field_pattern
                .replace(raw_content, format!("{}: {}", field, value))
                .into_owned())
        } else {
            Ok(raw_content.replacen("---\n", &format!("---\n{}: {}\n", field, value), 1))
        }
    }

    pub fn remove_field(raw_content: &str, field: &str) -> Result<String> {
        let field_pattern =
            Regex::new(&format!(r"(?m)^{}:\s*.*\n?", regex::escape(field))).unwrap();
        Ok(field_pattern.replace(raw_content, "").into_owned())
    }
}

fn validate_field_name(field: &str, operation: &str) -> Result<()> {
    if !VALID_TICKET_FIELDS.contains(&field) {
        return Err(JanusError::InvalidField {
            field: field.to_string(),
            valid_fields: VALID_TICKET_FIELDS.iter().map(|s| s.to_string()).collect(),
        });
    }

    if IMMUTABLE_TICKET_FIELDS.contains(&field) {
        return Err(JanusError::Other(format!(
            "cannot {} immutable field '{}'",
            operation, field
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_field_name_valid() {
        assert!(validate_field_name("status", "update").is_ok());
        assert!(validate_field_name("priority", "update").is_ok());
        assert!(validate_field_name("type", "update").is_ok());
    }

    #[test]
    fn test_validate_field_name_invalid() {
        let result = validate_field_name("unknown_field", "update");
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::InvalidField {
                field,
                valid_fields: _,
            } => {
                assert_eq!(field, "unknown_field");
            }
            _ => panic!("Expected InvalidField error"),
        }
    }

    #[test]
    fn test_validate_field_name_immutable_id() {
        let result = validate_field_name("id", "update");
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::Other(msg) => {
                assert!(msg.contains("cannot update immutable field 'id'"));
            }
            _ => panic!("Expected Other error for immutable field"),
        }
    }

    #[test]
    fn test_validate_field_name_immutable_uuid() {
        let result = validate_field_name("uuid", "update");
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::Other(msg) => {
                assert!(msg.contains("cannot update immutable field 'uuid'"));
            }
            _ => panic!("Expected Other error for immutable field"),
        }
    }

    #[test]
    fn test_validate_field_name_remove_immutable() {
        let result = validate_field_name("id", "remove");
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::Other(msg) => {
                assert!(msg.contains("cannot remove immutable field 'id'"));
            }
            _ => panic!("Expected Other error for immutable field"),
        }
    }
}

/// Handles field manipulation operations for tickets
pub struct TicketEditor {
    file: TicketFile,
}

impl TicketEditor {
    pub fn new(file: TicketFile) -> Self {
        TicketEditor { file }
    }

    fn extract_field_value(content: &str, field: &str) -> Option<String> {
        let field_pattern = Regex::new(&format!(r"(?m)^{}:\s*.*$", regex::escape(field))).unwrap();
        field_pattern.find(content).map(|m| {
            m.as_str()
                .split(':')
                .nth(1)
                .map(|v| v.trim().to_string())
                .unwrap_or_default()
        })
    }

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

    pub fn update_field(&self, field: &str, value: &str) -> Result<()> {
        validate_field_name(field, "update")?;

        let raw_content = self.file.read_raw()?;
        let old_value = Self::extract_field_value(&raw_content, field);

        let mut context = HookContext::new()
            .with_item_type(ItemType::Ticket)
            .with_item_id(self.file.id())
            .with_file_path(self.file.file_path())
            .with_field_name(field)
            .with_new_value(value);

        if let Some(ref old_val) = old_value {
            context = context.with_old_value(old_val);
        }

        self.with_write_hooks(
            context,
            || {
                let new_content = TicketContent::update_field(&raw_content, field, value)?;
                self.file.write_raw(&new_content)
            },
            Some(HookEvent::TicketUpdated),
        )
    }

    pub fn remove_field(&self, field: &str) -> Result<()> {
        validate_field_name(field, "remove")?;

        let raw_content = self.file.read_raw()?;
        let old_value = Self::extract_field_value(&raw_content, field);

        let mut context = HookContext::new()
            .with_item_type(ItemType::Ticket)
            .with_item_id(self.file.id())
            .with_file_path(self.file.file_path())
            .with_field_name(field);

        if let Some(ref old_val) = old_value {
            context = context.with_old_value(old_val);
        }

        self.with_write_hooks(
            context,
            || {
                let new_content = TicketContent::remove_field(&raw_content, field)?;
                self.file.write_raw(&new_content)
            },
            Some(HookEvent::TicketUpdated),
        )
    }

    pub fn add_to_array_field(&self, field: &str, value: &str) -> Result<bool> {
        let raw_content = self.file.read_raw()?;
        let metadata = TicketContent::parse(&raw_content)?;
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
            .with_item_id(self.file.id())
            .with_file_path(self.file.file_path())
            .with_field_name(field)
            .with_new_value(value);

        self.with_write_hooks(
            context,
            || {
                let mut new_array = current_array.clone();
                new_array.push(value.to_string());
                let json_value = serde_json::to_string(&new_array)?;
                self.update_field_internal(field, &json_value)
            },
            Some(HookEvent::TicketUpdated),
        )?;

        Ok(true)
    }

    pub fn remove_from_array_field(&self, field: &str, value: &str) -> Result<bool> {
        let raw_content = self.file.read_raw()?;
        let metadata = TicketContent::parse(&raw_content)?;
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
            .with_item_id(self.file.id())
            .with_file_path(self.file.file_path())
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

    fn update_field_internal(&self, field: &str, value: &str) -> Result<()> {
        let raw_content = self.file.read_raw()?;
        let new_content = TicketContent::update_field(&raw_content, field, value)?;
        self.file.write_raw(&new_content)
    }

    pub fn write(&self, content: &str) -> Result<()> {
        let context = HookContext::new()
            .with_item_type(ItemType::Ticket)
            .with_item_id(self.file.id())
            .with_file_path(self.file.file_path());

        self.with_write_hooks(
            context,
            || self.file.write_raw(content),
            Some(HookEvent::TicketUpdated),
        )
    }
}

/// A simplified Ticket that acts as a facade for common operations
pub struct Ticket {
    pub file_path: PathBuf,
    pub id: String,
    file: TicketFile,
    editor: TicketEditor,
}

impl Ticket {
    pub fn find(partial_id: &str) -> Result<Self> {
        let locator = TicketLocator::find(partial_id)?;
        let file = TicketFile::new(locator.clone());
        let editor = TicketEditor::new(file.clone());
        Ok(Ticket {
            file_path: locator.file_path.clone(),
            id: locator.id.clone(),
            file,
            editor,
        })
    }

    pub async fn find_async(partial_id: &str) -> Result<Self> {
        let locator = TicketLocator::find_async(partial_id).await?;
        let file = TicketFile::new(locator.clone());
        let editor = TicketEditor::new(file.clone());
        Ok(Ticket {
            file_path: locator.file_path.clone(),
            id: locator.id.clone(),
            file,
            editor,
        })
    }

    pub fn new(file_path: PathBuf) -> Self {
        let locator = TicketLocator::new(file_path.clone());
        let file = TicketFile::new(locator.clone());
        let editor = TicketEditor::new(file.clone());
        Ticket {
            file_path: locator.file_path.clone(),
            id: locator.id.clone(),
            file,
            editor,
        }
    }

    pub fn read(&self) -> Result<TicketMetadata> {
        let raw_content = self.file.read_raw()?;
        let mut metadata = TicketContent::parse(&raw_content)?;
        metadata.file_path = Some(self.file.file_path().clone());
        Ok(metadata)
    }

    pub fn read_content(&self) -> Result<String> {
        self.file.read_raw()
    }

    pub fn write(&self, content: &str) -> Result<()> {
        self.editor.write(content)
    }

    pub fn update_field(&self, field: &str, value: &str) -> Result<()> {
        self.editor.update_field(field, value)
    }

    pub fn remove_field(&self, field: &str) -> Result<()> {
        self.editor.remove_field(field)
    }

    pub fn add_to_array_field(&self, field: &str, value: &str) -> Result<bool> {
        self.editor.add_to_array_field(field, value)
    }

    pub fn remove_from_array_field(&self, field: &str, value: &str) -> Result<bool> {
        self.editor.remove_from_array_field(field, value)
    }
}

/// Repository that orchestrates all ticket operations
pub struct TicketRepository;

impl TicketRepository {
    pub fn find(partial_id: &str) -> Result<Ticket> {
        Ticket::find(partial_id)
    }

    pub async fn find_async(partial_id: &str) -> Result<Ticket> {
        Ticket::find_async(partial_id).await
    }

    pub fn from_path(file_path: PathBuf) -> Ticket {
        Ticket::new(file_path)
    }

    pub fn get_all() -> Vec<TicketMetadata> {
        use tokio::runtime::Handle;

        if Handle::try_current().is_err() {
            let rt = tokio::runtime::Runtime::new().ok();
            if let Some(rt) = rt {
                return rt.block_on(async { Self::get_all_async().await });
            }
        }

        Self::get_all_from_disk()
    }

    pub async fn get_all_async() -> Vec<TicketMetadata> {
        if let Some(cache) = cache::get_or_init_cache().await {
            if let Ok(tickets) = cache.get_all_tickets().await {
                return tickets;
            }
            eprintln!("Warning: cache read failed, falling back to file reads");
        }

        Self::get_all_from_disk()
    }

    fn get_all_from_disk() -> Vec<TicketMetadata> {
        let files = find_tickets();
        let mut tickets = Vec::new();

        for file in files {
            let file_path = PathBuf::from(TICKETS_ITEMS_DIR).join(&file);
            match fs::read_to_string(&file_path) {
                Ok(content) => match TicketContent::parse(&content) {
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

    pub fn build_map() -> HashMap<String, TicketMetadata> {
        use tokio::runtime::Handle;

        if Handle::try_current().is_err() {
            let rt = tokio::runtime::Runtime::new().ok();
            if let Some(rt) = rt {
                return rt.block_on(async { Self::build_map_async().await });
            }
        }

        Self::get_all_from_disk()
            .into_iter()
            .filter_map(|t| t.id.clone().map(|id| (id, t)))
            .collect()
    }

    pub async fn build_map_async() -> HashMap<String, TicketMetadata> {
        if let Some(cache) = cache::get_or_init_cache().await {
            if let Ok(map) = cache.build_ticket_map().await {
                return map;
            }
            eprintln!("Warning: cache read failed, falling back to file reads");
        }

        Self::get_all_async()
            .await
            .into_iter()
            .filter_map(|t| t.id.clone().map(|id| (id, t)))
            .collect()
    }
}

pub async fn get_all_tickets() -> Vec<TicketMetadata> {
    TicketRepository::get_all_async().await
}

pub fn get_all_tickets_sync() -> Vec<TicketMetadata> {
    TicketRepository::get_all()
}

pub fn get_all_tickets_from_disk() -> Vec<TicketMetadata> {
    TicketRepository::get_all_from_disk()
}

pub async fn build_ticket_map() -> HashMap<String, TicketMetadata> {
    TicketRepository::build_map_async().await
}

pub fn build_ticket_map_sync() -> HashMap<String, TicketMetadata> {
    TicketRepository::build_map()
}

pub fn get_file_mtime(path: &Path) -> Option<std::time::SystemTime> {
    fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

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

    pub fn description(mut self, desc: Option<impl Into<String>>) -> Self {
        self.description = desc.map(|d| d.into());
        self
    }

    pub fn design(mut self, design: Option<impl Into<String>>) -> Self {
        self.design = design.map(|d| d.into());
        self
    }

    pub fn acceptance(mut self, acceptance: Option<impl Into<String>>) -> Self {
        self.acceptance = acceptance.map(|a| a.into());
        self
    }

    pub fn prefix(mut self, prefix: Option<impl Into<String>>) -> Self {
        self.prefix = prefix.map(|p| p.into());
        self
    }

    pub fn ticket_type(mut self, ticket_type: impl Into<String>) -> Self {
        self.ticket_type = Some(ticket_type.into());
        self
    }

    pub fn status(mut self, status: impl Into<String>) -> Self {
        self.status = Some(status.into());
        self
    }

    pub fn priority(mut self, priority: impl Into<String>) -> Self {
        self.priority = Some(priority.into());
        self
    }

    pub fn external_ref(mut self, external_ref: Option<impl Into<String>>) -> Self {
        self.external_ref = external_ref.map(|r| r.into());
        self
    }

    pub fn parent(mut self, parent: Option<impl Into<String>>) -> Self {
        self.parent = parent.map(|p| p.into());
        self
    }

    pub fn remote(mut self, remote: Option<impl Into<String>>) -> Self {
        self.remote = remote.map(|r| r.into());
        self
    }

    pub fn include_uuid(mut self, include_uuid: bool) -> Self {
        self.include_uuid = include_uuid;
        self
    }

    pub fn uuid(mut self, uuid: Option<impl Into<String>>) -> Self {
        self.uuid = uuid.map(|u| u.into());
        self
    }

    pub fn created(mut self, created: Option<impl Into<String>>) -> Self {
        self.created = created.map(|c| c.into());
        self
    }

    pub fn run_hooks(mut self, run_hooks: bool) -> Self {
        self.run_hooks = run_hooks;
        self
    }

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

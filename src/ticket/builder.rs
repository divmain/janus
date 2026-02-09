use crate::error::{JanusError, Result};
use crate::hooks::{HookContext, HookEvent, run_post_hooks, run_pre_hooks};
use crate::types::{
    EntityType, TicketPriority, TicketSize, TicketStatus, TicketType, tickets_items_dir,
};
use crate::utils;
use serde::Serialize;
use std::path::PathBuf;
use std::str::FromStr;

/// Wrapper for priority that serializes as an integer
#[derive(Debug, Clone, Copy)]
struct PriorityAsU8(u8);

impl Serialize for PriorityAsU8 {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u8(self.0)
    }
}

impl From<TicketPriority> for PriorityAsU8 {
    fn from(p: TicketPriority) -> Self {
        PriorityAsU8(p.as_num())
    }
}

/// Temporary struct for serializing ticket frontmatter to YAML
#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
struct TicketFrontmatter {
    id: String,
    uuid: String,
    status: String,
    deps: Vec<String>,
    links: Vec<String>,
    created: String,
    r#type: String,
    priority: PriorityAsU8,
    #[serde(skip_serializing_if = "Option::is_none")]
    external_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    remote: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    spawned_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    spawn_context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    depth: Option<u32>,
    triaged: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<String>,
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
    uuid: Option<String>,
    created: Option<String>,
    run_hooks: bool,
    spawned_from: Option<String>,
    spawn_context: Option<String>,
    depth: Option<u32>,
    triaged: Option<bool>,
    size: Option<TicketSize>,
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
            uuid: None,
            created: None,
            run_hooks: true,
            spawned_from: None,
            spawn_context: None,
            depth: None,
            triaged: None,
            size: None,
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

    pub fn spawned_from(mut self, spawned_from: Option<impl Into<String>>) -> Self {
        self.spawned_from = spawned_from.map(|s| s.into());
        self
    }

    pub fn spawn_context(mut self, spawn_context: Option<impl Into<String>>) -> Self {
        self.spawn_context = spawn_context.map(|s| s.into());
        self
    }

    pub fn depth(mut self, depth: Option<u32>) -> Self {
        self.depth = depth;
        self
    }

    pub fn triaged(mut self, triaged: bool) -> Self {
        self.triaged = Some(triaged);
        self
    }

    pub fn size(mut self, size: Option<TicketSize>) -> Self {
        self.size = size;
        self
    }

    pub fn build(self) -> Result<(String, PathBuf)> {
        utils::ensure_dir()?;

        let id = utils::generate_id_with_custom_prefix(self.prefix.as_deref())?;

        // Validate that the generated ID is safe to use as a filename
        utils::validate_filename(&id)?;

        let uuid = self.uuid.unwrap_or_else(utils::generate_uuid);
        let now = self.created.unwrap_or_else(utils::iso_date);
        let status = self.status.unwrap_or_else(|| "new".to_string());
        let ticket_type = self.ticket_type.unwrap_or_else(|| "task".to_string());
        let priority = self.priority.unwrap_or_else(|| "2".to_string());

        TicketStatus::from_str(&status).map_err(|_| JanusError::InvalidStatus(status.clone()))?;
        TicketType::from_str(&ticket_type)
            .map_err(|_| JanusError::InvalidTicketType(ticket_type.clone()))?;
        let priority_enum = TicketPriority::from_str(&priority)
            .map_err(|_| JanusError::InvalidPriority(priority.clone()))?;

        let frontmatter_data = TicketFrontmatter {
            id: id.clone(),
            uuid,
            status,
            deps: vec![],
            links: vec![],
            created: now,
            r#type: ticket_type,
            priority: priority_enum.into(),
            external_ref: self.external_ref,
            parent: self.parent,
            remote: self.remote,
            spawned_from: self.spawned_from,
            spawn_context: self.spawn_context,
            depth: self.depth,
            triaged: self.triaged.unwrap_or(false),
            size: self.size.map(|s| s.to_string()),
        };

        let yaml_content = serde_yaml_ng::to_string(&frontmatter_data).map_err(|e| {
            JanusError::InternalError(format!("Failed to serialize frontmatter: {e}"))
        })?;
        let frontmatter = format!("---\n{yaml_content}---");

        let mut sections = vec![format!("# {}", self.title)];

        if let Some(ref desc) = self.description {
            sections.push(format!("\n{desc}"));
        }
        if let Some(ref design) = self.design {
            sections.push(format!("\n## Design\n\n{design}"));
        }
        if let Some(ref acceptance) = self.acceptance {
            sections.push(format!("\n## Acceptance Criteria\n\n{acceptance}"));
        }

        let body = sections.join("\n");
        let content = format!("{frontmatter}\n{body}\n");

        let items_dir = tickets_items_dir();
        let file_path = items_dir.join(format!("{id}.md"));

        // Ensure parent directory exists
        crate::fs::ensure_parent_dir(&file_path)?;

        if self.run_hooks {
            let context = HookContext::new()
                .with_item_type(EntityType::Ticket)
                .with_item_id(&id)
                .with_file_path(&file_path);

            run_pre_hooks(HookEvent::PreWrite, &context)?;

            std::fs::write(&file_path, &content).map_err(|e| {
                JanusError::Io(std::io::Error::new(
                    e.kind(),
                    format!(
                        "Failed to write ticket at {}: {}",
                        crate::utils::format_relative_path(&file_path),
                        e
                    ),
                ))
            })?;

            run_post_hooks(HookEvent::PostWrite, &context);
            run_post_hooks(HookEvent::TicketCreated, &context);
        } else {
            std::fs::write(&file_path, &content).map_err(|e| {
                JanusError::Io(std::io::Error::new(
                    e.kind(),
                    format!(
                        "Failed to write ticket at {}: {}",
                        crate::utils::format_relative_path(&file_path),
                        e
                    ),
                ))
            })?;
        }

        Ok((id, file_path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::fs;

    #[test]
    #[serial]
    fn test_builder_rejects_invalid_status() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_builder_rejects_invalid_status");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let result = TicketBuilder::new("Test")
            .status("invalid_status")
            .run_hooks(false)
            .build();

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, JanusError::InvalidStatus(_)));
    }

    #[test]
    #[serial]
    fn test_builder_rejects_invalid_ticket_type() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_builder_rejects_invalid_ticket_type");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let result = TicketBuilder::new("Test")
            .ticket_type("invalid_type")
            .run_hooks(false)
            .build();

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("invalid ticket type"));
    }

    #[test]
    #[serial]
    fn test_builder_rejects_invalid_priority() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_builder_rejects_invalid_priority");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let result = TicketBuilder::new("Test")
            .priority("999")
            .run_hooks(false)
            .build();

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("invalid priority"));
    }

    #[test]
    #[serial]
    fn test_builder_accepts_valid_status() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_builder_accepts_valid_status");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let result = TicketBuilder::new("Test")
            .status("complete")
            .run_hooks(false)
            .build();

        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    fn test_builder_accepts_valid_ticket_type() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_builder_accepts_valid_ticket_type");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let result = TicketBuilder::new("Test")
            .ticket_type("bug")
            .run_hooks(false)
            .build();

        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    fn test_builder_accepts_valid_priority() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_builder_accepts_valid_priority");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let result = TicketBuilder::new("Test")
            .priority("0")
            .run_hooks(false)
            .build();

        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    fn test_builder_with_spawned_from() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_builder_with_spawned_from");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let result = TicketBuilder::new("Test Spawned Ticket")
            .spawned_from(Some("j-parent"))
            .spawn_context(Some("Test context"))
            .depth(Some(1))
            .run_hooks(false)
            .build();

        assert!(result.is_ok());
        let (id, path) = result.unwrap();
        let content = fs::read_to_string(&path).unwrap();

        assert!(content.contains(&format!("id: {id}")));
        assert!(content.contains("spawned-from: j-parent"));
        assert!(content.contains("spawn-context: Test context"));
        assert!(content.contains("depth: 1"));
    }

    #[test]
    #[serial]
    fn test_builder_without_spawning_fields() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_builder_without_spawning_fields");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let result = TicketBuilder::new("Test Regular Ticket")
            .run_hooks(false)
            .build();

        assert!(result.is_ok());
        let (_id, path) = result.unwrap();
        let content = fs::read_to_string(&path).unwrap();

        // Spawning fields should not be present when not set
        assert!(!content.contains("spawned-from"));
        assert!(!content.contains("spawn-context"));
        assert!(!content.contains("depth"));
    }

    #[test]
    #[serial]
    fn test_builder_spawned_from_with_depth_zero() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp
            .path()
            .join("test_builder_spawned_from_with_depth_zero");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        // Create a ticket spawned from a root ticket (depth 0)
        let result = TicketBuilder::new("Test Spawned From Root")
            .spawned_from(Some("j-root"))
            .depth(Some(1))
            .run_hooks(false)
            .build();

        assert!(result.is_ok());
        let (_id, path) = result.unwrap();
        let content = fs::read_to_string(&path).unwrap();

        assert!(content.contains("spawned-from: j-root"));
        assert!(content.contains("depth: 1"));
    }
}

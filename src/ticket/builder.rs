use crate::error::Result;
use crate::hooks::{HookContext, HookEvent, ItemType, run_post_hooks, run_pre_hooks};
use crate::types::TICKETS_ITEMS_DIR;
use crate::utils;
use std::fs;
use std::path::PathBuf;

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

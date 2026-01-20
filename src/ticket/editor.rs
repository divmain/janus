use crate::error::{JanusError, Result};
use crate::hooks::{run_post_hooks, run_pre_hooks, HookContext, HookEvent};
use crate::ticket::content::{
    extract_field_value, parse, remove_field as remove_field_from_content,
    update_field as update_field_in_content, validate_field_name,
};
use crate::ticket::file::TicketFile;
use serde_json;

pub struct TicketEditor {
    file: TicketFile,
}

impl TicketEditor {
    pub fn new(file: TicketFile) -> Self {
        TicketEditor { file }
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
        let old_value = extract_field_value(&raw_content, field);

        let mut context = self
            .file
            .hook_context()
            .with_field_name(field)
            .with_new_value(value);

        if let Some(ref old_val) = old_value {
            context = context.with_old_value(old_val);
        }

        self.with_write_hooks(
            context,
            || {
                let new_content = update_field_in_content(&raw_content, field, value)?;
                self.file.write_raw(&new_content)
            },
            Some(HookEvent::TicketUpdated),
        )
    }

    pub fn remove_field(&self, field: &str) -> Result<()> {
        validate_field_name(field, "remove")?;

        let raw_content = self.file.read_raw()?;
        let old_value = extract_field_value(&raw_content, field);

        let mut context = self.file.hook_context().with_field_name(field);

        if let Some(ref old_val) = old_value {
            context = context.with_old_value(old_val);
        }

        self.with_write_hooks(
            context,
            || {
                let new_content = remove_field_from_content(&raw_content, field)?;
                self.file.write_raw(&new_content)
            },
            Some(HookEvent::TicketUpdated),
        )
    }

    fn get_array_field<'a>(
        metadata: &'a crate::types::TicketMetadata,
        field: &str,
    ) -> Result<&'a Vec<String>> {
        match field {
            "deps" => Ok(&metadata.deps),
            "links" => Ok(&metadata.links),
            _ => Err(JanusError::UnknownArrayField(field.to_string())),
        }
    }

    pub fn add_to_array_field(&self, field: &str, value: &str) -> Result<bool> {
        let raw_content = self.file.read_raw()?;
        let metadata = parse(&raw_content)?;
        let current_array = Self::get_array_field(&metadata, field)?;

        if current_array.contains(&value.to_string()) {
            return Ok(false);
        }

        let mut new_array = current_array.clone();
        new_array.push(value.to_string());
        let json_value = serde_json::to_string(&new_array)?;

        let context = self
            .file
            .hook_context()
            .with_field_name(field)
            .with_new_value(value);

        self.with_write_hooks(
            context,
            || {
                let new_content = update_field_in_content(&raw_content, field, &json_value)?;
                self.file.write_raw(&new_content)
            },
            Some(HookEvent::TicketUpdated),
        )?;

        Ok(true)
    }

    pub fn remove_from_array_field(&self, field: &str, value: &str) -> Result<bool> {
        let raw_content = self.file.read_raw()?;
        let metadata = parse(&raw_content)?;
        let current_array = Self::get_array_field(&metadata, field)?;

        if !current_array.contains(&value.to_string()) {
            return Ok(false);
        }

        let new_array: Vec<_> = current_array
            .iter()
            .filter(|v: &&String| v.as_str() != value)
            .collect();
        let json_value = if new_array.is_empty() {
            "[]".to_string()
        } else {
            serde_json::to_string(&new_array)?
        };

        let context = self
            .file
            .hook_context()
            .with_field_name(field)
            .with_old_value(value);

        self.with_write_hooks(
            context,
            || {
                let new_content = update_field_in_content(&raw_content, field, &json_value)?;
                self.file.write_raw(&new_content)
            },
            Some(HookEvent::TicketUpdated),
        )?;

        Ok(true)
    }

    pub fn write_validated(&self, content: &str) -> Result<()> {
        parse(content)?;
        self.write(content)
    }

    pub fn write(&self, content: &str) -> Result<()> {
        let context = self.file.hook_context();

        self.with_write_hooks(
            context,
            || self.file.write_raw(content),
            Some(HookEvent::TicketUpdated),
        )
    }
}

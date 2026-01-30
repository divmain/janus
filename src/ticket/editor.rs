use crate::error::{JanusError, Result};
use crate::ticket::Ticket;
use crate::types::{TicketMetadata, TicketSize};

/// Editor for modifying ticket metadata.
///
/// Provides a fluent API for updating ticket fields with proper validation
/// and type safety. Changes are applied when `save()` is called.
///
/// # Example
///
/// ```ignore
/// let ticket = Ticket::find("abc").await?;
/// let mut editor = TicketEditor::new(ticket)?;
/// editor.set_size(TicketSize::Medium).save()?;
/// ```
pub struct TicketEditor {
    ticket: Ticket,
    pub metadata: TicketMetadata,
}

impl TicketEditor {
    /// Create a new TicketEditor for the given ticket.
    ///
    /// Reads the current metadata from the ticket file.
    pub fn new(ticket: Ticket) -> Result<Self> {
        let metadata = ticket.read()?;
        Ok(TicketEditor { ticket, metadata })
    }

    /// Update a field by name with a string value.
    ///
    /// # Arguments
    ///
    /// * `field` - The field name to update (e.g., "size", "status", "priority")
    /// * `value` - The string value to set
    ///
    /// # Errors
    ///
    /// Returns `JanusError::InvalidFieldValue` if the value cannot be parsed
    /// for the specified field.
    pub fn update_field(&mut self, field: &str, value: &str) -> Result<()> {
        match field {
            "size" => {
                let size =
                    value
                        .parse::<TicketSize>()
                        .map_err(|_| JanusError::InvalidFieldValue {
                            field: "size".into(),
                            value: value.into(),
                            valid_values: crate::types::VALID_SIZES
                                .iter()
                                .map(|s| s.to_string())
                                .collect(),
                        })?;
                self.metadata.size = Some(size);
            }
            _ => {
                return Err(JanusError::InvalidFieldName(field.to_string()));
            }
        }
        Ok(())
    }

    /// Set the size field to a specific value.
    pub fn set_size(&mut self, size: TicketSize) -> &mut Self {
        self.metadata.size = Some(size);
        self
    }

    /// Clear the size field (set to None).
    pub fn clear_size(&mut self) -> &mut Self {
        self.metadata.size = None;
        self
    }

    /// Save the modified metadata back to the ticket file.
    ///
    /// This writes the changes to disk using the ticket's update_field method
    /// for each modified field.
    pub fn save(&self) -> Result<()> {
        // For now, we only support updating the size field
        // Other fields would need similar handling
        if let Some(size) = self.metadata.size {
            self.ticket.update_field("size", &size.to_string())?;
        } else {
            // If size is None, remove the field from the ticket
            self.ticket.remove_field("size")?;
        }
        Ok(())
    }

    /// Get a reference to the ticket being edited.
    pub fn ticket(&self) -> &Ticket {
        &self.ticket
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ticket::builder::TicketBuilder;
    use serial_test::serial;
    use std::fs;

    #[test]
    #[serial]
    fn test_ticket_editor_set_size() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_editor_set_size");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        // Create a ticket first
        let (_id, path) = TicketBuilder::new("Test Ticket")
            .run_hooks(false)
            .build()
            .unwrap();

        let ticket = Ticket::new(path.clone()).unwrap();
        let mut editor = TicketEditor::new(ticket).unwrap();

        // Set size using the convenience method
        editor.set_size(TicketSize::Medium);
        assert_eq!(editor.metadata.size, Some(TicketSize::Medium));

        // Save and verify
        editor.save().unwrap();

        // Read back and verify
        let ticket = Ticket::new(path).unwrap();
        let metadata = ticket.read().unwrap();
        assert_eq!(metadata.size, Some(TicketSize::Medium));
    }

    #[test]
    #[serial]
    fn test_ticket_editor_clear_size() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_editor_clear_size");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        // Create a ticket with size first
        let (_id, path) = TicketBuilder::new("Test Ticket")
            .run_hooks(false)
            .build()
            .unwrap();

        // First set a size
        let ticket = Ticket::new(path.clone()).unwrap();
        let mut editor = TicketEditor::new(ticket).unwrap();
        editor.set_size(TicketSize::Large);
        editor.save().unwrap();

        // Now clear it
        let ticket = Ticket::new(path.clone()).unwrap();
        let mut editor = TicketEditor::new(ticket).unwrap();
        editor.clear_size();
        assert_eq!(editor.metadata.size, None);

        // Save and verify
        editor.save().unwrap();

        // Read back and verify
        let ticket = Ticket::new(path).unwrap();
        let metadata = ticket.read().unwrap();
        assert_eq!(metadata.size, None);
    }

    #[test]
    #[serial]
    fn test_ticket_editor_update_field_size() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_editor_update_field_size");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        // Create a ticket first
        let (_id, path) = TicketBuilder::new("Test Ticket")
            .run_hooks(false)
            .build()
            .unwrap();

        let ticket = Ticket::new(path.clone()).unwrap();
        let mut editor = TicketEditor::new(ticket).unwrap();

        // Set size using update_field
        editor.update_field("size", "small").unwrap();
        assert_eq!(editor.metadata.size, Some(TicketSize::Small));

        // Save and verify
        editor.save().unwrap();

        // Read back and verify
        let ticket = Ticket::new(path).unwrap();
        let metadata = ticket.read().unwrap();
        assert_eq!(metadata.size, Some(TicketSize::Small));
    }

    #[test]
    #[serial]
    fn test_ticket_editor_update_field_size_invalid() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_editor_update_field_size_invalid");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        // Create a ticket first
        let (_id, path) = TicketBuilder::new("Test Ticket")
            .run_hooks(false)
            .build()
            .unwrap();

        let ticket = Ticket::new(path).unwrap();
        let mut editor = TicketEditor::new(ticket).unwrap();

        // Try to set an invalid size
        let result = editor.update_field("size", "invalid_size");
        assert!(result.is_err());

        match result.unwrap_err() {
            JanusError::InvalidFieldValue { field, value, .. } => {
                assert_eq!(field, "size");
                assert_eq!(value, "invalid_size");
            }
            _ => panic!("Expected InvalidFieldValue error"),
        }
    }

    #[test]
    #[serial]
    fn test_ticket_roundtrip_with_size() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_roundtrip_with_size");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        // Create a ticket
        let (_id, path) = TicketBuilder::new("Test Ticket")
            .run_hooks(false)
            .build()
            .unwrap();

        // Set size via editor
        let ticket = Ticket::new(path.clone()).unwrap();
        let mut editor = TicketEditor::new(ticket).unwrap();
        editor.set_size(TicketSize::XLarge);
        editor.save().unwrap();

        // Read back the raw content to verify it's in the file
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("size: xlarge"));

        // Read via TicketEditor and verify
        let ticket = Ticket::new(path.clone()).unwrap();
        let editor = TicketEditor::new(ticket).unwrap();
        assert_eq!(editor.metadata.size, Some(TicketSize::XLarge));

        // Read via Ticket::read and verify
        let ticket = Ticket::new(path).unwrap();
        let metadata = ticket.read().unwrap();
        assert_eq!(metadata.size, Some(TicketSize::XLarge));
    }
}

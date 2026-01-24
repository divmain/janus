//! Plan field manipulation with hook orchestration
//!
//! Provides the `PlanEditor` struct for modifying plan content while
//! properly orchestrating pre/post hooks and plan-specific events.

use crate::error::Result;
use crate::hooks::{HookEvent, run_post_hooks, run_pre_hooks};
use crate::plan::file::PlanFile;
use crate::plan::parser::parse_plan_content;
use crate::storage::FileStorage;

/// Handles plan editing operations with proper hook orchestration.
///
/// `PlanEditor` provides methods for modifying plan content while ensuring
/// that appropriate hooks are triggered before and after writes.
///
/// Note: Plans have a simpler editing model than tickets. Most plan modifications
/// involve rewriting the entire content rather than updating individual fields
/// in the frontmatter. This editor primarily handles the write-with-hooks pattern.
pub struct PlanEditor {
    file: PlanFile,
}

impl PlanEditor {
    /// Create a new editor for the given plan file.
    pub fn new(file: PlanFile) -> Self {
        PlanEditor { file }
    }

    /// Execute an operation with standard write hooks.
    ///
    /// Runs PreWrite hook before the operation and PostWrite + PlanUpdated
    /// hooks after successful completion.
    fn with_write_hooks<F>(&self, operation: F) -> Result<()>
    where
        F: FnOnce() -> Result<()>,
    {
        let context = self.file.hook_context();

        run_pre_hooks(HookEvent::PreWrite, &context)?;

        operation()?;

        run_post_hooks(HookEvent::PostWrite, &context);
        run_post_hooks(HookEvent::PlanUpdated, &context);

        Ok(())
    }

    /// Write content to the plan file with validation and hooks.
    ///
    /// This method:
    /// 1. Validates the content can be parsed as a plan
    /// 2. Runs PreWrite hook (can abort)
    /// 3. Writes the content
    /// 4. Runs PostWrite and PlanUpdated hooks
    ///
    /// # Arguments
    /// * `content` - The full plan content to write (frontmatter + markdown)
    pub fn write_validated(&self, content: &str) -> Result<()> {
        // Validate content is parseable
        parse_plan_content(content)?;
        self.write(content)
    }

    /// Write content to the plan file with hooks (no validation).
    ///
    /// Use this when you've already validated the content or when writing
    /// content that might not be fully valid yet.
    ///
    /// # Arguments
    /// * `content` - The full plan content to write
    pub fn write(&self, content: &str) -> Result<()> {
        self.with_write_hooks(|| self.file.write_raw(content))
    }

    /// Write content without triggering hooks.
    ///
    /// Used internally when hooks should be handled at a higher level
    /// (e.g., plan creation where PlanCreated should be fired instead of PlanUpdated).
    pub(crate) fn write_without_hooks(&self, content: &str) -> Result<()> {
        self.file.write_raw(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::locator::PlanLocator;

    fn make_editor(id: &str) -> PlanEditor {
        let locator = PlanLocator::with_id(id);
        let file = PlanFile::new(locator);
        PlanEditor::new(file)
    }

    #[test]
    fn test_plan_editor_new() {
        let editor = make_editor("plan-test");
        assert_eq!(editor.file.id(), "plan-test");
    }

    // Note: Full integration tests for write operations require
    // a test environment with .janus directory. Those tests are
    // in the integration test suite.
}

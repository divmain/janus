//! Builder pattern for creating objectives.
//!
//! Provides a fluent API for constructing new objective files with all
//! required and optional fields.

use crate::error::Result;
use crate::objective::serialize::serialize_objective;
use crate::objective::types::ObjectiveMetadata;
use crate::objective::{ensure_objectives_dir, generate_objective_id};
use crate::types::{CreatedAt, ObjectiveId};
use crate::utils;

/// Builder for creating new objectives.
pub struct ObjectiveBuilder {
    title: String,
    description: Option<String>,
    acceptance_criteria: Vec<String>,
    satisfied_by: Vec<String>,
}

impl ObjectiveBuilder {
    /// Create a new builder with the given title.
    pub fn new(title: &str) -> Self {
        ObjectiveBuilder {
            title: title.to_string(),
            description: None,
            acceptance_criteria: Vec::new(),
            satisfied_by: Vec::new(),
        }
    }

    /// Set the description.
    pub fn description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }

    /// Set the acceptance criteria.
    pub fn acceptance_criteria(mut self, criteria: Vec<String>) -> Self {
        self.acceptance_criteria = criteria;
        self
    }

    /// Add a single ticket or plan reference to the satisfied-by list.
    pub fn add_satisfied_by(mut self, ref_id: &str) -> Self {
        self.satisfied_by.push(ref_id.to_string());
        self
    }

    /// Set all satisfied-by references at once, replacing any previously added.
    pub fn satisfied_by_refs(mut self, refs: Vec<String>) -> Self {
        self.satisfied_by = refs;
        self
    }

    /// Build the objective, returning (id, file_content).
    ///
    /// This method:
    /// 1. Ensures the objectives directory exists
    /// 2. Generates a unique objective ID
    /// 3. Generates a UUID
    /// 4. Creates the markdown content with frontmatter + sections
    /// 5. Returns the ID and content string
    pub fn build(self) -> Result<(String, String)> {
        ensure_objectives_dir()?;

        let id = generate_objective_id()?;
        let uuid = utils::generate_uuid();
        let now = utils::iso_date();

        let metadata = ObjectiveMetadata {
            id: Some(ObjectiveId::new_unchecked(&id)),
            uuid: Some(uuid),
            created: Some(CreatedAt::new_unchecked(now)),
            satisfied_by: self.satisfied_by,
            title: Some(self.title),
            description: self.description,
            description_raw: None,
            acceptance_criteria: self.acceptance_criteria,
            acceptance_criteria_raw: None,
            notes_raw: None,
            file_path: None,
            body: None,
            extra_frontmatter: None,
        };

        let content = serialize_objective(&metadata)?;

        Ok((id, content))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objective::parser::parse_objective_content;
    use crate::paths::JanusRootGuard;

    #[test]
    fn test_builder_basic() {
        let temp = tempfile::TempDir::new().unwrap();
        let _guard = JanusRootGuard::new(temp.path().join(".janus"));

        let (id, content) = ObjectiveBuilder::new("Test Objective").build().unwrap();

        assert!(id.starts_with("objv-"));
        assert!(content.contains("# Test Objective"));
        assert!(content.contains(&format!("id: {id}")));

        // Verify it can be parsed back
        let metadata = parse_objective_content(&content).unwrap();
        assert_eq!(metadata.title, Some("Test Objective".to_string()));
    }

    #[test]
    fn test_builder_with_all_fields() {
        let temp = tempfile::TempDir::new().unwrap();
        let _guard = JanusRootGuard::new(temp.path().join(".janus"));

        let (id, content) = ObjectiveBuilder::new("Full Objective")
            .description("A detailed description.")
            .acceptance_criteria(vec!["Criterion A".to_string(), "Criterion B".to_string()])
            .add_satisfied_by("plan-x1y2")
            .build()
            .unwrap();

        assert!(id.starts_with("objv-"));
        assert!(content.contains("# Full Objective"));
        assert!(content.contains("A detailed description."));
        assert!(content.contains("- Criterion A"));
        assert!(content.contains("- Criterion B"));
        assert!(content.contains("- plan-x1y2"));

        // Verify round-trip
        let metadata = parse_objective_content(&content).unwrap();
        assert_eq!(metadata.title, Some("Full Objective".to_string()));
        assert_eq!(
            metadata.description,
            Some("A detailed description.".to_string())
        );
        assert_eq!(metadata.acceptance_criteria.len(), 2);
        assert_eq!(metadata.satisfied_by, vec!["plan-x1y2".to_string()]);
    }

    #[test]
    fn test_builder_multiple_refs() {
        let temp = tempfile::TempDir::new().unwrap();
        let _guard = JanusRootGuard::new(temp.path().join(".janus"));

        let (id, content) = ObjectiveBuilder::new("Multi Ref Objective")
            .add_satisfied_by("j-abc1")
            .add_satisfied_by("plan-xyz2")
            .build()
            .unwrap();

        assert!(id.starts_with("objv-"));
        assert!(content.contains("- j-abc1"));
        assert!(content.contains("- plan-xyz2"));

        let metadata = parse_objective_content(&content).unwrap();
        assert_eq!(metadata.satisfied_by, vec!["j-abc1".to_string(), "plan-xyz2".to_string()]);
    }
}

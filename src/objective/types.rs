use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::types::{CreatedAt, ObjectiveId};

/// Metadata parsed from an objective file's YAML frontmatter and markdown body.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ObjectiveMetadata {
    /// Objective ID (e.g., "objv-a1b2")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectiveId>,

    /// Durable UUID v4 for disambiguation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,

    /// Creation timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<CreatedAt>,

    /// References to tickets or plans that satisfy this objective
    #[serde(rename = "satisfied-by", default, skip_serializing_if = "Vec::is_empty")]
    pub satisfied_by: Vec<String>,

    // Runtime-only fields (not persisted to YAML frontmatter)
    /// Title extracted from H1 heading
    #[serde(skip)]
    pub title: Option<String>,

    /// Description section content
    #[serde(skip)]
    pub description: Option<String>,

    /// Raw description content for round-trip fidelity
    #[serde(skip)]
    pub description_raw: Option<String>,

    /// Acceptance criteria extracted from bullet list
    #[serde(skip)]
    pub acceptance_criteria: Vec<String>,

    /// Raw acceptance criteria content for round-trip fidelity
    #[serde(skip)]
    pub acceptance_criteria_raw: Option<String>,

    /// Raw notes section content for round-trip fidelity
    #[serde(skip)]
    pub notes_raw: Option<String>,

    /// Path to the objective file on disk
    #[serde(skip)]
    pub file_path: Option<PathBuf>,

    /// Full body content (only populated during store initialization)
    #[serde(skip)]
    pub body: Option<String>,

    /// Unknown/extra YAML frontmatter keys preserved for round-trip fidelity
    #[serde(skip)]
    pub extra_frontmatter: Option<HashMap<String, serde_yaml_ng::Value>>,
}

impl ObjectiveMetadata {
    /// Get the objective ID as a string slice
    pub fn id_str(&self) -> Option<&str> {
        self.id.as_ref().map(|id| id.as_ref())
    }

    /// Get the objective title
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Get the file path
    pub fn file_path(&self) -> Option<&PathBuf> {
        self.file_path.as_ref()
    }

    /// Get the item type
    pub fn item_type(&self) -> crate::types::EntityType {
        crate::types::EntityType::Objective
    }

    /// Parse the `created` field as a jiff::Timestamp.
    pub fn created_timestamp(&self) -> Option<jiff::Timestamp> {
        self.created.as_ref().and_then(|c| c.to_timestamp())
    }
}

/// Result of loading objectives from disk, including both successes and failures.
pub type ObjectiveLoadResult = crate::types::LoadResult<ObjectiveMetadata>;

impl ObjectiveLoadResult {
    /// Add a successfully loaded objective
    pub fn add_objective(&mut self, objective: ObjectiveMetadata) {
        self.items.push(objective);
    }

    /// Convert to a Result, returning Err if there are failures
    pub fn into_result(self) -> crate::error::Result<Vec<ObjectiveMetadata>> {
        if self.has_failures() {
            let failure_msgs: Vec<String> = self
                .failed
                .iter()
                .map(|(f, e)| format!("  - {f}: {e}"))
                .collect();
            Err(crate::error::JanusError::ObjectiveLoadFailed(failure_msgs))
        } else {
            Ok(self.items)
        }
    }

    /// Get just the objectives, ignoring failures
    pub fn into_objectives(self) -> Vec<ObjectiveMetadata> {
        self.items
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_objective_metadata_default() {
        let meta = ObjectiveMetadata::default();
        assert!(meta.id.is_none());
        assert!(meta.uuid.is_none());
        assert!(meta.created.is_none());
        assert!(meta.satisfied_by.is_empty());
        assert!(meta.title.is_none());
        assert!(meta.description.is_none());
        assert!(meta.acceptance_criteria.is_empty());
        assert!(meta.file_path.is_none());
    }

    #[test]
    fn test_objective_metadata_serialization() {
        use serde_yaml_ng as yaml;

        let meta = ObjectiveMetadata {
            id: Some(ObjectiveId::new_unchecked("objv-test")),
            uuid: Some("550e8400-e29b-41d4-a716-446655440000".to_string()),
            satisfied_by: vec!["plan-x1y2".to_string()],
            ..Default::default()
        };

        let yaml_str = yaml::to_string(&meta).unwrap();
        assert!(yaml_str.contains("id: objv-test"));
        assert!(yaml_str.contains("- plan-x1y2"));
    }

    #[test]
    fn test_objective_metadata_skip_none_fields() {
        use serde_yaml_ng as yaml;

        let meta = ObjectiveMetadata {
            id: Some(ObjectiveId::new_unchecked("objv-test")),
            ..Default::default()
        };

        let yaml_str = yaml::to_string(&meta).unwrap();
        assert!(!yaml_str.contains("satisfied-by"));
        assert!(!yaml_str.contains("uuid"));
    }

    #[test]
    fn test_objective_metadata_deserialization() {
        use serde_yaml_ng as yaml;

        let yaml_str = r#"
id: objv-test
uuid: 550e8400-e29b-41d4-a716-446655440000
satisfied-by:
  - plan-x1y2
"#;
        let meta: ObjectiveMetadata = yaml::from_str(yaml_str).unwrap();
        assert_eq!(meta.id.as_deref(), Some("objv-test"));
        assert_eq!(
            meta.uuid,
            Some("550e8400-e29b-41d4-a716-446655440000".to_string())
        );
        assert_eq!(meta.satisfied_by, vec!["plan-x1y2".to_string()]);
    }

    #[test]
    fn test_objective_metadata_deserialization_sequence() {
        use serde_yaml_ng as yaml;

        let yaml_str = r#"
id: objv-test
satisfied-by:
  - plan-x1y2
"#;
        let meta: ObjectiveMetadata = yaml::from_str(yaml_str).unwrap();
        assert_eq!(meta.satisfied_by, vec!["plan-x1y2".to_string()]);
    }

    #[test]
    fn test_objective_load_result() {
        let mut result = ObjectiveLoadResult::new();
        assert!(!result.has_failures());
        assert_eq!(result.success_count(), 0);

        let meta = ObjectiveMetadata {
            id: Some(ObjectiveId::new_unchecked("objv-test")),
            ..Default::default()
        };
        result.add_objective(meta);
        assert_eq!(result.success_count(), 1);
    }
}

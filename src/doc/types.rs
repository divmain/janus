use std::collections::HashMap;
use std::fmt;
use std::ops::Deref;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::types::CreatedAt;

/// A validated document label (filesystem-safe string).
///
/// Labels are free-form but must be filesystem-safe (no path separators,
/// no leading/trailing whitespace, and limited special characters).
/// They are used as filenames in `.janus/docs/{label}.md`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct DocLabel(String);

impl DocLabel {
    /// Create a `DocLabel` from a string, validating filesystem safety.
    pub fn new(s: impl Into<String>) -> crate::error::Result<Self> {
        let s = s.into();
        Self::validate(&s)?;
        Ok(DocLabel(s))
    }

    /// Create a `DocLabel` without validation.
    ///
    /// Use this only when you know the value is already valid
    /// (e.g., read from a trusted source).
    pub(crate) fn new_unchecked(s: impl Into<String>) -> Self {
        DocLabel(s.into())
    }

    /// Consume self and return the inner `String`.
    pub fn into_inner(self) -> String {
        self.0
    }

    /// Validate a document label string.
    ///
    /// Rules:
    /// - Must be non-empty
    /// - Must not contain path separators (/ or \)
    /// - Must not contain null bytes
    /// - Must not be ".." or "."
    /// - Leading/trailing whitespace is trimmed
    /// - Limited special characters allowed (hyphens, underscores, alphanumeric, spaces)
    fn validate(s: &str) -> crate::error::Result<()> {
        let s = s.trim();

        if s.is_empty() {
            return Err(crate::error::JanusError::InvalidDocLabel(
                "label cannot be empty".to_string(),
            ));
        }

        if s == "." || s == ".." {
            return Err(crate::error::JanusError::InvalidDocLabel(format!(
                "label cannot be '{s}'"
            )));
        }

        if s.contains('\n') || s.contains('\r') || s.contains('\t') {
            return Err(crate::error::JanusError::InvalidDocLabel(
                "label cannot contain newlines or tabs".to_string(),
            ));
        }

        if s.contains('/') || s.contains('\\') {
            return Err(crate::error::JanusError::InvalidDocLabel(
                "label cannot contain path separators".to_string(),
            ));
        }

        if s.contains('\0') {
            return Err(crate::error::JanusError::InvalidDocLabel(
                "label cannot contain null bytes".to_string(),
            ));
        }

        // Check for valid characters (alphanumeric, spaces, hyphens, underscores, periods)
        if !s
            .chars()
            .all(|c| c.is_alphanumeric() || c.is_whitespace() || c == '-' || c == '_' || c == '.')
        {
            return Err(crate::error::JanusError::InvalidDocLabel(format!(
                "label contains invalid characters: '{s}'"
            )));
        }

        Ok(())
    }
}

impl Deref for DocLabel {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for DocLabel {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DocLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::str::FromStr for DocLabel {
    type Err = crate::error::JanusError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        DocLabel::new(s)
    }
}

impl<'de> Deserialize<'de> for DocLabel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        DocLabel::new(s).map_err(serde::de::Error::custom)
    }
}

/// Metadata parsed from a document file's YAML frontmatter and markdown body.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DocMetadata {
    /// Document label (e.g., "Architecture", "API-Design")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<DocLabel>,

    /// Optional description from frontmatter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Tags for categorization
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// Creation timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<CreatedAt>,

    /// Last updated timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated: Option<CreatedAt>,

    /// Title extracted from first H1 heading (runtime-only)
    #[serde(skip)]
    pub title: Option<String>,

    /// Path to the document file on disk (runtime-only)
    #[serde(skip)]
    pub file_path: Option<PathBuf>,

    /// Unknown/extra YAML frontmatter keys preserved for round-trip fidelity.
    #[serde(skip)]
    pub extra_frontmatter: Option<HashMap<String, serde_yaml_ng::Value>>,
}

impl DocMetadata {
    /// Get the document label as a string slice
    pub fn label(&self) -> Option<&str> {
        self.label.as_ref().map(|l| l.as_ref())
    }

    /// Get the document title
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Get the file path
    pub fn file_path(&self) -> Option<&PathBuf> {
        self.file_path.as_ref()
    }

    /// Get the item type
    pub fn item_type(&self) -> crate::types::EntityType {
        crate::types::EntityType::Doc
    }
}

/// A chunk of a document, typically bounded by headings.
///
/// Represents a section of a document for semantic search and display.
/// Chunks are created at heading boundaries and track the heading hierarchy
/// for context-aware search results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocChunk {
    /// The document label this chunk belongs to
    pub label: String,

    /// The heading path (e.g., ["Architecture", "API Design"])
    /// Empty for headerless regions (intro paragraphs)
    pub heading_path: Vec<String>,

    /// The chunk content
    pub content: String,

    /// Start line number in the original document (1-indexed)
    pub start_line: usize,

    /// End line number in the original document (inclusive)
    pub end_line: usize,
}

impl DocChunk {
    /// Create a new document chunk
    pub fn new(
        label: impl Into<String>,
        heading_path: Vec<String>,
        content: impl Into<String>,
        start_line: usize,
        end_line: usize,
    ) -> Self {
        DocChunk {
            label: label.into(),
            heading_path,
            content: content.into(),
            start_line,
            end_line,
        }
    }

    /// Get the full heading path as a string
    pub fn heading_path_string(&self) -> String {
        if self.heading_path.is_empty() {
            return "(intro)".to_string();
        }
        self.heading_path.join(" > ")
    }

    /// Get the chunk content length
    pub fn content_len(&self) -> usize {
        self.content.len()
    }
}

/// Result of loading documents from disk, including both successes and failures.
pub type DocLoadResult = crate::types::LoadResult<DocMetadata>;

impl DocLoadResult {
    /// Add a successfully loaded document
    pub fn add_doc(&mut self, doc: DocMetadata) {
        self.items.push(doc);
    }

    /// Convert to a Result, returning Err if there are failures
    pub fn into_result(self) -> crate::error::Result<Vec<DocMetadata>> {
        if self.has_failures() {
            let failure_msgs: Vec<String> = self
                .failed
                .iter()
                .map(|(f, e)| format!("  - {f}: {e}"))
                .collect();
            Err(crate::error::JanusError::DocLoadFailed(failure_msgs))
        } else {
            Ok(self.items)
        }
    }

    /// Get just the documents, ignoring failures
    pub fn into_docs(self) -> Vec<DocMetadata> {
        self.items
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_doc_label_validation() {
        // Valid labels
        assert!(DocLabel::new("Architecture").is_ok());
        assert!(DocLabel::new("API-Design").is_ok());
        assert!(DocLabel::new("getting_started").is_ok());
        assert!(DocLabel::new("v2.0").is_ok());
        assert!(DocLabel::new("Test Doc").is_ok());

        // Invalid labels
        assert!(DocLabel::new("").is_err());
        assert!(DocLabel::new("  ").is_err());
        assert!(DocLabel::new("path/to/file").is_err());
        assert!(DocLabel::new("path\\\\to\\\\file").is_err());
        assert!(DocLabel::new("..").is_err());
        assert!(DocLabel::new(".").is_err());
        assert!(DocLabel::new("file\nname").is_err());
        assert!(DocLabel::new("file\tname").is_err());
    }

    #[test]
    fn test_doc_label_display() {
        let label = DocLabel::new("Test-Doc").unwrap();
        assert_eq!(format!("{label}"), "Test-Doc");
    }

    #[test]
    fn test_doc_label_from_str() {
        let label: DocLabel = "Valid-Label".parse().unwrap();
        assert_eq!(label.as_ref(), "Valid-Label");
    }

    #[test]
    fn test_doc_chunk_new() {
        let chunk = DocChunk::new(
            "test-doc",
            vec!["Section 1".to_string(), "Subsection".to_string()],
            "This is the content.",
            10,
            15,
        );
        assert_eq!(chunk.label, "test-doc");
        assert_eq!(chunk.heading_path, vec!["Section 1", "Subsection"]);
        assert_eq!(chunk.content, "This is the content.");
        assert_eq!(chunk.start_line, 10);
        assert_eq!(chunk.end_line, 15);
    }

    #[test]
    fn test_doc_chunk_heading_path_string() {
        let chunk = DocChunk::new(
            "test-doc",
            vec!["Architecture".to_string(), "API".to_string()],
            "content",
            1,
            5,
        );
        assert_eq!(chunk.heading_path_string(), "Architecture > API");

        let intro_chunk = DocChunk::new("test-doc", vec![], "intro", 1, 3);
        assert_eq!(intro_chunk.heading_path_string(), "(intro)");
    }

    #[test]
    fn test_doc_metadata_default() {
        let meta = DocMetadata::default();
        assert!(meta.label.is_none());
        assert!(meta.description.is_none());
        assert!(meta.tags.is_empty());
        assert!(meta.created.is_none());
        assert!(meta.updated.is_none());
        assert!(meta.title.is_none());
        assert!(meta.file_path.is_none());
    }

    #[test]
    fn test_doc_load_result() {
        let mut result = DocLoadResult::new();
        assert!(!result.has_failures());
        assert_eq!(result.success_count(), 0);
        assert_eq!(result.failure_count(), 0);

        let meta = DocMetadata {
            label: Some(DocLabel::new_unchecked("test")),
            ..Default::default()
        };
        result.add_doc(meta);
        assert_eq!(result.success_count(), 1);
    }
}

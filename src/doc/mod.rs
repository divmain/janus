//! Document module for project knowledge documents.
//!
//! This module provides the `Doc` type which handles file I/O for project
//! knowledge documents stored in `.janus/docs/`. Documents are markdown
//! files with YAML frontmatter containing metadata like label, description,
//! and tags.

pub mod chunker;
pub mod parser;
pub mod types;

pub use chunker::chunk_document;
pub use parser::{parse_doc_content, serialize_doc};
pub use types::{DocChunk, DocLabel, DocLoadResult, DocMetadata};

use std::fs;
use std::path::PathBuf;

use crate::entity::Entity;
use crate::error::{JanusError, Result};
use crate::utils::find_markdown_files;

/// A document handle for reading and writing document files.
///
/// Documents are stored as Markdown files with YAML frontmatter in `.janus/docs/`.
/// This struct provides direct file I/O operations for reading and writing document files.
#[derive(Debug, Clone)]
pub struct Doc {
    /// Path to the document file
    pub file_path: PathBuf,
    /// Document label
    pub label: String,
}

impl Doc {
    /// Find a document by its partial label.
    ///
    /// Searches for a document matching the given partial label and returns a Doc
    /// if found uniquely.
    pub async fn find(partial_label: &str) -> Result<Self> {
        let trimmed = partial_label.trim();
        if trimmed.is_empty() {
            return Err(JanusError::InvalidDocLabel(
                "label cannot be empty".to_string(),
            ));
        }

        // Check for invalid characters
        if !trimmed
            .chars()
            .all(|c| c.is_alphanumeric() || c.is_whitespace() || c == '-' || c == '_' || c == '.')
        {
            return Err(JanusError::InvalidDocLabel(format!(
                "label contains invalid characters: '{trimmed}'"
            )));
        }

        let docs_dir = crate::paths::docs_dir();

        // Use filesystem lookup for now (store integration in Phase 2)
        find_doc_by_label_filesystem(trimmed, &docs_dir)
    }

    /// Create a Doc from an existing file path.
    ///
    /// Extracts the label from the file path's stem.
    pub fn new(file_path: PathBuf) -> Result<Self> {
        let label = file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| {
                JanusError::InvalidFormat(format!(
                    "Invalid document file path: {}",
                    file_path.display()
                ))
            })?
            .to_string();

        Ok(Doc { file_path, label })
    }

    /// Create a new Doc with the given label.
    ///
    /// Validates the label and constructs the file path.
    pub fn with_label(label: &str) -> Result<Self> {
        let label = DocLabel::new(label)?;
        let file_path = crate::paths::docs_dir().join(format!("{label}.md"));
        Ok(Doc {
            file_path,
            label: label.into_inner(),
        })
    }

    /// Read and parse the document's metadata.
    pub fn read(&self) -> Result<DocMetadata> {
        let content = self.read_content()?;
        let mut metadata = parse_doc_content(&content)?;
        metadata.file_path = Some(self.file_path.clone());
        Ok(metadata)
    }

    /// Read the raw content of the document file.
    pub fn read_content(&self) -> Result<String> {
        fs::read_to_string(&self.file_path).map_err(|e| JanusError::StorageError {
            operation: "read",
            item_type: "doc",
            path: self.file_path.clone(),
            source: e,
        })
    }

    /// Write content to the document file.
    pub fn write(&self, content: &str) -> Result<()> {
        self.ensure_parent_dir()?;
        crate::fs::write_file_atomic(&self.file_path, content)
    }

    /// Write metadata to the document file.
    pub fn write_metadata(&self, metadata: &DocMetadata) -> Result<()> {
        let content = serialize_doc(metadata)?;
        self.write(&content)
    }

    /// Ensure the parent directory exists.
    fn ensure_parent_dir(&self) -> Result<()> {
        crate::fs::ensure_parent_dir(&self.file_path)
    }

    /// Check if the document file exists.
    pub fn exists(&self) -> bool {
        self.file_path.exists()
    }

    /// Delete the document file.
    pub fn delete(&self) -> Result<()> {
        if let Err(e) = fs::remove_file(&self.file_path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(JanusError::StorageError {
                    operation: "delete",
                    item_type: "doc",
                    path: self.file_path.clone(),
                    source: e,
                });
            }
        }
        Ok(())
    }

    /// Delete the document file (async version).
    pub async fn delete_async(&self) -> Result<()> {
        tokio::fs::remove_file(&self.file_path).await.map_err(|e| {
            if e.kind() != std::io::ErrorKind::NotFound {
                JanusError::StorageError {
                    operation: "delete",
                    item_type: "doc",
                    path: self.file_path.clone(),
                    source: e,
                }
            } else {
                JanusError::InvalidFormat("file not found".to_string())
            }
        })?;
        Ok(())
    }
}

impl Entity for Doc {
    type Metadata = DocMetadata;

    async fn find(partial_id: &str) -> Result<Self> {
        Doc::find(partial_id).await
    }

    fn read(&self) -> Result<DocMetadata> {
        self.read()
    }

    fn write(&self, content: &str) -> Result<()> {
        self.write(content)
    }

    fn delete(&self) -> Result<()> {
        self.delete()
    }

    fn exists(&self) -> bool {
        self.exists()
    }
}

/// Find a document by label using filesystem fallback.
fn find_doc_by_label_filesystem(partial_label: &str, dir: &std::path::Path) -> Result<Doc> {
    let files = find_markdown_files(dir).map_err(|e| JanusError::StorageError {
        operation: "read",
        item_type: "directory",
        path: dir.to_path_buf(),
        source: e,
    })?;

    // Check for exact match first
    let exact_name = format!("{partial_label}.md");
    if files.iter().any(|f| f == &exact_name) {
        let file_path = dir.join(&exact_name);
        return Doc::new(file_path);
    }

    // Then check for partial matches
    let matches: Vec<_> = files
        .iter()
        .filter(|f| {
            let stem = f.strip_suffix(".md").unwrap_or(f);
            stem.contains(partial_label)
        })
        .collect();

    match matches.len() {
        0 => Err(JanusError::DocNotFound(partial_label.to_string())),
        1 => {
            let file_path = dir.join(matches[0]);
            Doc::new(file_path)
        }
        _ => {
            let labels: Vec<String> = matches
                .iter()
                .map(|m| m.strip_suffix(".md").unwrap_or(m).to_string())
                .collect();
            Err(JanusError::AmbiguousDocLabel(
                partial_label.to_string(),
                labels,
            ))
        }
    }
}

/// Get all documents from disk.
pub fn get_all_docs_from_disk() -> DocLoadResult {
    let mut result = DocLoadResult::new();
    let d_dir = crate::paths::docs_dir();

    // If docs directory doesn't exist, return empty result
    if !d_dir.exists() {
        return result;
    }

    let files = match find_markdown_files(&d_dir) {
        Ok(files) => files,
        Err(e) => {
            result.add_failure("<docs directory>", format!("failed to read directory: {e}"));
            return result;
        }
    };

    for file in files {
        let file_path = d_dir.join(&file);
        match fs::read_to_string(&file_path) {
            Ok(content) => match parse_doc_content(&content) {
                Ok(mut metadata) => {
                    // If label is not set, use filename stem
                    if metadata.label.is_none() {
                        let label = file.strip_suffix(".md").unwrap_or(&file).to_string();
                        metadata.label = Some(DocLabel::new_unchecked(label));
                    }
                    metadata.file_path = Some(file_path);
                    result.add_doc(metadata);
                }
                Err(e) => {
                    result.add_failure(&file, format!("parse error: {e}"));
                }
            },
            Err(e) => {
                result.add_failure(&file, format!("read error: {e}"));
            }
        }
    }

    result
}

/// Ensure the docs directory exists.
pub fn ensure_docs_dir() -> Result<()> {
    let d_dir = crate::paths::docs_dir();
    fs::create_dir_all(&d_dir).map_err(|e| JanusError::StorageError {
        operation: "create",
        item_type: "directory",
        path: d_dir.clone(),
        source: e,
    })?;
    crate::utils::ensure_gitignore();
    Ok(())
}

/// Generate a sanitized label from a title.
///
/// Converts the title to a filesystem-safe label by:
/// - Converting to lowercase
/// - Replacing spaces with hyphens
/// - Removing invalid characters
/// - Truncating to a reasonable length
pub fn sanitize_label(title: &str) -> String {
    let mut label = title.to_lowercase();
    label = label.replace(' ', "-");
    label = label
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    label.truncate(50);
    label.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_doc_with_label() {
        let doc = Doc::with_label("Test-Doc").unwrap();
        assert_eq!(doc.label, "Test-Doc");
        assert!(doc.file_path.to_string_lossy().contains("Test-Doc.md"));
    }

    #[test]
    fn test_doc_with_label_invalid() {
        let result = Doc::with_label("path/to/file");
        assert!(result.is_err());
    }

    #[test]
    fn test_sanitize_label() {
        assert_eq!(sanitize_label("Hello World"), "hello-world");
        assert_eq!(sanitize_label("API Design"), "api-design");
        assert_eq!(sanitize_label("Test--Doc"), "test--doc");
        assert_eq!(sanitize_label("Special!@#Chars"), "specialchars");
    }

    #[test]
    fn test_sanitize_label_truncation() {
        let long_title = "a".repeat(100);
        let label = sanitize_label(&long_title);
        assert!(label.len() <= 50);
    }
}

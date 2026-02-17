//! Semantic search for project knowledge documents.
//!
//! This module provides document-level and chunk-level semantic search
//! capabilities, allowing users to find relevant content within project
//! knowledge documents.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use super::TicketStore;
use crate::doc::types::DocMetadata;
use crate::embedding::model::cosine_similarity;

/// Result of a document semantic search.
#[derive(Debug, Clone)]
pub struct DocSearchResult {
    /// The document label (e.g., "architecture", "api-design")
    pub label: String,
    /// The document metadata
    pub doc: DocMetadata,
    /// The heading path where the match was found (e.g., ["Section 1", "Subsection"])
    /// Empty for document-level matches
    pub heading_path: Vec<String>,
    /// A snippet of the matched content
    pub content_snippet: String,
    /// The line range in the original document (start, end)
    pub line_range: (usize, usize),
    /// Cosine similarity score
    pub similarity: f32,
}

/// A scored candidate for top-K selection via a min-heap.
///
/// Wraps a document/chunk key and similarity score.
struct ScoredDocCandidate {
    key: String,
    similarity: f32,
}

impl PartialEq for ScoredDocCandidate {
    fn eq(&self, other: &Self) -> bool {
        self.similarity == other.similarity
    }
}

impl Eq for ScoredDocCandidate {}

impl PartialOrd for ScoredDocCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScoredDocCandidate {
    fn cmp(&self, other: &Self) -> Ordering {
        // Ascending order for min-heap behavior
        other
            .similarity
            .partial_cmp(&self.similarity)
            .unwrap_or(Ordering::Equal)
    }
}

impl TicketStore {
    /// Perform semantic search across documents.
    ///
    /// Searches both document-level embeddings and chunk-level embeddings,
    /// returning results ranked by similarity. Results are grouped by document
    /// and include context about where in the document the match was found.
    ///
    /// # Arguments
    ///
    /// * `query_embedding` - The embedding vector of the search query
    /// * `limit` - Maximum number of results to return
    ///
    /// # Returns
    ///
    /// A vector of `DocSearchResult` sorted by similarity (highest first).
    pub fn doc_search(&self, query_embedding: &[f32], limit: usize) -> Vec<DocSearchResult> {
        if limit == 0 {
            return Vec::new();
        }

        let mut heap: BinaryHeap<ScoredDocCandidate> = BinaryHeap::with_capacity(limit + 1);

        // Search through all embeddings that start with "doc:"
        for entry in self.embeddings().iter() {
            let key = entry.key();
            if !key.starts_with("doc:") {
                continue;
            }

            let similarity = cosine_similarity(query_embedding, entry.value());

            if heap.len() < limit {
                heap.push(ScoredDocCandidate {
                    key: key.clone(),
                    similarity,
                });
            } else if let Some(min) = heap.peek() {
                if similarity > min.similarity {
                    heap.pop();
                    heap.push(ScoredDocCandidate {
                        key: key.clone(),
                        similarity,
                    });
                }
            }
        }

        // Convert heap entries to results
        let mut results: Vec<DocSearchResult> = heap
            .into_iter()
            .filter_map(|candidate| self.build_doc_result(&candidate.key, candidate.similarity))
            .collect();

        // Sort by similarity descending
        results.sort_by(|a, b| {
            b.similarity
                .partial_cmp(&a.similarity)
                .unwrap_or(Ordering::Equal)
        });

        results
    }

    /// Build a DocSearchResult from an embedding key and similarity.
    ///
    /// The key format is:
    /// - `doc:{label}` for document-level embeddings
    /// - `doc:{label}:c{line}` for chunk-level embeddings
    fn build_doc_result(&self, key: &str, similarity: f32) -> Option<DocSearchResult> {
        // Parse the key to extract label and chunk info
        let (label, chunk_info) = if let Some(chunk_start) = key.find(":c") {
            // This is a chunk key: doc:{label}:c{line}
            let label = key[4..chunk_start].to_string();
            let line_num = key[chunk_start + 2..].parse::<usize>().ok()?;
            (label, Some(line_num))
        } else {
            // This is a document key: doc:{label}
            let label = key[4..].to_string();
            (label, None)
        };

        // Get the document metadata
        let doc = self.docs().get(&label)?;

        if let Some(start_line) = chunk_info {
            // This is a chunk result - try to get the actual chunk content
            let (heading_path, content_snippet, line_range) =
                if let Some(file_path) = doc.file_path.clone() {
                    self.get_chunk_info(&label, start_line, &file_path)
                } else {
                    // Fallback if file path is not available
                    (
                        vec![],
                        "(chunk content unavailable)".to_string(),
                        (start_line, start_line + 10),
                    )
                };

            Some(DocSearchResult {
                label: label.clone(),
                doc: doc.clone(),
                heading_path,
                content_snippet,
                line_range,
                similarity,
            })
        } else {
            // This is a document-level result
            Some(DocSearchResult {
                label: label.clone(),
                doc: doc.clone(),
                heading_path: vec![],
                content_snippet: doc.description.clone().unwrap_or_default(),
                line_range: (0, 0),
                similarity,
            })
        }
    }

    /// Get chunk information from a document.
    ///
    /// Re-chunks the document to find the chunk at the given start line.
    fn get_chunk_info(
        &self,
        label: &str,
        start_line: usize,
        file_path: &std::path::Path,
    ) -> (Vec<String>, String, (usize, usize)) {
        use crate::doc::chunk_document;

        // Read the document content
        match std::fs::read_to_string(file_path) {
            Ok(content) => {
                // Chunk the document
                match chunk_document(label, &content) {
                    Ok(chunks) => {
                        // Find the chunk with the matching start line
                        if let Some(chunk) = chunks.iter().find(|c| c.start_line == start_line) {
                            // Create content snippet (first 200 chars)
                            let snippet = if chunk.content.len() > 200 {
                                format!("{}...", &chunk.content[..200])
                            } else {
                                chunk.content.clone()
                            };

                            (
                                chunk.heading_path.clone(),
                                snippet,
                                (chunk.start_line, chunk.end_line),
                            )
                        } else {
                            // Chunk not found at that line
                            (
                                vec!["(unknown section)".to_string()],
                                "(content unavailable)".to_string(),
                                (start_line, start_line + 10),
                            )
                        }
                    }
                    Err(_) => (
                        vec!["(parse error)".to_string()],
                        "(content unavailable)".to_string(),
                        (start_line, start_line + 10),
                    ),
                }
            }
            Err(_) => (
                vec!["(read error)".to_string()],
                "(content unavailable)".to_string(),
                (start_line, start_line + 10),
            ),
        }
    }

    /// Search documents with a threshold filter.
    ///
    /// Similar to `doc_search`, but only returns results above the given
    /// similarity threshold.
    pub fn doc_search_with_threshold(
        &self,
        query_embedding: &[f32],
        limit: usize,
        threshold: f32,
    ) -> Vec<DocSearchResult> {
        self.doc_search(query_embedding, limit)
            .into_iter()
            .filter(|r| r.similarity >= threshold)
            .collect()
    }

    /// Perform semantic search within a specific document.
    ///
    /// Searches only embeddings for the specified document (both document-level
    /// and chunk-level), returning results ranked by similarity.
    ///
    /// # Arguments
    ///
    /// * `query_embedding` - The embedding vector of the search query
    /// * `label` - The exact document label to search within
    /// * `limit` - Maximum number of results to return
    ///
    /// # Returns
    ///
    /// A vector of `DocSearchResult` sorted by similarity (highest first).
    pub fn doc_search_by_document(
        &self,
        query_embedding: &[f32],
        label: &str,
        limit: usize,
    ) -> Vec<DocSearchResult> {
        if limit == 0 {
            return Vec::new();
        }

        let mut heap: BinaryHeap<ScoredDocCandidate> = BinaryHeap::with_capacity(limit + 1);

        // Build prefix for this document's embeddings
        let doc_prefix = format!("doc:{label}:");
        let doc_key = format!("doc:{label}");

        // Search through embeddings for this specific document
        for entry in self.embeddings().iter() {
            let key = entry.key();
            // Match either doc:{label} (document-level) or doc:{label}:c{line} (chunk-level)
            if key == &doc_key || key.starts_with(&doc_prefix) {
                let similarity = cosine_similarity(query_embedding, entry.value());

                if heap.len() < limit {
                    heap.push(ScoredDocCandidate {
                        key: key.clone(),
                        similarity,
                    });
                } else if let Some(min) = heap.peek() {
                    if similarity > min.similarity {
                        heap.pop();
                        heap.push(ScoredDocCandidate {
                            key: key.clone(),
                            similarity,
                        });
                    }
                }
            }
        }

        // Convert heap entries to results
        let mut results: Vec<DocSearchResult> = heap
            .into_iter()
            .filter_map(|candidate| self.build_doc_result(&candidate.key, candidate.similarity))
            .collect();

        // Sort by similarity descending
        results.sort_by(|a, b| {
            b.similarity
                .partial_cmp(&a.similarity)
                .unwrap_or(Ordering::Equal)
        });

        results
    }
}

#[cfg(test)]
mod tests {
    use crate::doc::types::{DocLabel, DocMetadata};
    use crate::store::TicketStore;

    fn test_store_with_docs() -> TicketStore {
        let store = TicketStore::empty();

        // Document 1: "rust" direction
        store.upsert_doc(DocMetadata {
            label: Some(DocLabel::new_unchecked("rust-guide")),
            title: Some("Rust Programming Guide".to_string()),
            description: Some("A guide to Rust programming".to_string()),
            ..Default::default()
        });
        // Embedding that points in the "rust" direction
        store
            .embeddings()
            .insert("doc:rust-guide".to_string(), vec![0.9, 0.1, 0.0]);

        // Document 2: "python" direction
        store.upsert_doc(DocMetadata {
            label: Some(DocLabel::new_unchecked("python-guide")),
            title: Some("Python Programming Guide".to_string()),
            description: Some("A guide to Python programming".to_string()),
            ..Default::default()
        });
        // Embedding that points in the "python" direction
        store
            .embeddings()
            .insert("doc:python-guide".to_string(), vec![0.0, 0.9, 0.1]);

        // Document 3: "javascript" direction
        store.upsert_doc(DocMetadata {
            label: Some(DocLabel::new_unchecked("js-guide")),
            title: Some("JavaScript Guide".to_string()),
            description: Some("A guide to JavaScript".to_string()),
            ..Default::default()
        });
        // Embedding that points in the "js" direction
        store
            .embeddings()
            .insert("doc:js-guide".to_string(), vec![0.1, 0.0, 0.9]);

        // Document 4: no embedding
        store.upsert_doc(DocMetadata {
            label: Some(DocLabel::new_unchecked("no-embedding")),
            title: Some("Doc without embedding".to_string()),
            ..Default::default()
        });

        // Add a chunk embedding for rust-guide
        store
            .embeddings()
            .insert("doc:rust-guide:c10".to_string(), vec![0.95, 0.05, 0.0]);

        store
    }

    #[test]
    fn test_doc_search_basic() {
        let store = test_store_with_docs();

        // Query similar to "rust" direction
        let query = vec![1.0_f32, 0.0, 0.0];
        let results = store.doc_search(&query, 10);

        // We have 4 embeddings: 3 doc-level + 1 chunk for rust-guide
        assert_eq!(results.len(), 4);

        // Results should be sorted by similarity (highest first)
        assert_eq!(results[0].label, "rust-guide");
        assert!(results[0].similarity > results[1].similarity);
    }

    #[test]
    fn test_doc_search_limit() {
        let store = test_store_with_docs();

        let query = vec![1.0_f32, 0.0, 0.0];
        let results = store.doc_search(&query, 1);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].label, "rust-guide");
    }

    #[test]
    fn test_doc_search_zero_limit() {
        let store = test_store_with_docs();

        let query = vec![1.0_f32, 0.0, 0.0];
        let results = store.doc_search(&query, 0);

        assert!(results.is_empty());
    }

    #[test]
    fn test_doc_search_skips_docs_without_embeddings() {
        let store = test_store_with_docs();

        let query = vec![1.0_f32, 0.0, 0.0];
        let results = store.doc_search(&query, 10);

        // no-embedding should not appear in results
        for result in &results {
            assert_ne!(result.label, "no-embedding");
        }
    }

    #[test]
    fn test_doc_search_python_direction() {
        let store = test_store_with_docs();

        // Query similar to "python" direction
        let query = vec![0.0_f32, 1.0, 0.0];
        let results = store.doc_search(&query, 10);

        assert_eq!(results[0].label, "python-guide");
    }

    #[test]
    fn test_doc_search_similarity_range() {
        let store = test_store_with_docs();

        let query = vec![1.0_f32, 0.0, 0.0];
        let results = store.doc_search(&query, 10);

        for result in &results {
            // Cosine similarity should be between -1 and 1
            assert!(result.similarity >= -1.0);
            assert!(result.similarity <= 1.0);
        }
    }

    #[test]
    fn test_doc_search_sorted_descending() {
        let store = test_store_with_docs();

        let query = vec![0.5_f32, 0.5, 0.0];
        let results = store.doc_search(&query, 10);

        for window in results.windows(2) {
            assert!(window[0].similarity >= window[1].similarity);
        }
    }

    #[test]
    fn test_doc_search_empty_store() {
        let store = TicketStore::empty();

        let query = vec![1.0_f32, 0.0, 0.0];
        let results = store.doc_search(&query, 10);

        assert!(results.is_empty());
    }

    #[test]
    fn test_doc_search_with_threshold() {
        let store = test_store_with_docs();

        let query = vec![1.0_f32, 0.0, 0.0];
        let results = store.doc_search_with_threshold(&query, 10, 0.5);

        // All results should be above threshold
        for result in &results {
            assert!(result.similarity >= 0.5);
        }
    }

    #[test]
    fn test_doc_search_includes_chunks() {
        let store = test_store_with_docs();

        let query = vec![1.0_f32, 0.0, 0.0];
        let results = store.doc_search(&query, 10);

        // Should find both the doc-level and chunk-level embeddings
        let rust_results: Vec<_> = results.iter().filter(|r| r.label == "rust-guide").collect();
        assert!(
            rust_results.len() >= 1,
            "Should find at least the doc-level result for rust-guide"
        );
    }

    #[test]
    fn test_doc_search_by_document_filters_to_specific_doc() {
        let store = test_store_with_docs();

        // Query similar to "rust" direction
        let query = vec![1.0_f32, 0.0, 0.0];

        // Search only in python-guide (which points in a different direction)
        let results = store.doc_search_by_document(&query, "python-guide", 10);

        // Should only get python-guide results, even though query is closer to rust
        for result in &results {
            assert_eq!(result.label, "python-guide");
        }
    }

    #[test]
    fn test_doc_search_by_document_includes_chunks() {
        let store = test_store_with_docs();

        let query = vec![1.0_f32, 0.0, 0.0];
        let results = store.doc_search_by_document(&query, "rust-guide", 10);

        // Should find both doc-level and chunk-level embeddings for rust-guide
        assert!(!results.is_empty());
        for result in &results {
            assert_eq!(result.label, "rust-guide");
        }
    }

    #[test]
    fn test_doc_search_by_document_empty_for_unknown_doc() {
        let store = test_store_with_docs();

        let query = vec![1.0_f32, 0.0, 0.0];
        let results = store.doc_search_by_document(&query, "nonexistent", 10);

        assert!(results.is_empty());
    }

    #[test]
    fn test_doc_search_by_document_zero_limit() {
        let store = test_store_with_docs();

        let query = vec![1.0_f32, 0.0, 0.0];
        let results = store.doc_search_by_document(&query, "rust-guide", 0);

        assert!(results.is_empty());
    }

    #[test]
    fn test_doc_search_by_document_sorted_descending() {
        let store = test_store_with_docs();

        let query = vec![1.0_f32, 0.0, 0.0];
        let results = store.doc_search_by_document(&query, "rust-guide", 10);

        for window in results.windows(2) {
            assert!(window[0].similarity >= window[1].similarity);
        }
    }
}

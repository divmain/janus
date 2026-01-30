//! Semantic search functionality for tickets using vector embeddings.
//!
//! This module provides methods for performing semantic search over ticket
//! content using Turso's vector similarity functions.

use crate::cache::TicketCache;
use crate::error::{JanusError, Result};
use crate::types::{TicketMetadata, TicketPriority, TicketStatus, TicketType};

#[cfg(feature = "semantic-search")]
use crate::embedding::model::generate_embedding;

/// Result of a semantic search query containing the ticket metadata and similarity score.
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// The matching ticket metadata
    pub ticket: TicketMetadata,
    /// Cosine similarity score (1.0 = identical, -1.0 = opposite)
    pub similarity: f32,
}

impl TicketCache {
    /// Perform semantic search over tickets using a text query.
    ///
    /// This method:
    /// 1. Generates an embedding for the query text
    /// 2. Uses Turso's vector_distance_cos function to find similar tickets
    /// 3. Returns results ordered by similarity (most similar first)
    ///
    /// # Arguments
    /// * `query` - The search query text
    /// * `limit` - Maximum number of results to return
    ///
    /// # Returns
    /// A vector of SearchResult structs containing matching tickets and their similarity scores
    ///
    /// # Errors
    /// Returns an error if:
    /// - The embedding model fails to generate an embedding
    /// - The database query fails
    #[cfg(feature = "semantic-search")]
    pub async fn semantic_search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        // Generate query embedding
        let query_embedding = generate_embedding(query).map_err(JanusError::EmbeddingModel)?;

        // Convert embedding to blob for SQL query
        let embedding_blob = embedding_to_blob(&query_embedding);

        // Get connection and execute vector similarity search
        let conn = self.create_connection().await?;

        // Use Turso's vector distance function
        // vector_distance_cos returns cosine distance (0 = identical, 2 = opposite)
        // Similarity = 1.0 - distance
        let mut rows = conn
            .query(
                "SELECT ticket_id, uuid, status, title, priority, ticket_type, deps, links, 
                    parent, created, external_ref, remote, completion_summary, spawned_from, 
                    spawn_context, depth, file_path, triaged, body, size,
                    1.0 - vector_distance_cos(embedding, ?1) as similarity_score
             FROM tickets 
             WHERE embedding IS NOT NULL
             ORDER BY vector_distance_cos(embedding, ?1)
             LIMIT ?2",
                (embedding_blob, limit as i64),
            )
            .await?;

        let mut results = Vec::new();

        while let Some(row) = rows.next().await? {
            let ticket = parse_ticket_row(&row)?;
            let similarity: f64 = row.get(20)?;

            results.push(SearchResult {
                ticket,
                similarity: similarity as f32,
            });
        }

        Ok(results)
    }

    /// Perform semantic search (stub when semantic-search feature is disabled).
    #[cfg(not(feature = "semantic-search"))]
    pub async fn semantic_search(&self, _query: &str, _limit: usize) -> Result<Vec<SearchResult>> {
        Err(JanusError::EmbeddingsNotAvailable)
    }

    /// Get statistics on embedding coverage in the cache.
    ///
    /// Returns a tuple of (tickets_with_embeddings, total_tickets)
    ///
    /// # Errors
    /// Returns an error if the database query fails
    pub async fn embedding_coverage(&self) -> Result<(usize, usize)> {
        let conn = self.create_connection().await?;

        // Get total ticket count
        let mut total_rows = conn.query("SELECT COUNT(*) FROM tickets", ()).await?;
        let total: i64 = if let Some(row) = total_rows.next().await? {
            row.get(0)?
        } else {
            0
        };

        // Get count of tickets with embeddings
        let mut with_emb_rows = conn
            .query(
                "SELECT COUNT(*) FROM tickets WHERE embedding IS NOT NULL",
                (),
            )
            .await?;
        let with_embeddings: i64 = if let Some(row) = with_emb_rows.next().await? {
            row.get(0)?
        } else {
            0
        };

        Ok((with_embeddings as usize, total as usize))
    }
}

/// Convert embedding vector to byte blob for storage.
/// Each f32 is serialized as 4 little-endian bytes.
#[cfg(feature = "semantic-search")]
fn embedding_to_blob(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// Parse a ticket row from the database into a TicketMetadata struct.
fn parse_ticket_row(row: &turso::Row) -> Result<TicketMetadata> {
    use crate::cache::TicketCache;

    let ticket_id: Option<String> = row.get(0).ok();
    let uuid: Option<String> = row.get(1).ok();
    let status_str: Option<String> = row.get(2).ok();
    let title: Option<String> = row.get(3).ok();
    let priority_num: Option<i64> = row.get(4).ok();
    let type_str: Option<String> = row.get(5).ok();
    let deps_json: Option<String> = row.get(6).ok();
    let links_json: Option<String> = row.get(7).ok();
    let parent: Option<String> = row.get(8).ok();
    let created: Option<String> = row.get(9).ok();
    let external_ref: Option<String> = row.get(10).ok();
    let remote: Option<String> = row.get(11).ok();
    let completion_summary: Option<String> = row.get(12).ok();
    let spawned_from: Option<String> = row.get(13).ok();
    let spawn_context: Option<String> = row.get(14).ok();
    let depth: Option<i64> = row.get(15).ok();
    let file_path: Option<String> = row.get(16).ok();
    let triaged_num: Option<i64> = row.get(17).ok();
    let body: Option<String> = row.get(18).ok();
    let size_str: Option<String> = row.get(19).ok();

    // Parse status
    let status = status_str.and_then(|s| s.parse::<TicketStatus>().ok());

    // Parse priority
    let priority = priority_num.and_then(|p| match p {
        0 => Some(TicketPriority::P0),
        1 => Some(TicketPriority::P1),
        2 => Some(TicketPriority::P2),
        3 => Some(TicketPriority::P3),
        4 => Some(TicketPriority::P4),
        _ => None,
    });

    // Parse type
    let ticket_type = type_str.and_then(|t| t.parse::<TicketType>().ok());

    // Parse size
    let size = size_str.and_then(|s| s.parse::<crate::types::TicketSize>().ok());

    // Parse deps and links from JSON
    let deps = deps_json
        .map(|json| TicketCache::deserialize_array(Some(&json)))
        .unwrap_or_else(|| Ok(Vec::new()))?;
    let links = links_json
        .map(|json| TicketCache::deserialize_array(Some(&json)))
        .unwrap_or_else(|| Ok(Vec::new()))?;

    // Parse triaged
    let triaged = triaged_num.map(|t| t != 0);

    // Parse depth
    let depth_u32 = depth.map(|d| d as u32);

    // Parse file path
    let file_path_buf = file_path.map(std::path::PathBuf::from);

    Ok(TicketMetadata {
        id: ticket_id,
        uuid,
        status,
        title,
        priority,
        ticket_type,
        deps,
        links,
        parent,
        created,
        external_ref,
        remote,
        completion_summary,
        spawned_from,
        spawn_context,
        depth: depth_u32,
        triaged,
        file_path: file_path_buf,
        body,
        size,
    })
}

#[cfg(test)]
mod tests {
    use crate::cache::TicketCache;
    #[cfg(not(feature = "semantic-search"))]
    use crate::error::JanusError;
    use serial_test::serial;
    use std::fs;

    fn create_test_ticket_with_body(
        dir: &std::path::Path,
        ticket_id: &str,
        title: &str,
        priority: u8,
        body: &str,
    ) -> std::path::PathBuf {
        let tickets_dir = dir.join(".janus/items");
        fs::create_dir_all(&tickets_dir).unwrap();

        let ticket_path = tickets_dir.join(format!("{}.md", ticket_id));
        let content = if body.is_empty() {
            format!(
                r#"---
id: {}
uuid: 550e8400-e29b-41d4-a716-446655440000
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: {}
---
# {}
"#,
                ticket_id, priority, title
            )
        } else {
            format!(
                r#"---
id: {}
uuid: 550e8400-e29b-41d4-a716-446655440000
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: {}
---
# {}

{}
"#,
                ticket_id, priority, title, body
            )
        };
        fs::write(&ticket_path, content).unwrap();
        ticket_path
    }

    #[tokio::test]
    #[serial]
    async fn test_embedding_coverage() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_embedding_coverage");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        // Create test tickets
        create_test_ticket_with_body(&repo_path, "j-a1b2", "Test Ticket 1", 2, "");
        create_test_ticket_with_body(&repo_path, "j-c3d4", "Test Ticket 2", 2, "");

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let (with_emb, total) = cache.embedding_coverage().await.unwrap();

        // Total should be 2
        assert_eq!(total, 2);

        // With embeddings count depends on feature flag
        // Without semantic-search, it should be 0
        // With semantic-search, it should be 2 (after sync generates embeddings)
        #[cfg(feature = "semantic-search")]
        assert_eq!(
            with_emb, 2,
            "With semantic-search feature, both tickets should have embeddings"
        );

        #[cfg(not(feature = "semantic-search"))]
        assert_eq!(
            with_emb, 0,
            "Without semantic-search feature, no tickets should have embeddings"
        );

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_embedding_coverage_empty_db() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_coverage_empty");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        // Create empty tickets directory
        let tickets_dir = repo_path.join(".janus/items");
        fs::create_dir_all(&tickets_dir).unwrap();

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let (with_emb, total) = cache.embedding_coverage().await.unwrap();

        assert_eq!(total, 0);
        assert_eq!(with_emb, 0);

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    #[cfg(feature = "semantic-search")]
    async fn test_semantic_search_basic() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_semantic_search");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        // Create test tickets with different content
        create_test_ticket_with_body(
            &repo_path,
            "j-a1b2",
            "Rust async programming",
            2,
            "Implement async functions and await syntax in Rust",
        );
        create_test_ticket_with_body(
            &repo_path,
            "j-c3d4",
            "Database schema design",
            2,
            "Design SQL tables and relationships for the application",
        );
        create_test_ticket_with_body(
            &repo_path,
            "j-e5f6",
            "Frontend UI components",
            2,
            "Build React components for the user interface",
        );

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        // Search for Rust-related content
        let results = cache
            .semantic_search("async rust programming", 5)
            .await
            .unwrap();

        // Should return results
        assert!(!results.is_empty(), "Semantic search should return results");

        // All results should have similarity scores
        for result in &results {
            assert!(
                result.similarity >= -1.0 && result.similarity <= 1.0,
                "Similarity should be between -1.0 and 1.0, got {}",
                result.similarity
            );
            assert!(result.ticket.id.is_some(), "Result should have a ticket ID");
        }

        // The Rust ticket should be the top result
        if let Some(first) = results.first() {
            assert_eq!(first.ticket.id.as_deref().unwrap(), "j-a1b2");
            assert!(
                first.similarity > 0.0,
                "Top result should have positive similarity"
            );
        }

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    #[cfg(feature = "semantic-search")]
    async fn test_semantic_search_limit() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_search_limit");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        // Create 5 test tickets
        for i in 0..5 {
            let id = format!("j-t{}", i);
            create_test_ticket_with_body(&repo_path, &id, &format!("Test Ticket {}", i), 2, "");
        }

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        // Search with limit of 2
        let results = cache.semantic_search("test ticket", 2).await.unwrap();

        // Should return at most 2 results
        assert!(results.len() <= 2, "Should respect the limit parameter");

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    #[cfg(feature = "semantic-search")]
    async fn test_semantic_search_no_embeddings() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_search_no_embeddings");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        // Create ticket directory but don't sync (so no embeddings)
        let tickets_dir = repo_path.join(".janus/items");
        fs::create_dir_all(&tickets_dir).unwrap();

        let cache = TicketCache::open().await.unwrap();
        // Don't sync - this means no tickets in DB

        // Search should return empty results when no embeddings
        let results = cache.semantic_search("test query", 5).await.unwrap();
        assert!(
            results.is_empty(),
            "Should return empty results when no tickets have embeddings"
        );

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    #[cfg(feature = "semantic-search")]
    async fn test_semantic_search_empty_query() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_search_empty_query");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_ticket_with_body(&repo_path, "j-a1b2", "Test Ticket", 2, "");

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        // Empty query should still work (may return arbitrary results)
        let results = cache.semantic_search("", 5).await.unwrap();
        // Should not error, may or may not return results
        assert!(
            results.len() <= 5,
            "Should respect the limit even with empty query"
        );

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    #[cfg(not(feature = "semantic-search"))]
    async fn test_semantic_search_disabled() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_search_disabled");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let tickets_dir = repo_path.join(".janus/items");
        fs::create_dir_all(&tickets_dir).unwrap();

        let cache = TicketCache::open().await.unwrap();

        // When semantic-search feature is disabled, should return EmbeddingsNotAvailable error
        let result = cache.semantic_search("test query", 5).await;

        match result {
            Err(JanusError::EmbeddingsNotAvailable) => {
                // Expected error
            }
            _ => panic!("Expected EmbeddingsNotAvailable error when feature is disabled"),
        }

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }
}

use crate::embedding::model::cosine_similarity;
use crate::store::TicketStore;
use crate::types::TicketMetadata;

/// Result of a semantic search, containing the matched ticket and its similarity score.
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// The matched ticket metadata.
    pub ticket: TicketMetadata,
    /// Cosine similarity score between the query embedding and the ticket embedding.
    pub similarity: f32,
}

impl TicketStore {
    /// Perform semantic search using brute-force cosine similarity.
    ///
    /// 1. Iterates all entries in the embeddings DashMap
    /// 2. Computes cosine similarity for each against `query_embedding`
    /// 3. Sorts by similarity descending
    /// 4. Returns top-k results (up to `limit`)
    pub fn semantic_search(&self, query_embedding: &[f32], limit: usize) -> Vec<SearchResult> {
        let mut results: Vec<SearchResult> = self
            .embeddings()
            .iter()
            .filter_map(|entry| {
                let ticket_id = entry.key();
                let ticket_embedding = entry.value();

                let similarity = cosine_similarity(query_embedding, ticket_embedding);

                // Look up the ticket metadata
                self.tickets()
                    .get(ticket_id)
                    .map(|ticket_ref| SearchResult {
                        ticket: ticket_ref.value().clone(),
                        similarity,
                    })
            })
            .collect();

        // Sort by similarity descending
        results.sort_by(|a, b| {
            b.similarity
                .partial_cmp(&a.similarity)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        results.truncate(limit);
        results
    }
}

#[cfg(test)]
mod tests {
    use crate::store::TicketStore;
    use crate::types::{TicketMetadata, TicketStatus};

    /// Helper to create a store with tickets and mock embeddings.
    fn test_store_with_embeddings() -> TicketStore {
        let store = TicketStore::empty();

        // Ticket 1: "authentication" direction
        store.upsert_ticket(TicketMetadata {
            id: Some("j-auth".to_string()),
            title: Some("Implement authentication".to_string()),
            status: Some(TicketStatus::New),
            ..Default::default()
        });
        // Embedding that points in the "auth" direction
        store
            .embeddings()
            .insert("j-auth".to_string(), vec![0.9, 0.1, 0.0]);

        // Ticket 2: "database" direction
        store.upsert_ticket(TicketMetadata {
            id: Some("j-db".to_string()),
            title: Some("Set up database".to_string()),
            status: Some(TicketStatus::InProgress),
            ..Default::default()
        });
        // Embedding that points in the "db" direction
        store
            .embeddings()
            .insert("j-db".to_string(), vec![0.0, 0.9, 0.1]);

        // Ticket 3: "ui" direction
        store.upsert_ticket(TicketMetadata {
            id: Some("j-ui".to_string()),
            title: Some("Build user interface".to_string()),
            status: Some(TicketStatus::New),
            ..Default::default()
        });
        // Embedding that points in the "ui" direction
        store
            .embeddings()
            .insert("j-ui".to_string(), vec![0.0, 0.0, 1.0]);

        // Ticket 4: no embedding
        store.upsert_ticket(TicketMetadata {
            id: Some("j-noembedding".to_string()),
            title: Some("Ticket without embedding".to_string()),
            status: Some(TicketStatus::New),
            ..Default::default()
        });

        store
    }

    #[test]
    fn test_semantic_search_basic() {
        let store = test_store_with_embeddings();

        // Query similar to "auth" direction
        let query = vec![1.0_f32, 0.0, 0.0];
        let results = store.semantic_search(&query, 10);

        assert_eq!(results.len(), 3); // Only tickets with embeddings
                                      // Most similar should be j-auth
        assert_eq!(results[0].ticket.id.as_deref(), Some("j-auth"));
        assert!(results[0].similarity > results[1].similarity);
    }

    #[test]
    fn test_semantic_search_limit() {
        let store = test_store_with_embeddings();

        let query = vec![1.0_f32, 0.0, 0.0];
        let results = store.semantic_search(&query, 1);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].ticket.id.as_deref(), Some("j-auth"));
    }

    #[test]
    fn test_semantic_search_db_direction() {
        let store = test_store_with_embeddings();

        // Query similar to "db" direction
        let query = vec![0.0_f32, 1.0, 0.0];
        let results = store.semantic_search(&query, 10);

        assert_eq!(results[0].ticket.id.as_deref(), Some("j-db"));
    }

    #[test]
    fn test_semantic_search_empty_store() {
        let store = TicketStore::empty();

        let query = vec![1.0_f32, 0.0, 0.0];
        let results = store.semantic_search(&query, 10);

        assert!(results.is_empty());
    }

    #[test]
    fn test_semantic_search_zero_limit() {
        let store = test_store_with_embeddings();

        let query = vec![1.0_f32, 0.0, 0.0];
        let results = store.semantic_search(&query, 0);

        assert!(results.is_empty());
    }

    #[test]
    fn test_semantic_search_similarity_range() {
        let store = test_store_with_embeddings();

        let query = vec![1.0_f32, 0.0, 0.0];
        let results = store.semantic_search(&query, 10);

        for result in &results {
            // Cosine similarity should be between -1 and 1
            assert!(result.similarity >= -1.0);
            assert!(result.similarity <= 1.0);
        }
    }

    #[test]
    fn test_semantic_search_sorted_descending() {
        let store = test_store_with_embeddings();

        let query = vec![0.5_f32, 0.5, 0.5];
        let results = store.semantic_search(&query, 10);

        for window in results.windows(2) {
            assert!(window[0].similarity >= window[1].similarity);
        }
    }

    #[test]
    fn test_semantic_search_skips_tickets_without_embeddings() {
        let store = test_store_with_embeddings();

        let query = vec![1.0_f32, 0.0, 0.0];
        let results = store.semantic_search(&query, 10);

        // j-noembedding should not appear in results
        for result in &results {
            assert_ne!(result.ticket.id.as_deref(), Some("j-noembedding"));
        }
    }
}

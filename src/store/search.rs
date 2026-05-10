use std::cmp::Ordering;
use std::collections::BinaryHeap;

use super::TicketStore;
use crate::embedding::model::cosine_similarity;
use crate::objective::types::ObjectiveMetadata;
use crate::types::{EntityType, TicketMetadata};

/// Result of a semantic search, containing the matched ticket and its similarity score.
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// The matched ticket metadata.
    pub ticket: TicketMetadata,
    /// Cosine similarity score between the query embedding and the ticket embedding.
    pub similarity: f32,
}

/// Result of a unified semantic search across tickets and objectives.
#[derive(Debug, Clone)]
pub struct UnifiedSearchResult {
    /// The type of entity this result represents.
    pub entity_type: EntityType,
    /// The matched ticket metadata (present when entity_type is Ticket).
    pub ticket: Option<TicketMetadata>,
    /// The matched objective metadata (present when entity_type is Objective).
    pub objective: Option<ObjectiveMetadata>,
    /// Cosine similarity score.
    pub similarity: f32,
}

/// A scored candidate for top-K selection via a min-heap.
///
/// Wraps a ticket ID and similarity score, ordered by similarity ascending
/// so that `BinaryHeap` (a max-heap) surfaces the *smallest* score at the top.
/// This lets us efficiently evict the weakest candidate when the heap is full.
struct ScoredCandidate {
    ticket_id: String,
    similarity: f32,
}

impl PartialEq for ScoredCandidate {
    fn eq(&self, other: &Self) -> bool {
        self.similarity == other.similarity
    }
}

impl Eq for ScoredCandidate {}

impl PartialOrd for ScoredCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScoredCandidate {
    fn cmp(&self, other: &Self) -> Ordering {
        // Ascending order: smaller similarity = "greater" for the heap,
        // so BinaryHeap keeps the minimum-similarity item at the top.
        other
            .similarity
            .partial_cmp(&self.similarity)
            .unwrap_or(Ordering::Equal)
    }
}

impl TicketStore {
    /// Perform semantic search using brute-force cosine similarity with top-K selection.
    ///
    /// 1. Iterates all entries in the embeddings DashMap
    /// 2. Computes cosine similarity for each against `query_embedding`
    /// 3. Maintains a bounded min-heap of size `limit` for O(N log K) top-K selection
    /// 4. Sorts the final K results by similarity descending for presentation
    pub fn semantic_search(&self, query_embedding: &[f32], limit: usize) -> Vec<SearchResult> {
        if limit == 0 {
            return Vec::new();
        }

        // Snapshot embedding data (ID + similarity score) into a local Vec first,
        // so that all embeddings DashMap shard locks are released before we touch
        // the tickets DashMap. This prevents AB/BA deadlocks between the two maps
        // under concurrent access (e.g., watcher upserts + semantic search).
        //
        // While iterating, use a bounded min-heap of size `limit` to select the
        // top-K scored candidates in O(N log K) instead of sorting all N in O(N log N).
        let mut heap: BinaryHeap<ScoredCandidate> = BinaryHeap::with_capacity(limit + 1);

        for entry in self.embeddings().iter() {
            let similarity = cosine_similarity(query_embedding, entry.value());

            if heap.len() < limit {
                heap.push(ScoredCandidate {
                    ticket_id: entry.key().clone(),
                    similarity,
                });
            } else if let Some(min) = heap.peek()
                && similarity > min.similarity
            {
                heap.pop();
                heap.push(ScoredCandidate {
                    ticket_id: entry.key().clone(),
                    similarity,
                });
            }
        }

        // Drain the heap and look up ticket metadata without holding any embeddings guards.
        let mut results: Vec<SearchResult> = heap
            .into_iter()
            .filter_map(|candidate| {
                self.tickets()
                    .get(&candidate.ticket_id)
                    .map(|ticket_ref| SearchResult {
                        ticket: ticket_ref.value().clone(),
                        similarity: candidate.similarity,
                    })
            })
            .collect();

        // Sort the final K results by similarity descending for presentation.
        results.sort_by(|a, b| {
            b.similarity
                .partial_cmp(&a.similarity)
                .unwrap_or(Ordering::Equal)
        });

        results
    }

    /// Perform unified semantic search across both tickets and objectives.
    ///
    /// Unlike `semantic_search` which only returns ticket matches, this method
    /// searches the shared embeddings DashMap and resolves matches against both
    /// the tickets and objectives DashMaps.
    ///
    /// Doc embeddings (keyed with `doc:` prefix) are excluded — use `doc_search`
    /// for document-level search.
    pub fn unified_semantic_search(
        &self,
        query_embedding: &[f32],
        limit: usize,
    ) -> Vec<UnifiedSearchResult> {
        if limit == 0 {
            return Vec::new();
        }

        let mut heap: BinaryHeap<ScoredCandidate> = BinaryHeap::with_capacity(limit + 1);

        for entry in self.embeddings().iter() {
            // Skip doc embeddings — they have their own search
            if entry.key().starts_with("doc:") {
                continue;
            }

            let similarity = cosine_similarity(query_embedding, entry.value());

            if heap.len() < limit {
                heap.push(ScoredCandidate {
                    ticket_id: entry.key().clone(),
                    similarity,
                });
            } else if let Some(min) = heap.peek()
                && similarity > min.similarity
            {
                heap.pop();
                heap.push(ScoredCandidate {
                    ticket_id: entry.key().clone(),
                    similarity,
                });
            }
        }

        // Drain the heap and look up entity metadata in both tickets and objectives.
        let mut results: Vec<UnifiedSearchResult> = heap
            .into_iter()
            .filter_map(|candidate| {
                // Try ticket lookup first
                if let Some(ticket_ref) = self.tickets().get(&candidate.ticket_id) {
                    return Some(UnifiedSearchResult {
                        entity_type: EntityType::Ticket,
                        ticket: Some(ticket_ref.value().clone()),
                        objective: None,
                        similarity: candidate.similarity,
                    });
                }

                // Try objective lookup
                if let Some(objective_ref) = self.objectives().get(&candidate.ticket_id) {
                    return Some(UnifiedSearchResult {
                        entity_type: EntityType::Objective,
                        ticket: None,
                        objective: Some(objective_ref.value().clone()),
                        similarity: candidate.similarity,
                    });
                }

                None
            })
            .collect();

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
    use crate::objective::types::ObjectiveMetadata;
    use crate::store::TicketStore;
    use crate::types::{EntityType, ObjectiveId, TicketId, TicketMetadata, TicketStatus};

    /// Helper to create a store with tickets and mock embeddings.
    fn test_store_with_embeddings() -> TicketStore {
        let store = TicketStore::empty();

        // Ticket 1: "authentication" direction
        store.upsert_ticket(TicketMetadata {
            id: Some(TicketId::new_unchecked("j-auth")),
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
            id: Some(TicketId::new_unchecked("j-db")),
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
            id: Some(TicketId::new_unchecked("j-ui")),
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
            id: Some(TicketId::new_unchecked("j-noembedding")),
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

    /// Helper: naive full-sort approach for comparison with top-K.
    /// Collects all scores, sorts descending, and truncates.
    fn naive_sorted_search(store: &TicketStore, query: &[f32], limit: usize) -> Vec<(String, f32)> {
        use crate::embedding::model::cosine_similarity;

        // Phase 1: Snapshot embedding data into owned locals so all embeddings
        // DashMap shard locks are released before touching the tickets DashMap.
        let scored_candidates: Vec<(String, f32)> = store
            .embeddings()
            .iter()
            .map(|entry| {
                let ticket_id = entry.key().clone();
                let similarity = cosine_similarity(query, entry.value());
                (ticket_id, similarity)
            })
            .collect();

        // Phase 2: Filter against tickets DashMap now that embeddings guards
        // are released, preventing AB/BA lock-order inversion deadlocks.
        let mut scored: Vec<(String, f32)> = scored_candidates
            .into_iter()
            .filter(|(ticket_id, _)| store.tickets().contains_key(ticket_id.as_str()))
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);
        scored
    }

    #[test]
    fn test_topk_matches_naive_sort_full_results() {
        let store = test_store_with_embeddings();
        let query = vec![0.5_f32, 0.3, 0.2];

        // Request all results (limit >= N)
        let topk = store.semantic_search(&query, 10);
        let naive = naive_sorted_search(&store, &query, 10);

        assert_eq!(topk.len(), naive.len());
        for (topk_result, (naive_id, naive_sim)) in topk.iter().zip(naive.iter()) {
            assert_eq!(topk_result.ticket.id.as_deref(), Some(naive_id.as_str()));
            assert!(
                (topk_result.similarity - naive_sim).abs() < 1e-6,
                "similarity mismatch: topk={} naive={}",
                topk_result.similarity,
                naive_sim
            );
        }
    }

    #[test]
    fn test_topk_matches_naive_sort_limited() {
        let store = test_store_with_embeddings();
        let query = vec![0.4_f32, 0.4, 0.2];

        // Request fewer results than available
        let topk = store.semantic_search(&query, 2);
        let naive = naive_sorted_search(&store, &query, 2);

        assert_eq!(topk.len(), 2);
        assert_eq!(naive.len(), 2);
        for (topk_result, (naive_id, naive_sim)) in topk.iter().zip(naive.iter()) {
            assert_eq!(topk_result.ticket.id.as_deref(), Some(naive_id.as_str()));
            assert!(
                (topk_result.similarity - naive_sim).abs() < 1e-6,
                "similarity mismatch: topk={} naive={}",
                topk_result.similarity,
                naive_sim
            );
        }
    }

    #[test]
    fn test_topk_matches_naive_with_many_tickets() {
        // Create a larger store to exercise the heap eviction path.
        // Each ticket gets a unique embedding to guarantee distinct similarity scores.
        let store = TicketStore::empty();
        let count = 20;

        for i in 0..count {
            let id = format!("j-t{i:03}");
            store.upsert_ticket(TicketMetadata {
                id: Some(TicketId::new_unchecked(&id)),
                title: Some(format!("Ticket {i}")),
                status: Some(TicketStatus::New),
                ..Default::default()
            });
            // Embeddings that produce distinct cosine similarities with query [1,0,0,0]:
            // ticket i gets embedding [1 - i*0.05, i*0.05, 0, 0], giving decreasing
            // cosine similarity as i increases.
            let primary = 1.0 - (i as f32) * 0.05;
            let secondary = (i as f32) * 0.05;
            store
                .embeddings()
                .insert(id, vec![primary, secondary, 0.0, 0.0]);
        }

        let query = vec![1.0_f32, 0.0, 0.0, 0.0];
        let limit = 5;

        let topk = store.semantic_search(&query, limit);
        let naive = naive_sorted_search(&store, &query, limit);

        assert_eq!(topk.len(), limit);
        assert_eq!(naive.len(), limit);
        for (topk_result, (naive_id, naive_sim)) in topk.iter().zip(naive.iter()) {
            assert_eq!(topk_result.ticket.id.as_deref(), Some(naive_id.as_str()));
            assert!(
                (topk_result.similarity - naive_sim).abs() < 1e-6,
                "similarity mismatch for {}: topk={} naive={}",
                naive_id,
                topk_result.similarity,
                naive_sim
            );
        }
    }

    #[test]
    fn test_topk_limit_exceeds_count() {
        // When limit is larger than the number of embeddings, return all
        let store = test_store_with_embeddings();
        let query = vec![1.0_f32, 0.0, 0.0];

        let results = store.semantic_search(&query, 100);
        // Only 3 tickets have embeddings
        assert_eq!(results.len(), 3);
        // Should still be sorted descending
        for window in results.windows(2) {
            assert!(window[0].similarity >= window[1].similarity);
        }
    }

    #[test]
    fn test_topk_limit_one() {
        let store = test_store_with_embeddings();
        let query = vec![0.0_f32, 0.0, 1.0]; // "ui" direction

        let results = store.semantic_search(&query, 1);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].ticket.id.as_deref(), Some("j-ui"));
    }

    #[test]
    fn test_threshold_filtering_at_caller() {
        // Verify that threshold filtering (done by callers) still works
        // correctly with the top-K results.
        let store = test_store_with_embeddings();
        let query = vec![1.0_f32, 0.0, 0.0]; // "auth" direction

        let results = store.semantic_search(&query, 10);
        // Apply a threshold filter as the callers do
        let threshold = 0.5;
        let filtered: Vec<_> = results
            .into_iter()
            .filter(|r| r.similarity >= threshold)
            .collect();

        // Only j-auth has a high similarity to [1,0,0]; others should be below 0.5
        assert!(!filtered.is_empty());
        for result in &filtered {
            assert!(result.similarity >= threshold);
        }
    }

    #[test]
    fn test_unified_semantic_search_includes_objectives() {
        let store = TicketStore::empty();

        // Add a ticket
        store.upsert_ticket(TicketMetadata {
            id: Some(TicketId::new_unchecked("j-auth")),
            title: Some("Auth ticket".to_string()),
            status: Some(TicketStatus::New),
            ..Default::default()
        });
        store
            .embeddings()
            .insert("j-auth".to_string(), vec![0.9, 0.1, 0.0]);

        // Add an objective
        store.upsert_objective(ObjectiveMetadata {
            id: Some(ObjectiveId::new_unchecked("objv-goal")),
            title: Some("Security goal".to_string()),
            ..Default::default()
        });
        store
            .embeddings()
            .insert("objv-goal".to_string(), vec![0.8, 0.2, 0.0]);

        // Unified search should find both
        let query = vec![1.0_f32, 0.0, 0.0];
        let results = store.unified_semantic_search(&query, 10);

        assert_eq!(results.len(), 2);

        // Verify we got both entity types
        let has_ticket = results.iter().any(|r| r.entity_type == EntityType::Ticket);
        let has_objective = results
            .iter()
            .any(|r| r.entity_type == EntityType::Objective);
        assert!(has_ticket, "should find ticket result");
        assert!(has_objective, "should find objective result");

        // Regular semantic_search should only find the ticket
        let ticket_only = store.semantic_search(&query, 10);
        assert_eq!(ticket_only.len(), 1);
        assert_eq!(ticket_only[0].ticket.id.as_deref(), Some("j-auth"));
    }

    #[test]
    fn test_unified_semantic_search_excludes_doc_embeddings() {
        let store = TicketStore::empty();

        // Add a ticket with embedding
        store.upsert_ticket(TicketMetadata {
            id: Some(TicketId::new_unchecked("j-test")),
            title: Some("Test ticket".to_string()),
            status: Some(TicketStatus::New),
            ..Default::default()
        });
        store
            .embeddings()
            .insert("j-test".to_string(), vec![0.9, 0.1, 0.0]);

        // Add a doc embedding (should be excluded from unified search)
        store
            .embeddings()
            .insert("doc:test-doc".to_string(), vec![0.95, 0.05, 0.0]);

        let query = vec![1.0_f32, 0.0, 0.0];
        let results = store.unified_semantic_search(&query, 10);

        // Should only find the ticket, not the doc
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entity_type, EntityType::Ticket);
    }
}

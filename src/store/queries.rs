use std::collections::HashMap;

use crate::plan::types::PlanMetadata;
use crate::store::TicketStore;
use crate::tui::search::{parse_priority_filter, strip_priority_shorthand};
use crate::types::{TicketMetadata, TicketSize};

impl TicketStore {
    /// Get all tickets as a Vec, sorted by id for deterministic ordering.
    pub fn get_all_tickets(&self) -> Vec<TicketMetadata> {
        let mut results: Vec<TicketMetadata> =
            self.tickets().iter().map(|r| r.value().clone()).collect();
        results.sort_by(|a, b| {
            a.id.as_deref()
                .unwrap_or("")
                .cmp(b.id.as_deref().unwrap_or(""))
        });
        results
    }

    /// Get a single ticket by exact ID.
    pub fn get_ticket(&self, id: &str) -> Option<TicketMetadata> {
        self.tickets().get(id).map(|r| r.value().clone())
    }

    /// Find tickets by partial ID substring match, returning matching IDs.
    pub fn find_by_partial_id(&self, partial_id: &str) -> Vec<String> {
        self.tickets()
            .iter()
            .filter(|r| r.key().contains(partial_id))
            .map(|r| r.key().clone())
            .collect()
    }

    /// Build a HashMap of ticket_id -> metadata.
    pub fn build_ticket_map(&self) -> HashMap<String, TicketMetadata> {
        self.tickets()
            .iter()
            .map(|r| (r.key().clone(), r.value().clone()))
            .collect()
    }

    /// Get count of children (tickets spawned from this ticket).
    pub fn get_children_count(&self, id: &str) -> usize {
        self.tickets()
            .iter()
            .filter(|r| r.value().spawned_from.as_deref() == Some(id))
            .count()
    }

    /// Get children counts for all tickets that have spawned children.
    pub fn get_all_children_counts(&self) -> HashMap<String, usize> {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for entry in self.tickets().iter() {
            if let Some(parent_id) = &entry.value().spawned_from {
                *counts.entry(parent_id.clone()).or_insert(0) += 1;
            }
        }
        counts
    }

    /// Search tickets by text query with optional priority filter.
    ///
    /// Uses case-insensitive substring matching on: ticket_id, title, body, ticket_type.
    /// Supports priority shorthand (e.g., "p0 fix" filters to priority 0 and searches "fix").
    pub fn search_tickets(&self, query: &str) -> Vec<TicketMetadata> {
        let priority_filter = parse_priority_filter(query);
        let text_query = strip_priority_shorthand(query).to_lowercase();

        let mut results: Vec<TicketMetadata> = self
            .tickets()
            .iter()
            .filter(|r| {
                let ticket = r.value();

                // Apply priority filter if present
                if let Some(priority_num) = priority_filter {
                    let ticket_priority = ticket.priority.map(|p| p.as_num()).unwrap_or(2); // default P2
                    if ticket_priority != priority_num {
                        return false;
                    }
                }

                // If no text query remains after stripping priority, match all
                if text_query.is_empty() {
                    return true;
                }

                // Case-insensitive substring matching on relevant fields
                let id_match = r.key().to_lowercase().contains(&text_query);

                let title_match = ticket
                    .title
                    .as_ref()
                    .is_some_and(|t| t.to_lowercase().contains(&text_query));

                let body_match = ticket
                    .body
                    .as_ref()
                    .is_some_and(|b| b.to_lowercase().contains(&text_query));

                let type_match = ticket
                    .ticket_type
                    .as_ref()
                    .is_some_and(|t| t.to_string().to_lowercase().contains(&text_query));

                id_match || title_match || body_match || type_match
            })
            .map(|r| r.value().clone())
            .collect();
        results.sort_by(|a, b| {
            a.id.as_deref()
                .unwrap_or("")
                .cmp(b.id.as_deref().unwrap_or(""))
        });
        results
    }

    /// Get tickets filtered by size, sorted by id for deterministic ordering.
    pub fn get_tickets_by_size(&self, sizes: &[TicketSize]) -> Vec<TicketMetadata> {
        let mut results: Vec<TicketMetadata> = self
            .tickets()
            .iter()
            .filter(|r| r.value().size.as_ref().is_some_and(|s| sizes.contains(s)))
            .map(|r| r.value().clone())
            .collect();
        results.sort_by(|a, b| {
            a.id.as_deref()
                .unwrap_or("")
                .cmp(b.id.as_deref().unwrap_or(""))
        });
        results
    }

    /// Get all plans as a Vec, sorted by id for deterministic ordering.
    pub fn get_all_plans(&self) -> Vec<PlanMetadata> {
        let mut results: Vec<PlanMetadata> =
            self.plans().iter().map(|r| r.value().clone()).collect();
        results.sort_by(|a, b| {
            a.id.as_deref()
                .unwrap_or("")
                .cmp(b.id.as_deref().unwrap_or(""))
        });
        results
    }

    /// Get a single plan by exact ID.
    pub fn get_plan(&self, id: &str) -> Option<PlanMetadata> {
        self.plans().get(id).map(|r| r.value().clone())
    }

    /// Find plans by partial ID substring match, returning matching IDs.
    pub fn find_plan_by_partial_id(&self, partial_id: &str) -> Vec<String> {
        self.plans()
            .iter()
            .filter(|r| r.key().contains(partial_id))
            .map(|r| r.key().clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::plan::types::{PlanMetadata, PlanSection};
    use crate::store::TicketStore;
    use crate::types::{TicketMetadata, TicketPriority, TicketSize, TicketStatus, TicketType};

    /// Helper to create a test store with some tickets pre-loaded.
    fn test_store() -> TicketStore {
        let store = TicketStore::empty();

        store.upsert_ticket(TicketMetadata {
            id: Some("j-a1b2".to_string()),
            title: Some("Implement cache initialization".to_string()),
            status: Some(TicketStatus::New),
            ticket_type: Some(TicketType::Task),
            priority: Some(TicketPriority::P0),
            size: Some(TicketSize::Medium),
            body: Some("Set up the cache module".to_string()),
            spawned_from: None,
            ..Default::default()
        });

        store.upsert_ticket(TicketMetadata {
            id: Some("j-c3d4".to_string()),
            title: Some("Fix login bug".to_string()),
            status: Some(TicketStatus::InProgress),
            ticket_type: Some(TicketType::Bug),
            priority: Some(TicketPriority::P1),
            size: Some(TicketSize::Small),
            body: Some("Users cannot log in".to_string()),
            spawned_from: Some("j-a1b2".to_string()),
            ..Default::default()
        });

        store.upsert_ticket(TicketMetadata {
            id: Some("j-e5f6".to_string()),
            title: Some("Add feature flags".to_string()),
            status: Some(TicketStatus::Complete),
            ticket_type: Some(TicketType::Feature),
            priority: Some(TicketPriority::P2),
            size: Some(TicketSize::Large),
            spawned_from: Some("j-a1b2".to_string()),
            ..Default::default()
        });

        store.upsert_ticket(TicketMetadata {
            id: Some("j-g7h8".to_string()),
            title: Some("Refactor database layer".to_string()),
            status: Some(TicketStatus::New),
            ticket_type: Some(TicketType::Chore),
            priority: Some(TicketPriority::P3),
            size: Some(TicketSize::XLarge),
            ..Default::default()
        });

        store
    }

    /// Helper to create a test store with plans.
    fn test_store_with_plans() -> TicketStore {
        let store = test_store();

        store.upsert_plan(PlanMetadata {
            id: Some("plan-a1b2".to_string()),
            title: Some("Cache Implementation".to_string()),
            sections: vec![PlanSection::Tickets(vec![
                "j-a1b2".to_string(),
                "j-c3d4".to_string(),
            ])],
            ..Default::default()
        });

        store.upsert_plan(PlanMetadata {
            id: Some("plan-c3d4".to_string()),
            title: Some("Feature Rollout".to_string()),
            sections: vec![PlanSection::Tickets(vec!["j-e5f6".to_string()])],
            ..Default::default()
        });

        store
    }

    #[test]
    fn test_get_all_tickets() {
        let store = test_store();
        let tickets = store.get_all_tickets();
        assert_eq!(tickets.len(), 4);
    }

    #[test]
    fn test_get_ticket_existing() {
        let store = test_store();
        let ticket = store.get_ticket("j-a1b2");
        assert!(ticket.is_some());
        assert_eq!(
            ticket.unwrap().title.as_deref(),
            Some("Implement cache initialization")
        );
    }

    #[test]
    fn test_get_ticket_nonexistent() {
        let store = test_store();
        let ticket = store.get_ticket("j-nonexistent");
        assert!(ticket.is_none());
    }

    #[test]
    fn test_find_by_partial_id() {
        let store = test_store();

        // Prefix match
        let matches = store.find_by_partial_id("j-a1");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], "j-a1b2");

        // Multiple matches via common prefix
        let matches = store.find_by_partial_id("j-");
        assert_eq!(matches.len(), 4);

        // No matches
        let matches = store.find_by_partial_id("z-");
        assert!(matches.is_empty());

        // Substring match (non-prefix) — matches "j-a1b2" via suffix
        let matches = store.find_by_partial_id("b2");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], "j-a1b2");

        // Substring match in the middle — matches "j-a1b2" and "j-c3d4" (both contain "3" or "1")
        let matches = store.find_by_partial_id("1b");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], "j-a1b2");

        // Substring matching multiple tickets — "3" appears in "j-c3d4"
        let matches = store.find_by_partial_id("3");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], "j-c3d4");
    }

    #[test]
    fn test_build_ticket_map() {
        let store = test_store();
        let map = store.build_ticket_map();
        assert_eq!(map.len(), 4);
        assert!(map.contains_key("j-a1b2"));
        assert!(map.contains_key("j-c3d4"));
    }

    #[test]
    fn test_get_children_count() {
        let store = test_store();

        // j-a1b2 has 2 children (j-c3d4 and j-e5f6 are spawned from it)
        assert_eq!(store.get_children_count("j-a1b2"), 2);

        // j-c3d4 has no children
        assert_eq!(store.get_children_count("j-c3d4"), 0);

        // Nonexistent ticket has 0 children
        assert_eq!(store.get_children_count("j-nonexistent"), 0);
    }

    #[test]
    fn test_get_all_children_counts() {
        let store = test_store();
        let counts = store.get_all_children_counts();

        assert_eq!(counts.get("j-a1b2"), Some(&2));
        assert!(!counts.contains_key("j-c3d4")); // No children
        assert!(!counts.contains_key("j-g7h8")); // No children
    }

    #[test]
    fn test_search_tickets_by_title() {
        let store = test_store();
        let results = store.search_tickets("cache");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id.as_deref(), Some("j-a1b2"));
    }

    #[test]
    fn test_search_tickets_by_id() {
        let store = test_store();
        let results = store.search_tickets("j-a1b2");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id.as_deref(), Some("j-a1b2"));
    }

    #[test]
    fn test_search_tickets_by_body() {
        let store = test_store();
        let results = store.search_tickets("cannot log in");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id.as_deref(), Some("j-c3d4"));
    }

    #[test]
    fn test_search_tickets_by_type() {
        let store = test_store();
        let results = store.search_tickets("bug");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id.as_deref(), Some("j-c3d4"));
    }

    #[test]
    fn test_search_tickets_case_insensitive() {
        let store = test_store();
        let results = store.search_tickets("CACHE");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id.as_deref(), Some("j-a1b2"));
    }

    #[test]
    fn test_search_tickets_priority_filter() {
        let store = test_store();

        // p0 filter only
        let results = store.search_tickets("p0");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id.as_deref(), Some("j-a1b2"));

        // p1 filter only
        let results = store.search_tickets("p1");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id.as_deref(), Some("j-c3d4"));
    }

    #[test]
    fn test_search_tickets_priority_with_text() {
        let store = test_store();

        // p0 + text query
        let results = store.search_tickets("p0 cache");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id.as_deref(), Some("j-a1b2"));

        // p0 + text that doesn't match any p0 ticket
        let results = store.search_tickets("p0 login");
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_tickets_no_match() {
        let store = test_store();
        let results = store.search_tickets("zzz_nonexistent_zzz");
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_tickets_empty_query() {
        let store = test_store();
        let results = store.search_tickets("");
        assert_eq!(results.len(), 4);
    }

    #[test]
    fn test_get_tickets_by_size() {
        let store = test_store();

        let small = store.get_tickets_by_size(&[TicketSize::Small]);
        assert_eq!(small.len(), 1);
        assert_eq!(small[0].id.as_deref(), Some("j-c3d4"));

        let small_and_medium = store.get_tickets_by_size(&[TicketSize::Small, TicketSize::Medium]);
        assert_eq!(small_and_medium.len(), 2);

        let none = store.get_tickets_by_size(&[]);
        assert!(none.is_empty());
    }

    #[test]
    fn test_get_all_plans() {
        let store = test_store_with_plans();
        let plans = store.get_all_plans();
        assert_eq!(plans.len(), 2);
    }

    #[test]
    fn test_get_plan_existing() {
        let store = test_store_with_plans();
        let plan = store.get_plan("plan-a1b2");
        assert!(plan.is_some());
        assert_eq!(plan.unwrap().title.as_deref(), Some("Cache Implementation"));
    }

    #[test]
    fn test_get_plan_nonexistent() {
        let store = test_store_with_plans();
        let plan = store.get_plan("plan-nonexistent");
        assert!(plan.is_none());
    }

    #[test]
    fn test_find_plan_by_partial_id() {
        let store = test_store_with_plans();

        // Prefix match
        let matches = store.find_plan_by_partial_id("plan-a");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], "plan-a1b2");

        // Multiple matches via common prefix
        let matches = store.find_plan_by_partial_id("plan-");
        assert_eq!(matches.len(), 2);

        // No matches
        let matches = store.find_plan_by_partial_id("nonexistent");
        assert!(matches.is_empty());

        // Substring match (non-prefix) — matches "plan-a1b2" via suffix
        let matches = store.find_plan_by_partial_id("1b2");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], "plan-a1b2");

        // Substring match — "3d4" appears in "plan-c3d4"
        let matches = store.find_plan_by_partial_id("3d4");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], "plan-c3d4");
    }
}

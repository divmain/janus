use std::collections::HashMap;

use dashmap::mapref::multiple::RefMulti;

use super::TicketStore;
use crate::plan::types::PlanMetadata;
use crate::types::{TicketMetadata, TicketSize, TicketSummary};
use crate::utils::{parse_priority_filter, strip_priority_shorthand};

/// Case-insensitive substring match.
///
/// Uses `unicase` for correct Unicode case folding (handles Turkish i, German ß, etc.).
/// Note: This creates a folded string for the haystack, which is an allocation.
/// For allocation-free matching, use unicase::eq() for equality checks.
fn contains_case_insensitive(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let haystack_folded = unicase::UniCase::new(haystack).to_folded_case();
    let needle_folded = unicase::UniCase::new(needle).to_folded_case();
    haystack_folded.contains(&needle_folded)
}

/// Sort a slice by an optional string ID field, using `""` for `None`.
///
/// This eliminates the repeated `sort_by(|a, b| a.id.as_deref().unwrap_or("").cmp(...))` closure
/// that otherwise appears across many query methods.
fn sort_by_id<T>(results: &mut [T], id_fn: impl Fn(&T) -> Option<&str>) {
    results.sort_by(|a, b| id_fn(a).unwrap_or("").cmp(id_fn(b).unwrap_or("")));
}

/// Check whether a ticket matches a text search query with optional priority filter.
///
/// This is the shared predicate used by both `search_tickets` and
/// `search_ticket_summaries` to avoid duplicating ~25 lines of filter logic.
fn matches_search_query(
    key: &str,
    ticket: &TicketMetadata,
    text_query: &str,
    priority_filter: Option<u8>,
) -> bool {
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

    // Case-insensitive substring matching (allocation-free via unicase)
    let id_match = contains_case_insensitive(key, text_query);

    let title_match = ticket
        .title
        .as_ref()
        .is_some_and(|t| contains_case_insensitive(t, text_query));

    let body_match = ticket
        .body
        .as_ref()
        .is_some_and(|b| contains_case_insensitive(b, text_query));

    let type_match = ticket
        .ticket_type
        .as_ref()
        .is_some_and(|t| contains_case_insensitive(&t.to_string(), text_query));

    id_match || title_match || body_match || type_match
}

impl TicketStore {
    /// Get all tickets as a Vec, sorted by id for deterministic ordering.
    pub fn get_all_tickets(&self) -> Vec<TicketMetadata> {
        let mut results: Vec<TicketMetadata> =
            self.tickets().iter().map(|r| r.value().clone()).collect();
        sort_by_id(&mut results, |t| t.id.as_deref());
        results
    }

    /// Get a single ticket by exact ID.
    pub fn get_ticket(&self, id: &str) -> Option<TicketMetadata> {
        self.tickets().get(id).map(|r| r.value().clone())
    }

    /// Find tickets by partial ID substring match, returning matching IDs.
    pub fn find_by_partial_id(&self, partial_id: &str) -> Vec<String> {
        let mut matches: Vec<String> = self
            .tickets()
            .iter()
            .filter(|r| r.key().contains(partial_id))
            .map(|r| r.key().clone())
            .collect();
        matches.sort();
        matches
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
                *counts.entry(parent_id.to_string()).or_insert(0) += 1;
            }
        }
        counts
    }

    /// Return DashMap references for tickets matching a text search query.
    ///
    /// This is the shared filter used by both `search_tickets` and
    /// `search_ticket_summaries`; callers choose the final `.map()` to
    /// produce either `TicketMetadata` or `TicketSummary`.
    fn filter_tickets_by_query(&self, query: &str) -> Vec<RefMulti<'_, String, TicketMetadata>> {
        let priority_filter = parse_priority_filter(query);
        let text_query = strip_priority_shorthand(query);

        self.tickets()
            .iter()
            .filter(|r| matches_search_query(r.key(), r.value(), &text_query, priority_filter))
            .collect()
    }

    /// Return DashMap references for tickets matching a size filter.
    ///
    /// Shared filter used by both `get_tickets_by_size` and
    /// `get_ticket_summaries_by_size`.
    fn filter_tickets_by_size(
        &self,
        sizes: &[TicketSize],
    ) -> Vec<RefMulti<'_, String, TicketMetadata>> {
        self.tickets()
            .iter()
            .filter(|r| r.value().size.as_ref().is_some_and(|s| sizes.contains(s)))
            .collect()
    }

    /// Search tickets by text query with optional priority filter.
    ///
    /// Uses case-insensitive substring matching on: ticket_id, title, body, ticket_type.
    /// Supports priority shorthand (e.g., "p0 fix" filters to priority 0 and searches "fix").
    pub fn search_tickets(&self, query: &str) -> Vec<TicketMetadata> {
        let mut results: Vec<TicketMetadata> = self
            .filter_tickets_by_query(query)
            .into_iter()
            .map(|r| r.value().clone())
            .collect();
        sort_by_id(&mut results, |t| t.id.as_deref());
        results
    }

    /// Get tickets filtered by size, sorted by id for deterministic ordering.
    pub fn get_tickets_by_size(&self, sizes: &[TicketSize]) -> Vec<TicketMetadata> {
        let mut results: Vec<TicketMetadata> = self
            .filter_tickets_by_size(sizes)
            .into_iter()
            .map(|r| r.value().clone())
            .collect();
        sort_by_id(&mut results, |t| t.id.as_deref());
        results
    }

    // -------------------------------------------------------------------------
    // Lightweight summary APIs — avoid cloning the full body/file_path
    // -------------------------------------------------------------------------

    /// Get all tickets as lightweight summaries, sorted by id.
    ///
    /// This is cheaper than `get_all_tickets()` because it skips cloning
    /// the potentially large `body` and `file_path` fields.
    pub fn get_all_ticket_summaries(&self) -> Vec<TicketSummary> {
        let mut results: Vec<TicketSummary> = self
            .tickets()
            .iter()
            .map(|r| TicketSummary::from(r.value()))
            .collect();
        sort_by_id(&mut results, |t| t.id.as_deref());
        results
    }

    /// Search tickets by text query and return lightweight summaries.
    ///
    /// Uses case-insensitive substring matching on: ticket_id, title, body, ticket_type.
    /// Supports priority shorthand (e.g., "p0 fix" filters to priority 0 and searches "fix").
    ///
    /// Unlike `search_tickets`, this avoids cloning the full body.
    pub fn search_ticket_summaries(&self, query: &str) -> Vec<TicketSummary> {
        let mut results: Vec<TicketSummary> = self
            .filter_tickets_by_query(query)
            .into_iter()
            .map(|r| TicketSummary::from(r.value()))
            .collect();
        sort_by_id(&mut results, |t| t.id.as_deref());
        results
    }

    /// Get ticket summaries filtered by size, sorted by id.
    pub fn get_ticket_summaries_by_size(&self, sizes: &[TicketSize]) -> Vec<TicketSummary> {
        let mut results: Vec<TicketSummary> = self
            .filter_tickets_by_size(sizes)
            .into_iter()
            .map(|r| TicketSummary::from(r.value()))
            .collect();
        sort_by_id(&mut results, |t| t.id.as_deref());
        results
    }

    /// Get all ticket IDs, sorted for deterministic ordering.
    ///
    /// This is the cheapest query — only clones the ID strings.
    pub fn get_all_ticket_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.tickets().iter().map(|r| r.key().clone()).collect();
        ids.sort();
        ids
    }

    /// Get all plans as a Vec, sorted by id for deterministic ordering.
    pub fn get_all_plans(&self) -> Vec<PlanMetadata> {
        let mut results: Vec<PlanMetadata> =
            self.plans().iter().map(|r| r.value().clone()).collect();
        sort_by_id(&mut results, |p| p.id.as_deref());
        results
    }

    /// Get a single plan by exact ID.
    pub fn get_plan(&self, id: &str) -> Option<PlanMetadata> {
        self.plans().get(id).map(|r| r.value().clone())
    }

    /// Find plans by partial ID substring match, returning matching IDs.
    pub fn find_plan_by_partial_id(&self, partial_id: &str) -> Vec<String> {
        let mut matches: Vec<String> = self
            .plans()
            .iter()
            .filter(|r| r.key().contains(partial_id))
            .map(|r| r.key().clone())
            .collect();
        matches.sort();
        matches
    }
}

#[cfg(test)]
mod tests {
    use crate::plan::types::{PlanMetadata, PlanSection, TicketsSection};
    use crate::store::TicketStore;
    use crate::types::{
        PlanId, TicketData, TicketId, TicketMetadata, TicketPriority, TicketSize, TicketStatus,
        TicketType,
    };

    /// Helper to create a test store with some tickets pre-loaded.
    fn test_store() -> TicketStore {
        let store = TicketStore::empty();

        store.upsert_ticket(TicketMetadata {
            id: Some(TicketId::new_unchecked("j-a1b2")),
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
            id: Some(TicketId::new_unchecked("j-c3d4")),
            title: Some("Fix login bug".to_string()),
            status: Some(TicketStatus::InProgress),
            ticket_type: Some(TicketType::Bug),
            priority: Some(TicketPriority::P1),
            size: Some(TicketSize::Small),
            body: Some("Users cannot log in".to_string()),
            spawned_from: Some(TicketId::new_unchecked("j-a1b2")),
            ..Default::default()
        });

        store.upsert_ticket(TicketMetadata {
            id: Some(TicketId::new_unchecked("j-e5f6")),
            title: Some("Add feature flags".to_string()),
            status: Some(TicketStatus::Complete),
            ticket_type: Some(TicketType::Feature),
            priority: Some(TicketPriority::P2),
            size: Some(TicketSize::Large),
            spawned_from: Some(TicketId::new_unchecked("j-a1b2")),
            ..Default::default()
        });

        store.upsert_ticket(TicketMetadata {
            id: Some(TicketId::new_unchecked("j-g7h8")),
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
            id: Some(PlanId::new_unchecked("plan-a1b2")),
            title: Some("Cache Implementation".to_string()),
            sections: vec![PlanSection::Tickets(TicketsSection::new(vec![
                "j-a1b2".to_string(),
                "j-c3d4".to_string(),
            ]))],
            ..Default::default()
        });

        store.upsert_plan(PlanMetadata {
            id: Some(PlanId::new_unchecked("plan-c3d4")),
            title: Some("Feature Rollout".to_string()),
            sections: vec![PlanSection::Tickets(TicketsSection::new(vec![
                "j-e5f6".to_string(),
            ]))],
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
    fn test_get_tickets_by_size_from_parsed_content() {
        use crate::store::test_helpers::make_ticket_content;
        use crate::ticket::parse_ticket;

        let store = TicketStore::empty();

        // Parse tickets from make_ticket_content (which includes size: medium)
        let content1 = make_ticket_content("j-t1t1", "Ticket One");
        let meta1 = parse_ticket(&content1).expect("should parse ticket");
        store.upsert_ticket(meta1);

        let content2 = make_ticket_content("j-t2t2", "Ticket Two");
        let meta2 = parse_ticket(&content2).expect("should parse ticket");
        store.upsert_ticket(meta2);

        // Both tickets should be medium size from make_ticket_content
        let medium = store.get_tickets_by_size(&[TicketSize::Medium]);
        assert_eq!(medium.len(), 2);

        // No tickets should match small
        let small = store.get_tickets_by_size(&[TicketSize::Small]);
        assert!(small.is_empty());

        // Verify ticket metadata has size populated
        let ticket = store.get_ticket("j-t1t1").unwrap();
        assert_eq!(ticket.size, Some(TicketSize::Medium));
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

    // -------------------------------------------------------------------------
    // Lightweight summary API tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_get_all_ticket_summaries() {
        let store = test_store();
        let summaries = store.get_all_ticket_summaries();
        assert_eq!(summaries.len(), 4);

        // Verify summaries are sorted by ID
        let ids: Vec<_> = summaries.iter().map(|s| s.id.as_deref().unwrap()).collect();
        assert_eq!(ids, vec!["j-a1b2", "j-c3d4", "j-e5f6", "j-g7h8"]);

        // Verify summary fields are populated correctly
        let first = &summaries[0];
        assert_eq!(first.id.as_deref(), Some("j-a1b2"));
        assert_eq!(
            first.title.as_deref(),
            Some("Implement cache initialization")
        );
        assert_eq!(first.status, Some(TicketStatus::New));
        assert_eq!(first.ticket_type, Some(TicketType::Task));
        assert_eq!(first.priority, Some(TicketPriority::P0));
        assert_eq!(first.size, Some(TicketSize::Medium));
    }

    #[test]
    fn test_get_all_ticket_summaries_preserves_spawned_from() {
        let store = test_store();
        let summaries = store.get_all_ticket_summaries();

        let child = summaries
            .iter()
            .find(|s| s.id.as_deref() == Some("j-c3d4"))
            .unwrap();
        assert_eq!(child.spawned_from.as_deref(), Some("j-a1b2"));

        let root = summaries
            .iter()
            .find(|s| s.id.as_deref() == Some("j-g7h8"))
            .unwrap();
        assert!(root.spawned_from.is_none());
    }

    #[test]
    fn test_search_ticket_summaries_by_title() {
        let store = test_store();
        let results = store.search_ticket_summaries("cache");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id.as_deref(), Some("j-a1b2"));
    }

    #[test]
    fn test_search_ticket_summaries_by_id() {
        let store = test_store();
        let results = store.search_ticket_summaries("j-a1b2");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id.as_deref(), Some("j-a1b2"));
    }

    #[test]
    fn test_search_ticket_summaries_by_body() {
        let store = test_store();
        let results = store.search_ticket_summaries("cannot log in");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id.as_deref(), Some("j-c3d4"));
    }

    #[test]
    fn test_search_ticket_summaries_by_type() {
        let store = test_store();
        let results = store.search_ticket_summaries("bug");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id.as_deref(), Some("j-c3d4"));
    }

    #[test]
    fn test_search_ticket_summaries_case_insensitive() {
        let store = test_store();
        let results = store.search_ticket_summaries("CACHE");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id.as_deref(), Some("j-a1b2"));
    }

    #[test]
    fn test_search_ticket_summaries_priority_filter() {
        let store = test_store();

        let results = store.search_ticket_summaries("p0");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id.as_deref(), Some("j-a1b2"));

        let results = store.search_ticket_summaries("p1");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id.as_deref(), Some("j-c3d4"));
    }

    #[test]
    fn test_search_ticket_summaries_priority_with_text() {
        let store = test_store();

        let results = store.search_ticket_summaries("p0 cache");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id.as_deref(), Some("j-a1b2"));

        let results = store.search_ticket_summaries("p0 login");
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_ticket_summaries_no_match() {
        let store = test_store();
        let results = store.search_ticket_summaries("zzz_nonexistent_zzz");
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_ticket_summaries_empty_query() {
        let store = test_store();
        let results = store.search_ticket_summaries("");
        assert_eq!(results.len(), 4);
    }

    #[test]
    fn test_get_ticket_summaries_by_size() {
        let store = test_store();

        let small = store.get_ticket_summaries_by_size(&[TicketSize::Small]);
        assert_eq!(small.len(), 1);
        assert_eq!(small[0].id.as_deref(), Some("j-c3d4"));

        let small_and_medium =
            store.get_ticket_summaries_by_size(&[TicketSize::Small, TicketSize::Medium]);
        assert_eq!(small_and_medium.len(), 2);

        let none = store.get_ticket_summaries_by_size(&[]);
        assert!(none.is_empty());
    }

    #[test]
    fn test_get_all_ticket_ids() {
        let store = test_store();
        let ids = store.get_all_ticket_ids();
        assert_eq!(ids.len(), 4);
        assert_eq!(ids, vec!["j-a1b2", "j-c3d4", "j-e5f6", "j-g7h8"]);
    }

    #[test]
    fn test_get_all_ticket_ids_empty_store() {
        let store = TicketStore::empty();
        let ids = store.get_all_ticket_ids();
        assert!(ids.is_empty());
    }

    #[test]
    fn test_contains_case_insensitive() {
        use super::contains_case_insensitive;

        // Basic matching
        assert!(contains_case_insensitive("Hello World", "hello"));
        assert!(contains_case_insensitive("Hello World", "WORLD"));
        assert!(contains_case_insensitive("Hello World", "lo Wo"));

        // Empty needle always matches
        assert!(contains_case_insensitive("anything", ""));
        assert!(contains_case_insensitive("", ""));

        // Needle longer than haystack
        assert!(!contains_case_insensitive("hi", "hello"));

        // No match
        assert!(!contains_case_insensitive("Hello World", "xyz"));

        // Exact match
        assert!(contains_case_insensitive("test", "test"));
        assert!(contains_case_insensitive("test", "TEST"));

        // Unicode case folding
        assert!(contains_case_insensitive("Straße", "straße"));
    }

    #[test]
    fn test_summary_priority_num() {
        let store = test_store();
        let summaries = store.get_all_ticket_summaries();

        let p0 = summaries
            .iter()
            .find(|s| s.id.as_deref() == Some("j-a1b2"))
            .unwrap();
        assert_eq!(p0.priority_num(), 0);

        let p1 = summaries
            .iter()
            .find(|s| s.id.as_deref() == Some("j-c3d4"))
            .unwrap();
        assert_eq!(p1.priority_num(), 1);
    }

    #[test]
    fn test_summary_compute_depth() {
        let store = test_store();
        let summaries = store.get_all_ticket_summaries();

        // Root ticket (no spawned_from) should have depth 0
        let root = summaries
            .iter()
            .find(|s| s.id.as_deref() == Some("j-a1b2"))
            .unwrap();
        assert_eq!(root.compute_depth(), 0);

        // Child ticket (has spawned_from but no explicit depth) should have depth 1
        let child = summaries
            .iter()
            .find(|s| s.id.as_deref() == Some("j-c3d4"))
            .unwrap();
        assert_eq!(child.compute_depth(), 1);
    }
}

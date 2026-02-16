use std::path::{Path, PathBuf};
use std::sync::Arc;

use dashmap::DashMap;
use tokio::fs as tokio_fs;
use tokio::sync::OnceCell;

use crate::error::Result;
use crate::plan::parser::parse_plan_content;
use crate::plan::types::PlanMetadata;
use crate::ticket::parse_ticket;
use crate::types::{TicketMetadata, plans_dir, tickets_items_dir};

/// A warning that occurred during store initialization.
#[derive(Debug, Clone)]
pub struct InitWarning {
    /// The file path associated with the warning (if applicable)
    pub file_path: Option<PathBuf>,
    /// The warning message
    pub message: String,
    /// The type of entity (ticket or plan)
    pub entity_type: String,
}

/// Collection of warnings captured during initialization.
#[derive(Debug, Clone, Default)]
pub struct InitWarnings {
    warnings: Arc<std::sync::Mutex<Vec<InitWarning>>>,
}

impl InitWarnings {
    /// Create a new empty warning collection.
    fn new() -> Self {
        Self {
            warnings: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// Add a warning to the collection.
    fn add(&self, warning: InitWarning) {
        if let Ok(mut guard) = self.warnings.lock() {
            guard.push(warning);
        }
    }

    /// Get all warnings as a vector.
    pub fn get_all(&self) -> Vec<InitWarning> {
        self.warnings
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    /// Get the count of warnings.
    pub fn count(&self) -> usize {
        self.warnings.lock().map(|g| g.len()).unwrap_or(0)
    }

    /// Check if there are any warnings.
    pub fn is_empty(&self) -> bool {
        self.count() == 0
    }

    /// Get ticket warnings only.
    pub fn ticket_warnings(&self) -> Vec<InitWarning> {
        self.get_all()
            .into_iter()
            .filter(|w| w.entity_type == "ticket")
            .collect()
    }

    /// Get plan warnings only.
    pub fn plan_warnings(&self) -> Vec<InitWarning> {
        self.get_all()
            .into_iter()
            .filter(|w| w.entity_type == "plan")
            .collect()
    }
}

/// Trait for entity metadata that can be loaded from files.
///
/// This trait abstracts over `TicketMetadata` and `PlanMetadata` to enable
/// generic file loading functionality without code duplication.
pub trait EntityMetadata: Send + 'static {
    /// Get the entity ID if set.
    fn id(&self) -> Option<&str>;
    /// Set the entity ID.
    fn set_id(&mut self, id: String);
    /// Get the file path if set.
    fn file_path(&self) -> Option<&PathBuf>;
    /// Set the file path.
    fn set_file_path(&mut self, path: PathBuf);
}

impl EntityMetadata for TicketMetadata {
    fn id(&self) -> Option<&str> {
        self.id.as_ref().map(|id| id.as_ref())
    }
    fn set_id(&mut self, id: String) {
        self.id = Some(crate::types::TicketId::new_unchecked(id));
    }
    fn file_path(&self) -> Option<&PathBuf> {
        self.file_path.as_ref()
    }
    fn set_file_path(&mut self, path: PathBuf) {
        self.file_path = Some(path);
    }
}

impl EntityMetadata for PlanMetadata {
    fn id(&self) -> Option<&str> {
        self.id.as_ref().map(|id| id.as_ref())
    }
    fn set_id(&mut self, id: String) {
        self.id = Some(crate::types::PlanId::new_unchecked(id));
    }
    fn file_path(&self) -> Option<&PathBuf> {
        self.file_path.as_ref()
    }
    fn set_file_path(&mut self, path: PathBuf) {
        self.file_path = Some(path);
    }
}

pub mod embeddings;
pub mod queries;
pub mod search;
pub mod watcher;

pub use watcher::{StoreEvent, mark_recently_edited, start_watching, subscribe_to_changes};

/// In-memory store for ticket and plan metadata with concurrent access.
///
/// The store holds all ticket and plan metadata in `DashMap` structures,
/// allowing lock-free concurrent reads and fine-grained locking for writes.
/// It also manages embedding vectors for semantic search.
pub struct TicketStore {
    tickets: DashMap<String, TicketMetadata>,
    plans: DashMap<String, PlanMetadata>,
    embeddings: DashMap<String, Vec<f32>>,
    /// Warnings captured during initialization
    init_warnings: InitWarnings,
}

/// Global singleton for the ticket store.
static STORE: OnceCell<TicketStore> = OnceCell::const_new();

/// Get or initialize the global ticket store singleton.
///
/// On first call, reads all tickets and plans from disk to populate the store.
/// Also ensures all tickets have embeddings generated (blocking call).
/// Subsequent calls return the existing store without re-reading.
/// If initialization fails, the error is propagated and the `OnceCell` remains
/// unset, allowing subsequent calls to retry.
///
/// Set `JANUS_SKIP_EMBEDDINGS=1` to skip eager embedding generation (useful for
/// tests and environments where semantic search is not needed).
pub async fn get_or_init_store() -> Result<&'static TicketStore> {
    STORE
        .get_or_try_init(|| async {
            // Step 1: Initialize store (loads tickets, plans, existing embeddings)
            let store = TicketStore::init().await?;

            // Step 2: Ensure all tickets have embeddings (unless skipped)
            // JANUS_SKIP_EMBEDDINGS=1 disables this for tests and environments
            // where semantic search is not needed.
            let skip = std::env::var("JANUS_SKIP_EMBEDDINGS")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);

            if !skip {
                match store.ensure_all_embeddings().await {
                    Ok((generated, total)) => {
                        if generated > 0 {
                            // User-facing message for startup
                            eprintln!("Generated embeddings for {generated}/{total} tickets");
                        }
                    }
                    Err(e) => {
                        // Log embedding failures for production visibility
                        tracing::warn!("Failed to generate embeddings: {e}");
                    }
                }
            }

            Ok(store)
        })
        .await
}

/// Get the store if it has been initialized, otherwise return None.
/// This is a synchronous alternative to `get_or_init_store()` for cases
/// where async is not available.
pub fn get_store() -> Option<&'static TicketStore> {
    STORE.get()
}

impl TicketStore {
    /// Create an empty store with no tickets or plans.
    pub fn empty() -> Self {
        TicketStore {
            tickets: DashMap::new(),
            plans: DashMap::new(),
            embeddings: DashMap::new(),
            init_warnings: InitWarnings::new(),
        }
    }

    /// Initialize the store by reading all tickets and plans from disk.
    ///
    /// Scans `.janus/items/` for ticket files and `.janus/plans/` for plan files,
    /// parsing each and populating the internal DashMaps. Files that fail to parse
    /// are logged as warnings but do not prevent initialization.
    pub async fn init() -> Result<Self> {
        let store = Self::empty();

        // Load tickets
        let items_dir = tickets_items_dir();
        if tokio_fs::try_exists(&items_dir).await.unwrap_or(false) {
            store.load_tickets_from_dir(&items_dir).await;
        }

        // Load plans
        let p_dir = plans_dir();
        if tokio_fs::try_exists(&p_dir).await.unwrap_or(false) {
            store.load_plans_from_dir(&p_dir).await;
        }

        // Load embeddings (requires tickets to be loaded first)
        if let Err(e) = store.load_embeddings() {
            tracing::warn!("Failed to load embeddings: {e}");
        }

        Ok(store)
    }

    /// Generic async function to load entities from a directory into the store.
    async fn load_entities_from_dir<T, F>(
        &self,
        dir: &Path,
        entity_name: &str,
        parser: F,
        mut insert: impl FnMut(T),
    ) where
        T: EntityMetadata,
        F: Fn(&str) -> crate::error::Result<T>,
    {
        let mut entries = match tokio_fs::read_dir(dir).await {
            Ok(entries) => entries,
            Err(e) => {
                self.init_warnings.add(InitWarning {
                    file_path: Some(dir.to_path_buf()),
                    message: format!("Failed to read {entity_name}s directory: {e}"),
                    entity_type: entity_name.to_string(),
                });
                return;
            }
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "md") {
                match tokio_fs::read_to_string(&path).await {
                    Ok(content) => match parser(&content) {
                        Ok(mut metadata) => {
                            if let Some(stem) = path.file_stem() {
                                let stem_str = stem.to_string_lossy();
                                match metadata.id() {
                                    Some(frontmatter_id) if frontmatter_id != stem_str.as_ref() => {
                                        self.init_warnings.add(InitWarning {
                                            file_path: Some(path.clone()),
                                            message: format!(
                                                "ID mismatch: frontmatter ID '{frontmatter_id}' doesn't match filename '{stem_str}'. Using filename as authoritative ID to ensure filesystem consistency."
                                            ),
                                            entity_type: entity_name.to_string(),
                                        });
                                        metadata.set_id(stem_str.to_string());
                                    }
                                    None => {
                                        metadata.set_id(stem_str.to_string());
                                    }
                                    Some(_) => {
                                        // IDs match, no action needed
                                    }
                                }
                            }
                            metadata.set_file_path(path);
                            if metadata.id().is_some() {
                                insert(metadata);
                            }
                        }
                        Err(e) => {
                            self.init_warnings.add(InitWarning {
                                file_path: Some(path.clone()),
                                message: format!("Failed to parse {entity_name} file: {e}"),
                                entity_type: entity_name.to_string(),
                            });
                        }
                    },
                    Err(e) => {
                        self.init_warnings.add(InitWarning {
                            file_path: Some(path.clone()),
                            message: format!("Failed to read {entity_name} file: {e}"),
                            entity_type: entity_name.to_string(),
                        });
                    }
                }
            }
        }
    }

    /// Load all ticket files from a directory into the store.
    async fn load_tickets_from_dir(&self, dir: &Path) {
        self.load_entities_from_dir(dir, "ticket", parse_ticket, |metadata: TicketMetadata| {
            if let Some(id) = metadata.id.clone() {
                self.tickets.insert(id.to_string(), metadata);
            }
        })
        .await;
    }

    /// Load all plan files from a directory into the store.
    async fn load_plans_from_dir(&self, dir: &Path) {
        self.load_entities_from_dir(dir, "plan", parse_plan_content, |metadata: PlanMetadata| {
            if let Some(id) = metadata.id.clone() {
                self.plans.insert(id.to_string(), metadata);
            }
        })
        .await;
    }

    /// Insert or update a ticket in the store.
    pub fn upsert_ticket(&self, metadata: TicketMetadata) {
        if let Some(id) = metadata.id.clone() {
            self.tickets.insert(id.to_string(), metadata);
        } else {
            self.init_warnings.add(InitWarning {
                file_path: metadata.file_path.clone(),
                message: "Skipping ticket upsert: missing ID in frontmatter".to_string(),
                entity_type: "ticket".to_string(),
            });
        }
    }

    /// Remove a ticket from the store and cascade delete references from other tickets.
    ///
    /// Also removes the corresponding embedding entry to prevent orphaned
    /// embeddings from inflating coverage counts.
    ///
    /// Performs cascade deletion to remove references to the deleted ticket
    /// from other tickets' deps and links arrays on disk to maintain referential integrity.
    pub fn remove_ticket_with_cascade(&self, id: &str) {
        // Get ticket info before removal for cascade cleanup
        let ticket_info = self
            .tickets
            .get(id)
            .map(|t| (t.file_path.clone(), t.id.as_ref().map(|id| id.to_string())));

        // Remove ticket and its embedding from the store
        self.tickets.remove(id);
        self.embeddings.remove(id);

        // Cascade deletion: remove references from other tickets' deps and links
        if let Some((_, Some(deleted_id))) = ticket_info {
            self.remove_ticket_references(&deleted_id);
        }
    }

    /// Remove references to a deleted ticket from all other tickets' deps and links.
    ///
    /// This maintains referential integrity by ensuring no dangling references
    /// remain when a ticket is deleted.
    fn remove_ticket_references(&self, deleted_id: &str) {
        // Collect tickets that need to be updated (those referencing the deleted ticket)
        let tickets_to_update: Vec<(String, PathBuf, bool, bool)> = self
            .tickets
            .iter()
            .filter_map(|entry| {
                let ticket_id = entry.key().clone();
                let metadata = entry.value();

                // Check if this ticket references the deleted ticket
                let has_in_deps = metadata.deps.iter().any(|dep| dep.as_ref() == deleted_id);
                let has_in_links = metadata
                    .links
                    .iter()
                    .any(|link| link.as_ref() == deleted_id);

                if has_in_deps || has_in_links {
                    metadata
                        .file_path
                        .clone()
                        .map(|path| (ticket_id, path, has_in_deps, has_in_links))
                } else {
                    None
                }
            })
            .collect();

        // Update each ticket that had references to the deleted ticket
        for (ticket_id, file_path, has_in_deps, has_in_links) in tickets_to_update {
            // Create a Ticket instance to perform the file operations
            if let Ok(ticket) = crate::ticket::Ticket::new(file_path.clone()) {
                // Remove from deps if present
                if has_in_deps {
                    if let Err(e) =
                        ticket.remove_from_array_field(crate::types::ArrayField::Deps, deleted_id)
                    {
                        tracing::warn!(
                            "Failed to remove {} from deps of ticket {}: {}",
                            deleted_id,
                            ticket_id,
                            e
                        );
                    }
                }

                // Remove from links if present
                if has_in_links {
                    if let Err(e) =
                        ticket.remove_from_array_field(crate::types::ArrayField::Links, deleted_id)
                    {
                        tracing::warn!(
                            "Failed to remove {} from links of ticket {}: {}",
                            deleted_id,
                            ticket_id,
                            e
                        );
                    }
                }

                // Update the in-memory store to reflect the changes
                if let Some(mut metadata) = self.tickets.get_mut(&ticket_id) {
                    metadata.deps.retain(|dep| dep.as_ref() != deleted_id);
                    metadata.links.retain(|link| link.as_ref() != deleted_id);
                }
            } else {
                tracing::warn!(
                    "Could not create Ticket instance for {} at {:?}",
                    ticket_id,
                    file_path
                );
            }
        }
    }

    /// Insert or update a plan in the store.
    pub fn upsert_plan(&self, metadata: PlanMetadata) {
        if let Some(id) = metadata.id.clone() {
            self.plans.insert(id.to_string(), metadata);
        } else {
            self.init_warnings.add(InitWarning {
                file_path: metadata.file_path.clone(),
                message: "Skipping plan upsert: missing ID in frontmatter".to_string(),
                entity_type: "plan".to_string(),
            });
        }
    }

    /// Remove a plan from the store by ID.
    pub fn remove_plan(&self, id: &str) {
        self.plans.remove(id);
    }

    /// Get a reference to the embeddings DashMap (for use by embeddings/search modules).
    pub(crate) fn embeddings(&self) -> &DashMap<String, Vec<f32>> {
        &self.embeddings
    }

    /// Get a reference to the tickets DashMap (for use by query modules).
    pub(crate) fn tickets(&self) -> &DashMap<String, TicketMetadata> {
        &self.tickets
    }

    /// Get a reference to the plans DashMap (for use by query modules).
    pub(crate) fn plans(&self) -> &DashMap<String, PlanMetadata> {
        &self.plans
    }

    /// Get the initialization warnings captured during store loading.
    ///
    /// Returns a copy of all warnings that occurred while parsing ticket and plan files.
    /// These warnings indicate files that were skipped due to errors, ID mismatches, or
    /// other non-fatal issues during initialization.
    pub fn get_init_warnings(&self) -> InitWarnings {
        self.init_warnings.clone()
    }

    /// Re-read a specific ticket from disk and upsert it into the store.
    ///
    /// This should be called after a mutation writes changes to disk,
    /// so the in-memory store is immediately consistent before queries
    /// are performed. The filesystem watcher provides eventual consistency for
    /// external changes, but direct mutations need immediate store updates.
    pub async fn refresh_ticket_in_store(&self, ticket_id: &str) {
        let ticket = match crate::ticket::Ticket::find(ticket_id).await {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(
                    "Failed to find ticket '{}' for store refresh: {}",
                    ticket_id,
                    e
                );
                return;
            }
        };
        let metadata = match ticket.read() {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(
                    "Failed to read ticket '{}' for store refresh: {}",
                    ticket_id,
                    e
                );
                return;
            }
        };
        self.upsert_ticket(metadata);
    }

    /// Re-read a specific plan from disk and upsert it into the store.
    ///
    /// This is the plan equivalent of `refresh_ticket_in_store`. It should be
    /// called after a mutation writes plan changes to disk, so the in-memory
    /// store is immediately consistent.
    pub async fn refresh_plan_in_store(&self, plan_id: &str) {
        let plan = match crate::plan::Plan::find(plan_id).await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("Failed to find plan '{}' for store refresh: {}", plan_id, e);
                return;
            }
        };
        let metadata = match plan.read() {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("Failed to read plan '{}' for store refresh: {}", plan_id, e);
                return;
            }
        };
        self.upsert_plan(metadata);
    }
}

#[cfg(test)]
pub(crate) mod test_helpers {
    /// Create a minimal ticket markdown file content for testing.
    pub fn make_ticket_content(id: &str, title: &str) -> String {
        // Derive a deterministic but unique UUID from the ticket ID
        let uuid = format!("550e8400-e29b-41d4-a716-{:0>12}", id.replace('-', ""));
        format!(
            r#"---
id: {id}
uuid: {uuid}
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
size: medium
---
# {title}

Description for {id}.
"#
        )
    }

    /// Create a minimal plan markdown file content for testing.
    pub fn make_plan_content(id: &str, title: &str) -> String {
        // Derive a deterministic but unique UUID from the plan ID
        let uuid = format!("550e8400-e29b-41d4-a716-{:0>12}", id.replace('-', ""));
        format!(
            r#"---
id: {id}
uuid: {uuid}
created: 2024-01-01T00:00:00Z
---
# {title}

Plan description.

## Tickets

1. j-a1b2
"#
        )
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::test_helpers::{make_plan_content, make_ticket_content};
    use super::*;
    use crate::paths::JanusRootGuard;
    use crate::types::{PlanId, TicketId};
    use crate::types::{TicketPriority, TicketStatus, TicketType};

    /// Set up a temporary Janus directory with ticket and plan files.
    /// Returns the TempDir (must be held alive for the duration of the test).
    fn setup_test_dir() -> TempDir {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let janus_root = tmp.path().join(".janus");
        let items_dir = janus_root.join("items");
        let plans_dir = janus_root.join("plans");

        fs::create_dir_all(&items_dir).expect("failed to create items dir");
        fs::create_dir_all(&plans_dir).expect("failed to create plans dir");

        // Write ticket files
        fs::write(
            items_dir.join("j-a1b2.md"),
            make_ticket_content("j-a1b2", "First Ticket"),
        )
        .unwrap();
        fs::write(
            items_dir.join("j-c3d4.md"),
            make_ticket_content("j-c3d4", "Second Ticket"),
        )
        .unwrap();

        // Write plan files
        fs::write(
            plans_dir.join("plan-x1y2.md"),
            make_plan_content("plan-x1y2", "Test Plan"),
        )
        .unwrap();

        tmp
    }

    #[test]
    fn test_empty_store() {
        let store = TicketStore::empty();
        assert_eq!(store.tickets.len(), 0);
        assert_eq!(store.plans.len(), 0);
        assert_eq!(store.embeddings.len(), 0);
    }

    #[tokio::test]
    async fn test_init_from_disk() {
        let tmp = setup_test_dir();
        let janus_root = tmp.path().join(".janus");

        let _guard = JanusRootGuard::new(&janus_root);

        let store = TicketStore::init().await.expect("init should succeed");

        assert_eq!(store.tickets.len(), 2);
        assert!(store.tickets.contains_key("j-a1b2"));
        assert!(store.tickets.contains_key("j-c3d4"));

        assert_eq!(store.plans.len(), 1);
        assert!(store.plans.contains_key("plan-x1y2"));

        // Verify ticket metadata
        let ticket = store.tickets.get("j-a1b2").unwrap();
        assert_eq!(ticket.title.as_deref(), Some("First Ticket"));
        assert_eq!(ticket.status, Some(TicketStatus::New));
        assert_eq!(ticket.ticket_type, Some(TicketType::Task));
        assert_eq!(ticket.priority, Some(TicketPriority::P2));

        // Verify plan metadata
        let plan = store.plans.get("plan-x1y2").unwrap();
        assert_eq!(plan.title.as_deref(), Some("Test Plan"));
    }

    #[tokio::test]
    async fn test_init_with_missing_dirs() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let janus_root = tmp.path().join(".janus");
        // Don't create the directories

        let _guard = JanusRootGuard::new(&janus_root);

        let store = TicketStore::init()
            .await
            .expect("init should succeed even with missing dirs");
        assert_eq!(store.tickets.len(), 0);
        assert_eq!(store.plans.len(), 0);
    }

    #[tokio::test]
    async fn test_init_with_invalid_file() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let janus_root = tmp.path().join(".janus");
        let items_dir = janus_root.join("items");

        fs::create_dir_all(&items_dir).unwrap();

        // Write a valid ticket
        fs::write(
            items_dir.join("j-good.md"),
            make_ticket_content("j-good", "Good Ticket"),
        )
        .unwrap();

        // Write an invalid ticket file
        fs::write(
            items_dir.join("j-bad.md"),
            "this is not a valid ticket file",
        )
        .unwrap();

        let _guard = JanusRootGuard::new(&janus_root);

        let store = TicketStore::init()
            .await
            .expect("init should succeed despite parse errors");
        // Only the valid ticket should be loaded
        assert_eq!(store.tickets.len(), 1);
        assert!(store.tickets.contains_key("j-good"));
    }

    #[tokio::test]
    async fn test_init_with_mismatched_id() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let janus_root = tmp.path().join(".janus");
        let items_dir = janus_root.join("items");

        fs::create_dir_all(&items_dir).unwrap();

        // Write a ticket where the filename stem differs from the frontmatter id.
        // Filename: j-file.md, frontmatter id: j-wrong
        fs::write(
            items_dir.join("j-file.md"),
            make_ticket_content("j-wrong", "Mismatched Ticket"),
        )
        .unwrap();

        let _guard = JanusRootGuard::new(&janus_root);

        let store = TicketStore::init().await.expect("init should succeed");

        // The ticket should be stored under the filename stem, not the frontmatter id
        assert_eq!(store.tickets.len(), 1);
        assert!(
            store.tickets.contains_key("j-file"),
            "ticket should be keyed by filename stem"
        );
        assert!(
            !store.tickets.contains_key("j-wrong"),
            "frontmatter id should not be used as key"
        );

        // The metadata ID should also reflect the filename stem
        let ticket = store.tickets.get("j-file").unwrap();
        assert_eq!(ticket.id.as_deref(), Some("j-file"));
    }

    #[test]
    fn test_upsert_ticket() {
        let store = TicketStore::empty();

        let metadata = TicketMetadata {
            id: Some(TicketId::new_unchecked("j-new1")),
            title: Some("New Ticket".to_string()),
            status: Some(TicketStatus::New),
            ..Default::default()
        };
        store.upsert_ticket(metadata);

        assert_eq!(store.tickets.len(), 1);
        assert!(store.tickets.contains_key("j-new1"));
        assert_eq!(
            store.tickets.get("j-new1").unwrap().title.as_deref(),
            Some("New Ticket")
        );

        // Update it
        let updated = TicketMetadata {
            id: Some(TicketId::new_unchecked("j-new1")),
            title: Some("Updated Ticket".to_string()),
            status: Some(TicketStatus::InProgress),
            ..Default::default()
        };
        store.upsert_ticket(updated);

        assert_eq!(store.tickets.len(), 1);
        assert_eq!(
            store.tickets.get("j-new1").unwrap().title.as_deref(),
            Some("Updated Ticket")
        );
    }

    #[test]
    fn test_remove_ticket() {
        let store = TicketStore::empty();

        store.upsert_ticket(TicketMetadata {
            id: Some(TicketId::new_unchecked("j-rm1")),
            ..Default::default()
        });
        assert_eq!(store.tickets.len(), 1);

        store.remove_ticket_with_cascade("j-rm1");
        assert_eq!(store.tickets.len(), 0);

        // Removing a nonexistent ticket should not panic
        store.remove_ticket_with_cascade("j-nonexistent");
    }

    #[test]
    fn test_upsert_plan() {
        let store = TicketStore::empty();

        let metadata = PlanMetadata {
            id: Some(PlanId::new_unchecked("plan-new1")),
            title: Some("New Plan".to_string()),
            ..Default::default()
        };
        store.upsert_plan(metadata);

        assert_eq!(store.plans.len(), 1);
        assert!(store.plans.contains_key("plan-new1"));
    }

    #[test]
    fn test_remove_plan() {
        let store = TicketStore::empty();

        store.upsert_plan(PlanMetadata {
            id: Some(PlanId::new_unchecked("plan-rm1")),
            ..Default::default()
        });
        assert_eq!(store.plans.len(), 1);

        store.remove_plan("plan-rm1");
        assert_eq!(store.plans.len(), 0);
    }

    #[test]
    fn test_upsert_ticket_without_id_is_noop() {
        let store = TicketStore::empty();

        store.upsert_ticket(TicketMetadata {
            id: None,
            title: Some("No ID".to_string()),
            ..Default::default()
        });

        assert_eq!(store.tickets.len(), 0);
    }

    #[test]
    fn test_upsert_plan_without_id_is_noop() {
        let store = TicketStore::empty();

        store.upsert_plan(PlanMetadata {
            id: None,
            title: Some("No ID Plan".to_string()),
            ..Default::default()
        });

        assert_eq!(store.plans.len(), 0);
    }

    #[tokio::test]
    async fn test_file_paths_populated() {
        let tmp = setup_test_dir();
        let janus_root = tmp.path().join(".janus");

        let _guard = JanusRootGuard::new(&janus_root);

        let store = TicketStore::init().await.expect("init should succeed");

        // Ticket file paths should be set
        let ticket = store.tickets.get("j-a1b2").unwrap();
        assert!(ticket.file_path.is_some());
        let file_path = ticket.file_path.as_ref().unwrap();
        assert!(file_path.ends_with("j-a1b2.md"));

        // Plan file paths should be set
        let plan = store.plans.get("plan-x1y2").unwrap();
        assert!(plan.file_path.is_some());
        let file_path = plan.file_path.as_ref().unwrap();
        assert!(file_path.ends_with("plan-x1y2.md"));
    }

    #[tokio::test]
    async fn test_init_warnings_captured() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let janus_root = tmp.path().join(".janus");
        let items_dir = janus_root.join("items");

        fs::create_dir_all(&items_dir).unwrap();

        // Write a valid ticket
        fs::write(
            items_dir.join("j-good.md"),
            make_ticket_content("j-good", "Good Ticket"),
        )
        .unwrap();

        // Write an invalid ticket file
        fs::write(
            items_dir.join("j-bad.md"),
            "this is not a valid ticket file",
        )
        .unwrap();

        // Write a ticket with mismatched ID
        fs::write(
            items_dir.join("j-mismatch.md"),
            make_ticket_content("j-wrong-id", "Mismatched Ticket"),
        )
        .unwrap();

        let _guard = JanusRootGuard::new(&janus_root);

        let store = TicketStore::init()
            .await
            .expect("init should succeed despite errors");

        // Verify the valid ticket was loaded
        assert_eq!(store.tickets.len(), 2); // j-good and j-mismatch
        assert!(store.tickets.contains_key("j-good"));
        assert!(store.tickets.contains_key("j-mismatch")); // keyed by filename

        // Verify warnings were captured
        let warnings = store.get_init_warnings();
        assert!(!warnings.is_empty(), "should have captured warnings");

        // Should have 2 warnings: 1 for parse error, 1 for ID mismatch
        let all_warnings = warnings.get_all();
        assert_eq!(all_warnings.len(), 2, "should have 2 warnings");

        // Check ticket-specific warnings
        let ticket_warnings = warnings.ticket_warnings();
        assert_eq!(ticket_warnings.len(), 2, "should have 2 ticket warnings");

        // Verify one warning is for the parse error
        let parse_error = all_warnings
            .iter()
            .any(|w| w.message.contains("Failed to parse"));
        assert!(parse_error, "should have a parse error warning");

        // Verify one warning is for the ID mismatch
        let id_mismatch = all_warnings
            .iter()
            .any(|w| w.message.contains("ID mismatch"));
        assert!(id_mismatch, "should have an ID mismatch warning");
    }

    #[tokio::test]
    async fn test_init_warnings_empty_when_all_valid() {
        let tmp = setup_test_dir();
        let janus_root = tmp.path().join(".janus");

        let _guard = JanusRootGuard::new(&janus_root);

        let store = TicketStore::init().await.expect("init should succeed");

        // No warnings should be captured for valid files
        let warnings = store.get_init_warnings();
        assert!(
            warnings.is_empty(),
            "should have no warnings for valid files"
        );
        assert_eq!(warnings.count(), 0);
        assert!(warnings.ticket_warnings().is_empty());
        assert!(warnings.plan_warnings().is_empty());
    }
}

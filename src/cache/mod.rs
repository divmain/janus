use std::fs;
use std::path::{Path, PathBuf};

use dashmap::DashMap;
use tokio::sync::OnceCell;

use crate::error::Result;
use crate::plan::parser::parse_plan_content;
use crate::plan::types::PlanMetadata;
use crate::ticket::parse_ticket;
use crate::types::{TicketMetadata, plans_dir, tickets_items_dir};

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

pub use watcher::{StoreEvent, start_watching, subscribe_to_changes};

/// In-memory store for ticket and plan metadata with concurrent access.
///
/// The store holds all ticket and plan metadata in `DashMap` structures,
/// allowing lock-free concurrent reads and fine-grained locking for writes.
/// It also manages embedding vectors for semantic search.
pub struct TicketStore {
    tickets: DashMap<String, TicketMetadata>,
    plans: DashMap<String, PlanMetadata>,
    embeddings: DashMap<String, Vec<f32>>,
}

/// Global singleton for the ticket store.
static STORE: OnceCell<TicketStore> = OnceCell::const_new();

/// Get or initialize the global ticket store singleton.
///
/// On first call, reads all tickets and plans from disk to populate the store.
/// Subsequent calls return the existing store without re-reading.
/// If initialization fails, the error is propagated and the `OnceCell` remains
/// unset, allowing subsequent calls to retry.
pub async fn get_or_init_store() -> Result<&'static TicketStore> {
    STORE
        .get_or_try_init(|| async {
            tokio::task::spawn_blocking(TicketStore::init)
                .await
                .map_err(|e| crate::error::JanusError::BlockingTaskFailed(e.to_string()))?
        })
        .await
}

impl TicketStore {
    /// Create an empty store with no tickets or plans.
    pub fn empty() -> Self {
        TicketStore {
            tickets: DashMap::new(),
            plans: DashMap::new(),
            embeddings: DashMap::new(),
        }
    }

    /// Initialize the store by reading all tickets and plans from disk.
    ///
    /// Scans `.janus/items/` for ticket files and `.janus/plans/` for plan files,
    /// parsing each and populating the internal DashMaps. Files that fail to parse
    /// are logged as warnings but do not prevent initialization.
    pub fn init() -> Result<Self> {
        let store = Self::empty();

        // Load tickets
        let items_dir = tickets_items_dir();
        if items_dir.exists() {
            store.load_tickets_from_dir(&items_dir);
        }

        // Load plans
        let p_dir = plans_dir();
        if p_dir.exists() {
            store.load_plans_from_dir(&p_dir);
        }

        // Load embeddings (requires tickets to be loaded first)
        store.load_embeddings();

        Ok(store)
    }

    /// Generic function to load entities from a directory into the store.
    ///
    /// This function abstracts the common logic for loading both tickets and plans
    /// from markdown files, eliminating code duplication between `load_tickets_from_dir`
    /// and `load_plans_from_dir`.
    ///
    /// # Type Parameters
    ///
    /// - `T`: The entity metadata type (e.g., `TicketMetadata`, `PlanMetadata`)
    /// - `F`: The parser function type
    ///
    /// # Arguments
    ///
    /// - `dir`: The directory to scan for `.md` files
    /// - `entity_name`: Name of the entity type (for error messages: "ticket" or "plan")
    /// - `parser`: Function to parse file content into the entity type
    /// - `insert`: Function to insert the parsed entity into the appropriate DashMap
    fn load_entities_from_dir<T, F>(
        &self,
        dir: &Path,
        entity_name: &str,
        parser: F,
        mut insert: impl FnMut(T),
    ) where
        T: EntityMetadata,
        F: Fn(&str) -> crate::error::Result<T>,
    {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) => {
                eprintln!("Warning: failed to read {entity_name}s directory: {e}");
                return;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "md") {
                match fs::read_to_string(&path) {
                    Ok(content) => match parser(&content) {
                        Ok(mut metadata) => {
                            if let Some(stem) = path.file_stem() {
                                let stem_str = stem.to_string_lossy();
                                match metadata.id() {
                                    Some(frontmatter_id) if frontmatter_id != stem_str.as_ref() => {
                                        eprintln!(
                                            "Warning: {entity_name} file '{stem_str}' has frontmatter id '{frontmatter_id}' — \
                                             using filename stem as authoritative ID",
                                        );
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
                            eprintln!(
                                "Warning: failed to parse {entity_name} {}: {e}",
                                path.display()
                            );
                        }
                    },
                    Err(e) => {
                        eprintln!(
                            "Warning: failed to read {entity_name} {}: {e}",
                            path.display()
                        );
                    }
                }
            }
        }
    }

    /// Load all ticket files from a directory into the store.
    fn load_tickets_from_dir(&self, dir: &Path) {
        self.load_entities_from_dir(dir, "ticket", parse_ticket, |metadata: TicketMetadata| {
            if let Some(id) = metadata.id.clone() {
                self.tickets.insert(id.to_string(), metadata);
            }
        });
    }

    /// Load all plan files from a directory into the store.
    fn load_plans_from_dir(&self, dir: &Path) {
        self.load_entities_from_dir(dir, "plan", parse_plan_content, |metadata: PlanMetadata| {
            if let Some(id) = metadata.id.clone() {
                self.plans.insert(id.to_string(), metadata);
            }
        });
    }

    /// Insert or update a ticket in the store.
    pub fn upsert_ticket(&self, metadata: TicketMetadata) {
        if let Some(id) = metadata.id.clone() {
            self.tickets.insert(id.to_string(), metadata);
        }
    }

    /// Remove a ticket from the store by ID.
    ///
    /// Also removes the corresponding embedding entry to prevent orphaned
    /// embeddings from inflating coverage counts.
    pub fn remove_ticket(&self, id: &str) {
        self.tickets.remove(id);
        self.embeddings.remove(id);
    }

    /// Insert or update a plan in the store.
    pub fn upsert_plan(&self, metadata: PlanMetadata) {
        if let Some(id) = metadata.id.clone() {
            self.plans.insert(id.to_string(), metadata);
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
    use serial_test::serial;
    use tempfile::TempDir;

    use super::test_helpers::{make_plan_content, make_ticket_content};
    use super::*;
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

    #[test]
    #[serial]
    fn test_init_from_disk() {
        let tmp = setup_test_dir();
        let janus_root = tmp.path().join(".janus");

        // SAFETY: We use #[serial] to ensure single-threaded access
        unsafe { std::env::set_var("JANUS_ROOT", janus_root.to_str().unwrap()) };

        let store = TicketStore::init().expect("init should succeed");

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

        unsafe { std::env::remove_var("JANUS_ROOT") };
    }

    #[test]
    #[serial]
    fn test_init_with_missing_dirs() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let janus_root = tmp.path().join(".janus");
        // Don't create the directories

        unsafe { std::env::set_var("JANUS_ROOT", janus_root.to_str().unwrap()) };

        let store = TicketStore::init().expect("init should succeed even with missing dirs");
        assert_eq!(store.tickets.len(), 0);
        assert_eq!(store.plans.len(), 0);

        unsafe { std::env::remove_var("JANUS_ROOT") };
    }

    #[test]
    #[serial]
    fn test_init_with_invalid_file() {
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

        unsafe { std::env::set_var("JANUS_ROOT", janus_root.to_str().unwrap()) };

        let store = TicketStore::init().expect("init should succeed despite parse errors");
        // Only the valid ticket should be loaded
        assert_eq!(store.tickets.len(), 1);
        assert!(store.tickets.contains_key("j-good"));

        unsafe { std::env::remove_var("JANUS_ROOT") };
    }

    #[test]
    #[serial]
    fn test_init_with_mismatched_id() {
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

        unsafe { std::env::set_var("JANUS_ROOT", janus_root.to_str().unwrap()) };

        let store = TicketStore::init().expect("init should succeed");

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

        unsafe { std::env::remove_var("JANUS_ROOT") };
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

        store.remove_ticket("j-rm1");
        assert_eq!(store.tickets.len(), 0);

        // Removing a nonexistent ticket should not panic
        store.remove_ticket("j-nonexistent");
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

    #[test]
    #[serial]
    fn test_file_paths_populated() {
        let tmp = setup_test_dir();
        let janus_root = tmp.path().join(".janus");

        unsafe { std::env::set_var("JANUS_ROOT", janus_root.to_str().unwrap()) };

        let store = TicketStore::init().expect("init should succeed");

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

        unsafe { std::env::remove_var("JANUS_ROOT") };
    }
}

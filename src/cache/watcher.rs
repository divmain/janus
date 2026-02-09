//! Filesystem watcher for the Janus store.
//!
//! Watches the `.janus/` directory recursively for changes and updates
//! the store's DashMaps automatically. Uses `notify::RecommendedWatcher`
//! with a tokio channel bridge and custom debouncing.
//!
//! By watching `.janus/` recursively (rather than `items/` and `plans/`
//! individually), the watcher automatically picks up subdirectories that
//! are created after startup — important for fresh projects where the TUI
//! is started before any tickets exist.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use notify::{EventKind, RecursiveMode, Watcher};
use tokio::sync::broadcast;

use crate::error::{JanusError, Result};
use crate::plan::parser::parse_plan_content;
use crate::ticket::parse_ticket;
use crate::types::janus_root;

use super::TicketStore;

/// Duration to wait for additional events before processing a batch.
const DEBOUNCE_DURATION: std::time::Duration = std::time::Duration::from_millis(150);

/// Notification event sent when the store is updated by the watcher.
#[derive(Debug, Clone)]
pub enum StoreEvent {
    /// One or more tickets were created, modified, or deleted.
    TicketsChanged,
    /// One or more plans were created, modified, or deleted.
    PlansChanged,
}

/// Filesystem watcher that monitors `.janus/` recursively and updates
/// a `TicketStore` when ticket or plan files change.
///
/// The watcher must be kept alive — dropping it stops watching.
pub struct StoreWatcher {
    /// Broadcast sender for store change notifications.
    sender: broadcast::Sender<StoreEvent>,
    /// Handle to the underlying `notify` filesystem watcher. This field is
    /// never read directly, but it **must** be kept alive: dropping the
    /// `RecommendedWatcher` deregisters the OS file-watch and stops all
    /// event delivery. The `_` prefix silences the "unused field" compiler
    /// warning while preserving the keep-alive semantics.
    _watcher: notify::RecommendedWatcher,
}

impl StoreWatcher {
    /// Start watching the `.janus/` directory recursively.
    ///
    /// Returns the watcher and a broadcast receiver for change events.
    /// The watcher spawns a background tokio task that debounces filesystem
    /// events and updates the store.
    ///
    /// If `.janus/` doesn't exist, the watcher is still returned but won't
    /// produce events. Subdirectories created after startup (e.g. `items/`,
    /// `plans/`) are automatically picked up by the recursive watch.
    ///
    /// # Limitation: `.janus/` must exist at startup
    ///
    /// If `.janus/` does not exist when this method is called, no OS watch
    /// is registered. If `.janus/` is created later (e.g. by running
    /// `janus create` in another terminal), the watcher will **not** pick it
    /// up — the TUI or MCP server must be restarted.
    ///
    /// Watching the parent working directory for `.janus/` creation would
    /// trigger events for every file change in the project, which is too
    /// noisy to be practical.
    pub fn start(store: &'static TicketStore) -> Result<(Self, broadcast::Receiver<StoreEvent>)> {
        let (broadcast_tx, broadcast_rx) = broadcast::channel(64);
        let (bridge_tx, bridge_rx) = tokio::sync::mpsc::unbounded_channel::<notify::Event>();

        // Create the notify watcher with a callback that bridges to tokio
        let watcher = {
            let tx = bridge_tx;
            notify::RecommendedWatcher::new(
                move |res: std::result::Result<notify::Event, notify::Error>| match res {
                    Ok(event) => {
                        let _ = tx.send(event);
                    }
                    Err(e) => {
                        eprintln!("Warning: filesystem watcher error: {e}");
                    }
                },
                notify::Config::default(),
            )
            .map_err(|e| {
                JanusError::WatcherError(format!("failed to create filesystem watcher: {e}"))
            })?
        };

        // Watch the .janus/ root directory recursively. This automatically
        // picks up subdirectories (items/, plans/) created after startup,
        // which matters for fresh projects where the TUI starts before any
        // tickets exist. Events are filtered in process_batch() to only
        // handle .md files under items/ or plans/.
        let mut watcher = watcher;
        let root = janus_root();
        if root.exists() {
            if let Err(e) = watcher.watch(&root, RecursiveMode::Recursive) {
                eprintln!("Warning: failed to watch .janus directory: {e}");
            }
        } else {
            // .janus/ doesn't exist yet — nothing to watch.
            // If it's created later (e.g. `janus create` in another terminal),
            // the watcher won't pick it up automatically. The TUI or MCP server
            // must be restarted to begin watching. The TUI handles this case by
            // showing a "No Janus Directory" empty state via InitResult::NoJanusDir.
            eprintln!(
                "Note: .janus directory not found — file watching is disabled. \
                 Restart after creating your first ticket."
            );
        }

        // Spawn the background debounce + process task
        let task_tx = broadcast_tx.clone();
        tokio::spawn(async move {
            run_event_loop(bridge_rx, store, task_tx).await;
        });

        Ok((
            StoreWatcher {
                sender: broadcast_tx,
                _watcher: watcher,
            },
            broadcast_rx,
        ))
    }

    /// Subscribe to store change events.
    pub fn subscribe(&self) -> broadcast::Receiver<StoreEvent> {
        self.sender.subscribe()
    }
}

/// Global watcher instance, stored statically to keep the `notify::RecommendedWatcher`
/// alive for the process lifetime. This replaces `std::mem::forget` with an explicit
/// static, so the OS file handles are properly documented as intentionally long-lived
/// and could be reclaimed if we ever add a shutdown path.
///
/// The tuple stores `(StoreWatcher, store_identity)` where `store_identity` is the
/// raw pointer address of the `&'static TicketStore` that was used to initialize the
/// watcher. This allows subsequent calls to `start_watching` to detect if a different
/// store instance is passed and return an explicit error instead of silently keeping
/// the old binding.
///
/// # Limitation: no recovery after watcher failure
///
/// If the underlying `notify::RecommendedWatcher` encounters a fatal error after
/// successful initialization (e.g., the OS revokes the watch handle, or the
/// background thread panics), the `OnceLock` prevents re-initialization. The
/// watcher will silently stop delivering events for the rest of the process
/// lifetime. In practice this is unlikely — `notify` watchers are robust on
/// macOS (FSEvents), Linux (inotify), and Windows (ReadDirectoryChanges) — but
/// if it occurs, the process must be restarted.
static WATCHER: OnceLock<(StoreWatcher, usize)> = OnceLock::new();

/// Return the identity (pointer address) of a `&'static TicketStore`.
fn store_identity(store: &'static TicketStore) -> usize {
    std::ptr::from_ref(store) as usize
}

/// Start the filesystem watcher for the given store.
///
/// Returns a broadcast receiver for store change events.
/// The first call starts the watcher; subsequent calls return a new receiver
/// from the existing sender, provided the same store instance is passed.
///
/// # Errors
///
/// Returns `JanusError::WatcherError` if a subsequent call passes a different
/// store instance than the one used for initialization. This prevents silent
/// action-at-a-distance bugs where the watcher keeps updating a stale store.
///
/// If two threads race to start the watcher, only one wins the `OnceLock::set`.
/// The loser's watcher is dropped (its background task ends), but we always
/// subscribe from the winning watcher stored in `WATCHER`, so the returned
/// receiver is always valid.
pub async fn start_watching(
    store: &'static TicketStore,
) -> Result<broadcast::Receiver<StoreEvent>> {
    let incoming_identity = store_identity(store);

    if let Some((watcher, bound_identity)) = WATCHER.get() {
        if *bound_identity != incoming_identity {
            return Err(JanusError::WatcherError(
                "watcher singleton is already bound to a different TicketStore instance; \
                 cannot re-bind to a new store"
                    .to_string(),
            ));
        }
        return Ok(watcher.subscribe());
    }

    let (watcher, _rx) = StoreWatcher::start(store)?;

    // Store the watcher in a static OnceLock to keep it alive for the process
    // lifetime. The notify watcher must remain alive or file watching stops.
    // If another thread raced and set it first, `set` returns Err and our
    // local watcher is dropped — that's fine, we subscribe from the winner.
    let _ = WATCHER.set((watcher, incoming_identity));

    // Always subscribe from whichever watcher won the race into WATCHER.
    // After set, verify the bound store matches ours (handles the race case
    // where another thread won with a different store).
    let (winning_watcher, bound_identity) = WATCHER
        .get()
        .expect("WATCHER must be initialized: either we set it or another thread did");

    if *bound_identity != incoming_identity {
        return Err(JanusError::WatcherError(
            "watcher singleton was initialized by another thread with a different \
             TicketStore instance"
                .to_string(),
        ));
    }

    Ok(winning_watcher.subscribe())
}

/// Subscribe to store change events (if watcher has been started).
pub fn subscribe_to_changes() -> Option<broadcast::Receiver<StoreEvent>> {
    WATCHER.get().map(|(watcher, _)| watcher.subscribe())
}

/// Background event loop: receives notify events, debounces them, and
/// processes batched changes against the store.
async fn run_event_loop(
    mut bridge_rx: tokio::sync::mpsc::UnboundedReceiver<notify::Event>,
    store: &'static TicketStore,
    broadcast_tx: broadcast::Sender<StoreEvent>,
) {
    // Accumulate events keyed by path → last event kind
    let mut pending: HashMap<PathBuf, EventKind> = HashMap::new();

    loop {
        // Wait for the first event (blocks until something happens)
        let event = match bridge_rx.recv().await {
            Some(e) => e,
            None => break, // Channel closed — watcher was dropped
        };

        // Accumulate this event
        accumulate_event(&mut pending, &event);

        // Drain additional events within the debounce window
        loop {
            match tokio::time::timeout(DEBOUNCE_DURATION, bridge_rx.recv()).await {
                Ok(Some(e)) => {
                    accumulate_event(&mut pending, &e);
                }
                Ok(None) => {
                    // Channel closed
                    process_batch(&mut pending, store, &broadcast_tx);
                    return;
                }
                Err(_) => {
                    // Timeout — debounce window elapsed, process the batch
                    break;
                }
            }
        }

        process_batch(&mut pending, store, &broadcast_tx);
    }
}

/// Accumulate a notify event into the pending map.
///
/// For each path in the event, stores the event kind. Later events
/// for the same path overwrite earlier ones (last-writer-wins).
fn accumulate_event(pending: &mut HashMap<PathBuf, EventKind>, event: &notify::Event) {
    for path in &event.paths {
        pending.insert(path.clone(), event.kind);
    }
}

/// Process a batch of accumulated filesystem events.
///
/// Determines whether each path is a ticket or plan, then upserts or
/// removes accordingly. Sends broadcast notifications for each category
/// that had changes.
fn process_batch(
    pending: &mut HashMap<PathBuf, EventKind>,
    store: &'static TicketStore,
    broadcast_tx: &broadcast::Sender<StoreEvent>,
) {
    if pending.is_empty() {
        return;
    }

    let mut tickets_changed = false;
    let mut plans_changed = false;

    for (path, kind) in pending.drain() {
        // Only process .md files
        if path.extension().is_none_or(|ext| ext != "md") {
            continue;
        }

        let is_ticket = is_ticket_path(&path);
        let is_plan = is_plan_path(&path);

        if !is_ticket && !is_plan {
            continue;
        }

        match classify_event_kind(kind) {
            FileAction::CreateOrModify => {
                if is_ticket {
                    if process_ticket_file(&path, store) {
                        tickets_changed = true;
                    }
                } else if is_plan && process_plan_file(&path, store) {
                    plans_changed = true;
                }
            }
            FileAction::Remove => {
                if let Some(id) = path.file_stem().map(|s| s.to_string_lossy().to_string()) {
                    if is_ticket {
                        store.remove_ticket(&id);
                        tickets_changed = true;
                    } else if is_plan {
                        store.remove_plan(&id);
                        plans_changed = true;
                    }
                }
            }
            FileAction::Ignore => {}
        }
    }

    if tickets_changed {
        let _ = broadcast_tx.send(StoreEvent::TicketsChanged);
    }
    if plans_changed {
        let _ = broadcast_tx.send(StoreEvent::PlansChanged);
    }
}

/// Classify a notify event kind into one of our simplified actions.
fn classify_event_kind(kind: EventKind) -> FileAction {
    match kind {
        EventKind::Create(_) | EventKind::Modify(_) => FileAction::CreateOrModify,
        EventKind::Remove(_) => FileAction::Remove,
        _ => FileAction::Ignore,
    }
}

enum FileAction {
    CreateOrModify,
    Remove,
    Ignore,
}

/// Check if a path is within the tickets items directory.
fn is_ticket_path(path: &Path) -> bool {
    // Check if any ancestor component is "items" within a .janus directory
    let components: Vec<_> = path.components().collect();
    for (i, comp) in components.iter().enumerate() {
        if let std::path::Component::Normal(s) = comp {
            if *s == "items" && i > 0 {
                if let std::path::Component::Normal(parent) = &components[i - 1] {
                    if parent.to_string_lossy() == ".janus" {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Check if a path is within the plans directory.
fn is_plan_path(path: &Path) -> bool {
    let components: Vec<_> = path.components().collect();
    for (i, comp) in components.iter().enumerate() {
        if let std::path::Component::Normal(s) = comp {
            if *s == "plans" && i > 0 {
                if let std::path::Component::Normal(parent) = &components[i - 1] {
                    if parent.to_string_lossy() == ".janus" {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Read and parse a ticket file, updating the store. Returns true if the store changed.
///
/// If the file no longer exists (race with deletion), removes the ticket from the store.
fn process_ticket_file(path: &Path, store: &TicketStore) -> bool {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // File was deleted between event and processing — treat as removal
            if let Some(stem) = path.file_stem() {
                store.remove_ticket(&stem.to_string_lossy());
            }
            return true;
        }
        Err(e) => {
            eprintln!(
                "Warning: failed to read ticket file {}: {e}",
                path.display()
            );
            return false;
        }
    };

    match parse_ticket(&content) {
        Ok(mut metadata) => {
            if let Some(stem) = path.file_stem() {
                crate::ticket::enforce_filename_authority(&mut metadata, &stem.to_string_lossy());
            }
            metadata.file_path = Some(path.to_path_buf());
            // Capture the ID before upsert consumes ownership
            let ticket_id = metadata.id.clone();
            store.upsert_ticket(metadata);
            // Invalidate stale embedding — the ticket content changed but
            // the embedding was computed from the old content. The user can
            // run `janus cache rebuild` to regenerate embeddings.
            if let Some(id) = &ticket_id {
                store.embeddings().remove(id.as_ref());
            }
            true
        }
        Err(e) => {
            eprintln!(
                "Warning: failed to parse ticket file {}: {e}",
                path.display()
            );
            false
        }
    }
}

/// Read and parse a plan file, updating the store. Returns true if the store changed.
///
/// If the file no longer exists (race with deletion), removes the plan from the store.
fn process_plan_file(path: &Path, store: &TicketStore) -> bool {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // File was deleted between event and processing — treat as removal
            if let Some(stem) = path.file_stem() {
                store.remove_plan(&stem.to_string_lossy());
            }
            return true;
        }
        Err(e) => {
            eprintln!("Warning: failed to read plan file {}: {e}", path.display());
            return false;
        }
    };

    match parse_plan_content(&content) {
        Ok(mut metadata) => {
            if metadata.id.is_none() {
                if let Some(stem) = path.file_stem() {
                    metadata.id = Some(crate::types::PlanId::new_unchecked(stem.to_string_lossy()));
                }
            }
            metadata.file_path = Some(path.to_path_buf());
            store.upsert_plan(metadata);
            true
        }
        Err(e) => {
            eprintln!("Warning: failed to parse plan file {}: {e}", path.display());
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::Duration;

    use serial_test::serial;
    use tempfile::TempDir;
    use tokio::time::{sleep, timeout};

    use super::*;
    use crate::cache::TicketStore;
    use crate::cache::test_helpers::{make_plan_content, make_ticket_content};

    /// Set up a temporary Janus directory structure.
    fn setup_test_dir() -> TempDir {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let janus_root = tmp.path().join(".janus");
        let items_dir = janus_root.join("items");
        let plans_dir = janus_root.join("plans");

        fs::create_dir_all(&items_dir).expect("failed to create items dir");
        fs::create_dir_all(&plans_dir).expect("failed to create plans dir");

        tmp
    }

    /// Leak a `TicketStore` to obtain a `&'static` reference for tests.
    ///
    /// `StoreWatcher::start()` requires `&'static TicketStore` because the
    /// watcher spawns a background tokio task that must outlive any particular
    /// scope. `Box::leak` is the standard Rust pattern for creating `'static`
    /// references in tests — the leaked memory is reclaimed when the test
    /// process exits, so this is intentional and not a bug.
    fn leak_store(store: TicketStore) -> &'static TicketStore {
        Box::leak(Box::new(store))
    }

    #[test]
    fn test_is_ticket_path() {
        assert!(is_ticket_path(Path::new("/tmp/foo/.janus/items/j-a1b2.md")));
        assert!(is_ticket_path(Path::new(".janus/items/j-a1b2.md")));
        assert!(!is_ticket_path(Path::new(
            "/tmp/foo/.janus/plans/plan-a.md"
        )));
        assert!(!is_ticket_path(Path::new("/tmp/foo/items/j-a1b2.md")));
    }

    #[test]
    fn test_is_plan_path() {
        assert!(is_plan_path(Path::new(
            "/tmp/foo/.janus/plans/plan-a1b2.md"
        )));
        assert!(is_plan_path(Path::new(".janus/plans/plan-a1b2.md")));
        assert!(!is_plan_path(Path::new("/tmp/foo/.janus/items/j-a.md")));
        assert!(!is_plan_path(Path::new("/tmp/foo/plans/plan-a.md")));
    }

    #[test]
    fn test_classify_event_kind() {
        assert!(matches!(
            classify_event_kind(EventKind::Create(notify::event::CreateKind::File)),
            FileAction::CreateOrModify
        ));
        assert!(matches!(
            classify_event_kind(EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content
            ))),
            FileAction::CreateOrModify
        ));
        assert!(matches!(
            classify_event_kind(EventKind::Remove(notify::event::RemoveKind::File)),
            FileAction::Remove
        ));
        assert!(matches!(
            classify_event_kind(EventKind::Access(notify::event::AccessKind::Read)),
            FileAction::Ignore
        ));
    }

    #[tokio::test]
    #[serial]
    async fn test_watcher_detects_ticket_creation() {
        let tmp = setup_test_dir();
        let janus_root = tmp.path().join(".janus");

        unsafe { std::env::set_var("JANUS_ROOT", janus_root.to_str().unwrap()) };

        let store = leak_store(TicketStore::empty());
        let (_watcher, mut rx) = StoreWatcher::start(store).expect("watcher should start");

        // Create a ticket file
        let ticket_path = janus_root.join("items").join("j-new1.md");
        fs::write(&ticket_path, make_ticket_content("j-new1", "New Ticket")).unwrap();

        // Wait for the watcher to process the event
        let result = timeout(Duration::from_secs(3), rx.recv()).await;
        assert!(result.is_ok(), "should receive a broadcast event");

        // Give a moment for the store to be updated
        sleep(Duration::from_millis(50)).await;

        assert!(
            store.tickets().contains_key("j-new1"),
            "store should contain the new ticket"
        );

        unsafe { std::env::remove_var("JANUS_ROOT") };
    }

    #[tokio::test]
    #[serial]
    async fn test_watcher_detects_ticket_modification() {
        let tmp = setup_test_dir();
        let janus_root = tmp.path().join(".janus");

        unsafe { std::env::set_var("JANUS_ROOT", janus_root.to_str().unwrap()) };

        // Pre-populate a ticket file and store
        let ticket_path = janus_root.join("items").join("j-mod1.md");
        fs::write(
            &ticket_path,
            make_ticket_content("j-mod1", "Original Title"),
        )
        .unwrap();

        let store = leak_store(TicketStore::init().expect("init should succeed"));
        assert_eq!(
            store.tickets().get("j-mod1").unwrap().title.as_deref(),
            Some("Original Title")
        );

        let (_watcher, mut rx) = StoreWatcher::start(store).expect("watcher should start");

        // Modify the ticket file
        fs::write(&ticket_path, make_ticket_content("j-mod1", "Updated Title")).unwrap();

        let result = timeout(Duration::from_secs(3), rx.recv()).await;
        assert!(result.is_ok(), "should receive a broadcast event");

        sleep(Duration::from_millis(50)).await;

        assert_eq!(
            store.tickets().get("j-mod1").unwrap().title.as_deref(),
            Some("Updated Title")
        );

        unsafe { std::env::remove_var("JANUS_ROOT") };
    }

    #[tokio::test]
    #[serial]
    async fn test_watcher_detects_ticket_deletion() {
        let tmp = setup_test_dir();
        let janus_root = tmp.path().join(".janus");

        unsafe { std::env::set_var("JANUS_ROOT", janus_root.to_str().unwrap()) };

        // Start the watcher FIRST on an empty store
        let store = leak_store(TicketStore::empty());
        let (_watcher, mut rx) = StoreWatcher::start(store).expect("watcher should start");

        // Give the watcher time to register with the OS
        sleep(Duration::from_millis(200)).await;

        // Create a ticket file while watcher is running
        let ticket_path = janus_root.join("items").join("j-del1.md");
        fs::write(&ticket_path, make_ticket_content("j-del1", "To Be Deleted")).unwrap();

        // Wait for creation event
        let _ = timeout(Duration::from_secs(3), rx.recv()).await;
        sleep(Duration::from_millis(100)).await;
        assert!(
            store.tickets().contains_key("j-del1"),
            "ticket should be in store after creation"
        );

        // Now delete the file
        fs::remove_file(&ticket_path).unwrap();

        // Wait for the watcher to process the deletion event.
        let deadline = Duration::from_secs(5);
        let result = timeout(deadline, rx.recv()).await;
        assert!(
            result.is_ok(),
            "should receive a broadcast event for deletion"
        );

        sleep(Duration::from_millis(100)).await;

        assert!(
            !store.tickets().contains_key("j-del1"),
            "ticket should be removed from store"
        );

        unsafe { std::env::remove_var("JANUS_ROOT") };
    }

    #[tokio::test]
    #[serial]
    async fn test_watcher_detects_plan_creation() {
        let tmp = setup_test_dir();
        let janus_root = tmp.path().join(".janus");

        unsafe { std::env::set_var("JANUS_ROOT", janus_root.to_str().unwrap()) };

        let store = leak_store(TicketStore::empty());
        let (_watcher, mut rx) = StoreWatcher::start(store).expect("watcher should start");

        // Create a plan file
        let plan_path = janus_root.join("plans").join("plan-new1.md");
        fs::write(&plan_path, make_plan_content("plan-new1", "New Plan")).unwrap();

        let result = timeout(Duration::from_secs(3), rx.recv()).await;
        assert!(result.is_ok(), "should receive a broadcast event");

        sleep(Duration::from_millis(50)).await;

        assert!(
            store.plans().contains_key("plan-new1"),
            "store should contain the new plan"
        );

        unsafe { std::env::remove_var("JANUS_ROOT") };
    }

    #[tokio::test]
    #[serial]
    async fn test_watcher_multiple_subscribers() {
        let tmp = setup_test_dir();
        let janus_root = tmp.path().join(".janus");

        unsafe { std::env::set_var("JANUS_ROOT", janus_root.to_str().unwrap()) };

        let store = leak_store(TicketStore::empty());
        let (watcher, mut rx1) = StoreWatcher::start(store).expect("watcher should start");
        let mut rx2 = watcher.subscribe();

        // Create a ticket file
        let ticket_path = janus_root.join("items").join("j-sub1.md");
        fs::write(&ticket_path, make_ticket_content("j-sub1", "Sub Test")).unwrap();

        // Both subscribers should receive the event
        let r1 = timeout(Duration::from_secs(3), rx1.recv()).await;
        let r2 = timeout(Duration::from_secs(3), rx2.recv()).await;

        assert!(r1.is_ok(), "subscriber 1 should receive event");
        assert!(r2.is_ok(), "subscriber 2 should receive event");

        unsafe { std::env::remove_var("JANUS_ROOT") };
    }

    #[tokio::test]
    #[serial]
    async fn test_watcher_debounces_rapid_writes() {
        let tmp = setup_test_dir();
        let janus_root = tmp.path().join(".janus");

        unsafe { std::env::set_var("JANUS_ROOT", janus_root.to_str().unwrap()) };

        let store = leak_store(TicketStore::empty());
        let (_watcher, mut rx) = StoreWatcher::start(store).expect("watcher should start");

        // Write the same file multiple times in rapid succession
        let ticket_path = janus_root.join("items").join("j-rapid.md");
        for i in 0..5 {
            fs::write(
                &ticket_path,
                make_ticket_content("j-rapid", &format!("Rapid Write {i}")),
            )
            .unwrap();
            sleep(Duration::from_millis(10)).await;
        }

        // Wait for debounced processing
        let result = timeout(Duration::from_secs(3), rx.recv()).await;
        assert!(result.is_ok(), "should receive at least one event");

        sleep(Duration::from_millis(100)).await;

        // The store should have the ticket with the latest content
        assert!(store.tickets().contains_key("j-rapid"));
        let title = store
            .tickets()
            .get("j-rapid")
            .unwrap()
            .title
            .clone()
            .unwrap();
        assert!(
            title.starts_with("Rapid Write"),
            "should have a title from one of the writes, got: {title}"
        );

        unsafe { std::env::remove_var("JANUS_ROOT") };
    }

    #[tokio::test]
    #[serial]
    async fn test_watcher_ignores_non_md_files() {
        let tmp = setup_test_dir();
        let janus_root = tmp.path().join(".janus");

        unsafe { std::env::set_var("JANUS_ROOT", janus_root.to_str().unwrap()) };

        let store = leak_store(TicketStore::empty());
        let (_watcher, mut rx) = StoreWatcher::start(store).expect("watcher should start");

        // Create a non-.md file
        let txt_path = janus_root.join("items").join("notes.txt");
        fs::write(&txt_path, "not a ticket").unwrap();

        // Wait briefly — we should NOT get a meaningful store update
        let result = timeout(Duration::from_millis(500), rx.recv()).await;

        // Even if we get an event notification (the watcher sees the file),
        // the store should not have any tickets
        if result.is_ok() {
            sleep(Duration::from_millis(50)).await;
        }
        assert_eq!(store.tickets().len(), 0, "non-md files should be ignored");

        unsafe { std::env::remove_var("JANUS_ROOT") };
    }

    #[test]
    #[serial]
    fn test_watcher_handles_missing_directories() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let janus_root = tmp.path().join(".janus");
        // Don't create the directories

        unsafe { std::env::set_var("JANUS_ROOT", janus_root.to_str().unwrap()) };

        let store = leak_store(TicketStore::empty());
        // The watcher should start without error even if directories don't exist
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = rt.block_on(async { StoreWatcher::start(store) });
        assert!(
            result.is_ok(),
            "watcher should start gracefully with missing dirs"
        );

        unsafe { std::env::remove_var("JANUS_ROOT") };
    }

    #[test]
    fn test_store_identity_differs_for_different_instances() {
        let store_a = leak_store(TicketStore::empty());
        let store_b = leak_store(TicketStore::empty());

        let id_a = super::store_identity(store_a);
        let id_b = super::store_identity(store_b);

        assert_ne!(
            id_a, id_b,
            "different store instances should have different identities"
        );

        // Same store should have the same identity
        assert_eq!(
            super::store_identity(store_a),
            id_a,
            "same store should return consistent identity"
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_start_watching_rejects_different_store() {
        let tmp = setup_test_dir();
        let janus_root = tmp.path().join(".janus");

        unsafe { std::env::set_var("JANUS_ROOT", janus_root.to_str().unwrap()) };

        // Reset the global WATCHER singleton for this test by using
        // StoreWatcher::start directly and calling the global start_watching.
        // Since OnceLock can only be set once per process, we test the
        // mismatch detection by calling start_watching with two different stores
        // in sequence. The first call initializes; the second should fail.
        //
        // Note: Because OnceLock persists across tests in the same process,
        // this test may interact with other tests that call start_watching.
        // The #[serial] attribute ensures no concurrency issues.

        let store_a = leak_store(TicketStore::empty());
        let store_b = leak_store(TicketStore::empty());

        // First call: initializes the watcher (or re-uses if already set by another test)
        let result_a = start_watching(store_a).await;

        if result_a.is_ok() {
            // We won the initialization — verify that a different store is rejected
            let result_b = start_watching(store_b).await;
            assert!(
                result_b.is_err(),
                "start_watching with a different store should return an error"
            );
            let err_msg = result_b.unwrap_err().to_string();
            assert!(
                err_msg.contains("different TicketStore instance"),
                "error should mention store mismatch, got: {err_msg}"
            );

            // Idempotent re-initialization with the same store should succeed
            let result_a2 = start_watching(store_a).await;
            assert!(
                result_a2.is_ok(),
                "re-initializing with the same store should succeed"
            );
        } else {
            // Another test already initialized WATCHER with a different store.
            // Verify both stores are rejected (since neither matches the bound one).
            let result_b = start_watching(store_b).await;
            assert!(
                result_b.is_err(),
                "both stores should be rejected when WATCHER is bound to a third"
            );
        }

        unsafe { std::env::remove_var("JANUS_ROOT") };
    }
}

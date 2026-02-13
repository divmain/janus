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
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, OnceLock};

use dashmap::DashMap;
use notify::{EventKind, RecursiveMode, Watcher};
use tokio::sync::{Semaphore, broadcast};

use crate::error::{JanusError, Result};
use crate::plan::parser::parse_plan_content;
use crate::ticket::parse_ticket;
use crate::types::janus_root;

use super::TicketStore;

/// Global map tracking tickets recently edited by the TUI itself.
/// Used to suppress redundant watcher broadcasts for self-initiated changes.
/// Maps ticket ID to the timestamp when it was marked as recently edited.
static RECENTLY_EDITED: OnceLock<Arc<DashMap<String, std::time::Instant>>> = OnceLock::new();

/// Get or initialize the recently edited tracking map.
fn get_recently_edited() -> Arc<DashMap<String, std::time::Instant>> {
    RECENTLY_EDITED
        .get_or_init(|| Arc::new(DashMap::new()))
        .clone()
}

/// Mark a ticket ID as "recently edited" by the TUI.
/// This suppresses watcher broadcasts for this ticket for RECENTLY_EDITED_TTL seconds.
pub fn mark_recently_edited(ticket_id: &str) {
    let map = get_recently_edited();
    map.insert(ticket_id.to_string(), std::time::Instant::now());
}

/// Check if a ticket ID was recently edited (and the TTL hasn't expired).
fn is_recently_edited(ticket_id: &str) -> bool {
    let map = get_recently_edited();

    // Clean up expired entries while we're checking
    let now = std::time::Instant::now();
    let expired: Vec<String> = map
        .iter()
        .filter(|e| now.duration_since(*e.value()) > RECENTLY_EDITED_TTL)
        .map(|e| e.key().clone())
        .collect();
    for key in expired {
        map.remove(&key);
    }

    // Check if this ticket is still in the map
    if let Some(entry) = map.get(ticket_id) {
        return now.duration_since(*entry.value()) <= RECENTLY_EDITED_TTL;
    }

    false
}

/// Duration to wait for additional events before processing a batch.
const DEBOUNCE_DURATION: std::time::Duration = std::time::Duration::from_millis(150);

/// Capacity of the bounded channel bridging `notify` events to the tokio
/// event loop. When the channel is full, the watcher callback sets a
/// "rescan needed" flag instead of enqueuing individual events.
const CHANNEL_CAPACITY: usize = 512;

/// Maximum number of entries in the pending event map. When this cap is
/// exceeded, the map is cleared and a full rescan is performed instead.
const PENDING_CAP: usize = 1024;

/// Delay before retrying a file that failed to parse (likely mid-write).
const RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(300);

/// Maximum number of retry attempts per file before giving up.
const MAX_RETRY_ATTEMPTS: u8 = 3;

/// Time-to-live for recently edited entries. Tickets marked as recently edited
/// will suppress watcher broadcasts for this duration to prevent redundant UI updates.
const RECENTLY_EDITED_TTL: std::time::Duration = std::time::Duration::from_secs(2);

/// Semaphore to limit concurrent embedding generation tasks.
/// Prevents unbounded task spawning when many files change simultaneously.
static EMBEDDING_SEMAPHORE: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(4));

/// Notification event sent when the store is updated by the watcher.
#[derive(Debug, Clone)]
pub enum StoreEvent {
    /// One or more tickets were created, modified, or deleted.
    TicketsChanged,
    /// One or more plans were created, modified, or deleted.
    PlansChanged,
}

/// Tracks retry state for a file that failed to parse.
#[derive(Debug)]
struct RetryEntry {
    /// Number of attempts made so far (including the initial failure).
    attempts: u8,
    /// When the next retry should be attempted.
    next_retry: tokio::time::Instant,
    /// Whether this file is a ticket (true) or plan (false).
    is_ticket: bool,
}

/// Queue of files pending retry after parse failure.
///
/// Files may fail to parse when caught mid-write by the editor. This queue
/// schedules bounded retries with a short delay. Warnings are rate-limited:
/// only the first failure and the final give-up are logged per file.
struct RetryQueue {
    entries: HashMap<PathBuf, RetryEntry>,
}

impl RetryQueue {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Schedule a retry for a file that failed to parse.
    /// Returns `true` if the retry was scheduled, `false` if max attempts exceeded.
    fn schedule(&mut self, path: PathBuf, is_ticket: bool) -> bool {
        let entry = self.entries.entry(path.clone()).or_insert(RetryEntry {
            attempts: 0,
            next_retry: tokio::time::Instant::now(),
            is_ticket,
        });
        entry.attempts += 1;
        if entry.attempts > MAX_RETRY_ATTEMPTS {
            // Final failure — log and remove from queue
            eprintln!(
                "Warning: giving up on parsing {} after {MAX_RETRY_ATTEMPTS} attempts: {}",
                if is_ticket { "ticket" } else { "plan" },
                path.display()
            );
            self.entries.remove(&path);
            return false;
        }
        if entry.attempts == 1 {
            // First failure — log a warning
            eprintln!(
                "Warning: failed to parse {} (will retry): {}",
                if is_ticket { "ticket" } else { "plan" },
                path.display()
            );
        }
        entry.next_retry = tokio::time::Instant::now() + RETRY_DELAY;
        true
    }

    /// Remove a path from the retry queue (e.g., when a new event supersedes it).
    fn cancel(&mut self, path: &Path) {
        self.entries.remove(path);
    }

    /// Returns the earliest deadline among pending retries, if any.
    fn next_deadline(&self) -> Option<tokio::time::Instant> {
        self.entries.values().map(|e| e.next_retry).min()
    }

    /// Drain all entries whose retry deadline has passed.
    fn take_ready(&mut self) -> Vec<(PathBuf, bool)> {
        let now = tokio::time::Instant::now();
        let ready: Vec<PathBuf> = self
            .entries
            .iter()
            .filter(|(_, e)| e.next_retry <= now)
            .map(|(p, _)| p.clone())
            .collect();
        ready
            .into_iter()
            .map(|p| {
                let is_ticket = self.entries.get(&p).unwrap().is_ticket;
                (p, is_ticket)
            })
            .collect()
    }
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
        let (bridge_tx, bridge_rx) = tokio::sync::mpsc::channel::<notify::Event>(CHANNEL_CAPACITY);

        // Shared flag: when the bounded channel is full, the notify callback
        // sets this to `true` instead of blocking. The event loop checks the
        // flag before each batch and performs a full rescan when set.
        let rescan_needed = Arc::new(AtomicBool::new(false));

        // Create the notify watcher with a callback that bridges to tokio
        let watcher = {
            let tx = bridge_tx;
            let rescan = Arc::clone(&rescan_needed);
            notify::RecommendedWatcher::new(
                move |res: std::result::Result<notify::Event, notify::Error>| match res {
                    Ok(event) => {
                        if tx.try_send(event).is_err() {
                            // Channel full — flag a rescan instead of dropping events silently
                            if !rescan.swap(true, Ordering::Relaxed) {
                                eprintln!(
                                    "Warning: watcher channel full (capacity {CHANNEL_CAPACITY}), \
                                     coalescing into full rescan"
                                );
                            }
                        }
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
        let task_rescan = Arc::clone(&rescan_needed);
        tokio::spawn(async move {
            run_event_loop(bridge_rx, store, task_tx, task_rescan).await;
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
///
/// When the `rescan_needed` flag is set (because the bounded channel was
/// full), the loop skips per-file processing and performs a full store
/// rescan instead.
///
/// Files that fail to parse (e.g., caught mid-write) are scheduled for
/// bounded retries via a `RetryQueue`. The retry deadline is incorporated
/// into the event loop's sleep so retries fire promptly without busy-waiting.
async fn run_event_loop(
    mut bridge_rx: tokio::sync::mpsc::Receiver<notify::Event>,
    store: &'static TicketStore,
    broadcast_tx: broadcast::Sender<StoreEvent>,
    rescan_needed: Arc<AtomicBool>,
) {
    // Accumulate events keyed by path → last event kind
    let mut pending: HashMap<PathBuf, EventKind> = HashMap::new();
    let mut retries = RetryQueue::new();

    loop {
        // Determine how long to wait: either until the next retry deadline
        // or indefinitely if no retries are pending.
        let wait_result = if let Some(deadline) = retries.next_deadline() {
            // Wait for either a new event or the retry deadline
            tokio::select! {
                event = bridge_rx.recv() => event,
                _ = tokio::time::sleep_until(deadline) => {
                    // Retry deadline reached — process retries, then continue loop
                    process_retries(&mut retries, store, &broadcast_tx).await;
                    continue;
                }
            }
        } else {
            bridge_rx.recv().await
        };

        // Wait for the first event (blocks until something happens)
        let event = match wait_result {
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
                    // Check pending cap while draining
                    if pending.len() > PENDING_CAP {
                        eprintln!(
                            "Warning: pending event map exceeded cap ({PENDING_CAP} entries), \
                             coalescing into full rescan"
                        );
                        pending.clear();
                        rescan_needed.store(true, Ordering::Relaxed);
                    }
                }
                Ok(None) => {
                    // Channel closed — process remaining work and exit
                    if rescan_needed.swap(false, Ordering::Relaxed) {
                        pending.clear();
                        full_rescan(store, &broadcast_tx).await;
                    } else {
                        process_batch(&mut pending, store, &broadcast_tx, &mut retries).await;
                    }
                    return;
                }
                Err(_) => {
                    // Timeout — debounce window elapsed, process the batch
                    break;
                }
            }
        }

        if rescan_needed.swap(false, Ordering::Relaxed) {
            pending.clear();
            retries.entries.clear();
            full_rescan(store, &broadcast_tx).await;
        } else {
            process_batch(&mut pending, store, &broadcast_tx, &mut retries).await;
        }
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
///
/// Files that fail to parse are scheduled for retry in the `RetryQueue`.
/// A fresh event for a path cancels any pending retry for that path
/// (the new content supersedes the old attempt).
///
/// Note: Tickets marked as "recently edited" (within RECENTLY_EDITED_TTL seconds
/// by the TUI itself) will not trigger a broadcast, preventing redundant UI updates.
async fn process_batch(
    pending: &mut HashMap<PathBuf, EventKind>,
    store: &'static TicketStore,
    broadcast_tx: &broadcast::Sender<StoreEvent>,
    retries: &mut RetryQueue,
) {
    if pending.is_empty() {
        return;
    }

    let mut changed_ticket_ids: Vec<String> = Vec::new();
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

        // A new event for this path supersedes any pending retry
        retries.cancel(&path);

        match classify_event_kind(kind) {
            FileAction::CreateOrModify => {
                if is_ticket {
                    match process_ticket_file(&path, store).await {
                        ParseOutcome::Success => {
                            if let Some(id) = path.file_stem() {
                                changed_ticket_ids.push(id.to_string_lossy().to_string());
                            }
                        }
                        ParseOutcome::ParseFailed => {
                            retries.schedule(path, true);
                        }
                        ParseOutcome::Skipped => {}
                    }
                } else if is_plan {
                    match process_plan_file(&path, store).await {
                        ParseOutcome::Success => plans_changed = true,
                        ParseOutcome::ParseFailed => {
                            retries.schedule(path, false);
                        }
                        ParseOutcome::Skipped => {}
                    }
                }
            }
            FileAction::Remove => {
                if let Some(id) = path.file_stem().map(|s| s.to_string_lossy().to_string()) {
                    if is_ticket {
                        store.remove_ticket(&id);
                        changed_ticket_ids.push(id);
                    } else if is_plan {
                        store.remove_plan(&id);
                        plans_changed = true;
                    }
                }
            }
            FileAction::Ignore => {}
        }
    }

    // Filter out recently edited tickets (suppressed by TUI-initiated changes)
    let non_suppressed_ids: Vec<String> = changed_ticket_ids
        .into_iter()
        .filter(|id| !is_recently_edited(id))
        .collect();

    // Only broadcast if there are non-suppressed ticket changes
    if !non_suppressed_ids.is_empty() {
        // If only a single ticket changed and it's not suppressed, we could
        // emit a more specific event in the future. For now, use the generic event.
        let _ = broadcast_tx.send(StoreEvent::TicketsChanged);
    }
    // Note: Suppressed broadcasts (for recently edited tickets) are intentionally silent
    // to avoid interfering with the TUI display.

    if plans_changed {
        let _ = broadcast_tx.send(StoreEvent::PlansChanged);
    }
}

/// Process pending retries for files that previously failed to parse.
///
/// Attempts to re-parse each file whose retry deadline has passed.
/// Successful parses are removed from the queue; failures are re-scheduled
/// (up to `MAX_RETRY_ATTEMPTS`).
async fn process_retries(
    retries: &mut RetryQueue,
    store: &'static TicketStore,
    broadcast_tx: &broadcast::Sender<StoreEvent>,
) {
    let ready = retries.take_ready();
    if ready.is_empty() {
        return;
    }

    let mut changed_ticket_ids: Vec<String> = Vec::new();
    let mut plans_changed = false;

    for (path, is_ticket) in ready {
        let outcome = if is_ticket {
            process_ticket_file(&path, store).await
        } else {
            process_plan_file(&path, store).await
        };

        match outcome {
            ParseOutcome::Success => {
                retries.cancel(&path);
                if is_ticket {
                    if let Some(id) = path.file_stem() {
                        changed_ticket_ids.push(id.to_string_lossy().to_string());
                    }
                } else {
                    plans_changed = true;
                }
            }
            ParseOutcome::ParseFailed => {
                // schedule() handles attempt counting and final warning
                retries.schedule(path, is_ticket);
            }
            ParseOutcome::Skipped => {
                // File disappeared or IO error — stop retrying
                retries.cancel(&path);
            }
        }
    }

    // Filter out recently edited tickets (suppressed by TUI-initiated changes)
    let non_suppressed_ids: Vec<String> = changed_ticket_ids
        .into_iter()
        .filter(|id| !is_recently_edited(id))
        .collect();

    if !non_suppressed_ids.is_empty() {
        let _ = broadcast_tx.send(StoreEvent::TicketsChanged);
    }
    if plans_changed {
        let _ = broadcast_tx.send(StoreEvent::PlansChanged);
    }
}

/// Perform a full rescan of all ticket and plan files on disk.
///
/// This is the fallback when the bounded channel overflows or the pending
/// map exceeds its cap. Instead of processing individual events, we
/// re-read every `.md` file and reconcile the store, then broadcast
/// change notifications for both tickets and plans.
///
/// Parse failures during a full rescan are logged but not retried — the
/// retry queue is cleared before a rescan since we're reading everything
/// fresh.
async fn full_rescan(store: &'static TicketStore, broadcast_tx: &broadcast::Sender<StoreEvent>) {
    use crate::types::{plans_dir, tickets_items_dir};

    eprintln!("Warning: performing full rescan of .janus/ directory");

    let items_dir = tickets_items_dir();
    if items_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&items_dir) {
            // Collect IDs currently on disk so we can detect deletions
            let mut disk_ids = std::collections::HashSet::new();
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "md") {
                    if let Some(stem) = path.file_stem() {
                        disk_ids.insert(stem.to_string_lossy().to_string());
                    }
                    // Best-effort parse during rescan; failures are not retried
                    let _ = process_ticket_file(&path, store).await;
                }
            }
            // Remove tickets no longer on disk
            let store_ids: Vec<String> = store.tickets().iter().map(|r| r.key().clone()).collect();
            for id in store_ids {
                if !disk_ids.contains(&id) {
                    store.remove_ticket(&id);
                }
            }
        }
    }

    let p_dir = plans_dir();
    if p_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&p_dir) {
            let mut disk_ids = std::collections::HashSet::new();
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "md") {
                    if let Some(stem) = path.file_stem() {
                        disk_ids.insert(stem.to_string_lossy().to_string());
                    }
                    // Best-effort parse during rescan; failures are not retried
                    let _ = process_plan_file(&path, store).await;
                }
            }
            let store_ids: Vec<String> = store.plans().iter().map(|r| r.key().clone()).collect();
            for id in store_ids {
                if !disk_ids.contains(&id) {
                    store.remove_plan(&id);
                }
            }
        }
    }

    let _ = broadcast_tx.send(StoreEvent::TicketsChanged);
    let _ = broadcast_tx.send(StoreEvent::PlansChanged);
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

/// Outcome of attempting to parse and upsert a file into the store.
enum ParseOutcome {
    /// File was parsed and the store was updated.
    Success,
    /// File content could not be parsed (e.g., mid-write truncation).
    /// The store was NOT modified — last-known-good state is preserved.
    ParseFailed,
    /// File could not be read (IO error, not found) or was otherwise
    /// skipped. No retry is warranted.
    Skipped,
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

/// Read and parse a ticket file, updating the store.
///
/// Returns a `ParseOutcome` indicating whether the store was updated,
/// the parse failed (eligible for retry), or the file was skipped.
///
/// On parse failure, the store is **not** modified — the last-known-good
/// state is preserved so callers can schedule a retry.
///
/// If the file no longer exists (race with deletion), removes the ticket
/// from the store and returns `Skipped` (no retry needed).
async fn process_ticket_file(path: &Path, store: &TicketStore) -> ParseOutcome {
    let content = match tokio::fs::read_to_string(path).await {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // File was deleted between event and processing — treat as removal.
            // Return Success because the store was modified (ticket removed).
            if let Some(stem) = path.file_stem() {
                store.remove_ticket(&stem.to_string_lossy());
            }
            return ParseOutcome::Success;
        }
        Err(e) => {
            eprintln!(
                "Warning: failed to read ticket file {}: {e}",
                path.display()
            );
            return ParseOutcome::Skipped;
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

            // Remove stale embedding and regenerate
            if let Some(id) = &ticket_id {
                store.embeddings().remove(id.as_ref());

                // Generate new embedding asynchronously with concurrency limit
                // Acquire semaphore permit to prevent unbounded task spawning
                let id_clone = id.clone();
                tokio::spawn(async move {
                    // Acquire permit - waits if 4 tasks are already running
                    let permit = EMBEDDING_SEMAPHORE.acquire().await;
                    let _permit = match permit {
                        Ok(p) => p,
                        Err(e) => {
                            tracing::warn!("Failed to acquire embedding semaphore: {e}");
                            return;
                        }
                    };

                    // Get the global store singleton
                    if let Ok(store) = crate::store::get_or_init_store().await {
                        // Log embedding failures for production visibility
                        if let Err(e) = store.ensure_embedding(&id_clone).await {
                            tracing::warn!("Failed to generate embedding for {id_clone}: {e}");
                        }
                    }
                });
            }
            ParseOutcome::Success
        }
        Err(_) => {
            // Don't log here — the RetryQueue handles rate-limited warnings.
            // The store is intentionally NOT modified, preserving last-known-good state.
            ParseOutcome::ParseFailed
        }
    }
}

/// Read and parse a plan file, updating the store.
///
/// Returns a `ParseOutcome` indicating whether the store was updated,
/// the parse failed (eligible for retry), or the file was skipped.
///
/// On parse failure, the store is **not** modified — the last-known-good
/// state is preserved so callers can schedule a retry.
///
/// If the file no longer exists (race with deletion), removes the plan
/// from the store and returns `Skipped` (no retry needed).
async fn process_plan_file(path: &Path, store: &TicketStore) -> ParseOutcome {
    let content = match tokio::fs::read_to_string(path).await {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // File was deleted between event and processing — treat as removal.
            // Return Success because the store was modified (plan removed).
            if let Some(stem) = path.file_stem() {
                store.remove_plan(&stem.to_string_lossy());
            }
            return ParseOutcome::Success;
        }
        Err(e) => {
            eprintln!("Warning: failed to read plan file {}: {e}", path.display());
            return ParseOutcome::Skipped;
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
            ParseOutcome::Success
        }
        Err(_) => {
            // Don't log here — the RetryQueue handles rate-limited warnings.
            // The store is intentionally NOT modified, preserving last-known-good state.
            ParseOutcome::ParseFailed
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
    use crate::paths::JanusRootGuard;
    use crate::store::TicketStore;
    use crate::store::test_helpers::{make_plan_content, make_ticket_content};

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
    async fn test_watcher_detects_ticket_creation() {
        let tmp = setup_test_dir();
        let janus_root = tmp.path().join(".janus");

        let _guard = JanusRootGuard::new(&janus_root);

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
    }

    #[tokio::test]
    async fn test_watcher_detects_ticket_modification() {
        let tmp = setup_test_dir();
        let janus_root = tmp.path().join(".janus");

        let _guard = JanusRootGuard::new(&janus_root);

        // Pre-populate a ticket file and store
        let ticket_path = janus_root.join("items").join("j-mod1.md");
        fs::write(
            &ticket_path,
            make_ticket_content("j-mod1", "Original Title"),
        )
        .unwrap();

        let store = leak_store(TicketStore::init().await.expect("init should succeed"));
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
    }

    #[tokio::test]
    async fn test_watcher_detects_ticket_deletion() {
        let tmp = setup_test_dir();
        let janus_root = tmp.path().join(".janus");

        let _guard = JanusRootGuard::new(&janus_root);

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
    }

    #[tokio::test]
    async fn test_watcher_detects_plan_creation() {
        let tmp = setup_test_dir();
        let janus_root = tmp.path().join(".janus");

        let _guard = JanusRootGuard::new(&janus_root);

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
    }

    #[tokio::test]
    async fn test_watcher_multiple_subscribers() {
        let tmp = setup_test_dir();
        let janus_root = tmp.path().join(".janus");

        let _guard = JanusRootGuard::new(&janus_root);

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
    }

    #[tokio::test]
    async fn test_watcher_debounces_rapid_writes() {
        let tmp = setup_test_dir();
        let janus_root = tmp.path().join(".janus");

        let _guard = JanusRootGuard::new(&janus_root);

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
    }

    #[tokio::test]
    async fn test_watcher_ignores_non_md_files() {
        let tmp = setup_test_dir();
        let janus_root = tmp.path().join(".janus");

        let _guard = JanusRootGuard::new(&janus_root);

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
    }

    #[test]
    fn test_watcher_handles_missing_directories() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let janus_root = tmp.path().join(".janus");
        // Don't create the directories

        let _guard = JanusRootGuard::new(&janus_root);

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

        let _guard = JanusRootGuard::new(&janus_root);

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
    }
}

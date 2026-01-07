//! Cache performance benchmarks
//!
//! These benchmarks measure the performance of cache operations in realistic scenarios:
//!
//! - **Warm cache sync (no changes)**: Most common case - cache validates against filesystem
//! - **Incremental sync**: A few files changed since last sync
//! - **Cold cache sync**: Full rebuild from scratch (worst case)
//! - **Query operations**: get_all_tickets, get_ticket, find_by_partial_id
//!
//! Expected performance targets (from PLAN_CACHE.md):
//! - Warm cache sync (no changes): ~25ms for 10k tickets
//! - Incremental sync (10 files): ~35-45ms
//! - Query operations: <5ms

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use janus::cache::TicketCache;
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use tempfile::TempDir;
use tokio::runtime::Runtime;

/// Global mutex to prevent parallel benchmarks from interfering with each other
/// since they change the current working directory
static BENCH_MUTEX: Mutex<()> = Mutex::new(());

fn create_test_tickets(dir: &Path, count: usize) {
    for i in 0..count {
        let ticket_id = format!("j-{:04x}", i);
        let content = format!(
            "---
id: {}
status: new
type: task
priority: 2
assignee: Test User
created: 2024-01-01T00:00:00Z
deps: []
links: []
---
# Ticket {}
",
            ticket_id, i
        );
        let ticket_path = dir.join(format!("{}.md", ticket_id));
        fs::write(ticket_path, content).unwrap();
    }
}

fn modify_ticket(dir: &Path, ticket_id: &str) {
    let ticket_path = dir.join(format!("{}.md", ticket_id));
    let content = format!(
        "---
id: {}
status: in_progress
type: task
priority: 1
assignee: Modified User
created: 2024-01-01T00:00:00Z
deps: []
links: []
---
# Modified Ticket
",
        ticket_id
    );
    fs::write(ticket_path, content).unwrap();
}

/// Benchmark: Warm cache sync with NO changes
///
/// This is the most common case: cache is already populated, we just need to
/// verify nothing changed by scanning mtimes.
///
/// Expected: ~25ms for 10k tickets (dominated by directory stat calls)
fn bench_warm_cache_sync_no_changes(c: &mut Criterion) {
    let _lock = BENCH_MUTEX.lock().unwrap();

    let mut group = c.benchmark_group("warm_sync_no_changes");

    for size in [100, 1000, 5000].iter() {
        // Setup: Create temp dir, tickets, and warm the cache
        let temp = TempDir::new().unwrap();
        let repo_path = temp.path().join(format!("bench_warm_{}", size));
        fs::create_dir_all(&repo_path).unwrap();
        let items_dir = repo_path.join(".janus").join("items");
        fs::create_dir_all(&items_dir).unwrap();

        create_test_tickets(&items_dir, *size);

        std::env::set_current_dir(&repo_path).unwrap();

        // Create a shared runtime for the benchmark
        let rt = Runtime::new().unwrap();

        // Warm the cache (do the initial sync)
        rt.block_on(async {
            let mut cache = TicketCache::open().await.unwrap();
            cache.sync().await.unwrap();
        });

        // Benchmark: Sync with no changes (should just scan mtimes)
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                rt.block_on(async {
                    let mut cache = TicketCache::open().await.unwrap();
                    black_box(cache.sync().await.unwrap())
                })
            });
        });
    }

    group.finish();
}

/// Benchmark: Incremental sync with a few files changed
///
/// Simulates typical usage: most files unchanged, a few modified.
///
/// Expected: ~35-45ms for 10k tickets with 10 changes
fn bench_incremental_sync(c: &mut Criterion) {
    let _lock = BENCH_MUTEX.lock().unwrap();

    let mut group = c.benchmark_group("incremental_sync");

    for size in [100, 1000, 5000].iter() {
        // Setup
        let temp = TempDir::new().unwrap();
        let repo_path = temp.path().join(format!("bench_incr_{}", size));
        fs::create_dir_all(&repo_path).unwrap();
        let items_dir = repo_path.join(".janus").join("items");
        fs::create_dir_all(&items_dir).unwrap();

        create_test_tickets(&items_dir, *size);

        std::env::set_current_dir(&repo_path).unwrap();

        let rt = Runtime::new().unwrap();

        // Initial sync
        rt.block_on(async {
            let mut cache = TicketCache::open().await.unwrap();
            cache.sync().await.unwrap();
        });

        // Benchmark: Modify 5 tickets then sync
        let items_dir_clone = items_dir.clone();
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                // Modify 5 tickets
                for i in 0..5 {
                    modify_ticket(&items_dir_clone, &format!("j-{:04x}", i));
                }

                rt.block_on(async {
                    let mut cache = TicketCache::open().await.unwrap();
                    black_box(cache.sync().await.unwrap())
                })
            });
        });
    }

    group.finish();
}

/// Benchmark: Cold cache sync (full rebuild)
///
/// Worst case: cache doesn't exist or is cleared, all files must be read.
///
/// Expected: ~1-5s for 10k tickets
fn bench_cold_cache_sync(c: &mut Criterion) {
    let _lock = BENCH_MUTEX.lock().unwrap();

    let mut group = c.benchmark_group("cold_sync");
    // Use fewer samples for cold sync since it's slow
    group.sample_size(20);

    for size in [100, 500, 1000].iter() {
        // Setup: Create tickets but DON'T warm the cache
        let temp = TempDir::new().unwrap();
        let repo_path = temp.path().join(format!("bench_cold_{}", size));
        fs::create_dir_all(&repo_path).unwrap();
        let items_dir = repo_path.join(".janus").join("items");
        fs::create_dir_all(&items_dir).unwrap();

        create_test_tickets(&items_dir, *size);

        std::env::set_current_dir(&repo_path).unwrap();

        let rt = Runtime::new().unwrap();

        // Get cache path to delete between iterations
        let cache_db_path = rt.block_on(async {
            let cache = TicketCache::open().await.unwrap();
            cache.cache_db_path()
        });

        // Benchmark: Full cold sync (delete cache, then sync)
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                // Delete cache to force cold sync
                let _ = fs::remove_file(&cache_db_path);

                rt.block_on(async {
                    let mut cache = TicketCache::open().await.unwrap();
                    black_box(cache.sync().await.unwrap())
                })
            });
        });
    }

    group.finish();
}

/// Benchmark: get_all_tickets query
///
/// Expected: <5ms (just SQLite query, no file I/O)
fn bench_get_all_tickets(c: &mut Criterion) {
    let _lock = BENCH_MUTEX.lock().unwrap();

    let mut group = c.benchmark_group("get_all_tickets");

    for size in [100, 1000, 5000].iter() {
        let temp = TempDir::new().unwrap();
        let repo_path = temp.path().join(format!("bench_get_all_{}", size));
        fs::create_dir_all(&repo_path).unwrap();
        let items_dir = repo_path.join(".janus").join("items");
        fs::create_dir_all(&items_dir).unwrap();

        create_test_tickets(&items_dir, *size);

        std::env::set_current_dir(&repo_path).unwrap();

        let rt = Runtime::new().unwrap();

        // Warm the cache
        rt.block_on(async {
            let mut cache = TicketCache::open().await.unwrap();
            cache.sync().await.unwrap();
        });

        // Benchmark: Query all tickets (no sync, just query)
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                rt.block_on(async {
                    let cache = TicketCache::open().await.unwrap();
                    black_box(cache.get_all_tickets().await.unwrap())
                })
            });
        });
    }

    group.finish();
}

/// Benchmark: get_ticket by exact ID
///
/// Expected: <5ms (single row SQLite query)
fn bench_get_ticket(c: &mut Criterion) {
    let _lock = BENCH_MUTEX.lock().unwrap();

    let mut group = c.benchmark_group("get_ticket");

    for size in [100, 1000, 5000].iter() {
        let temp = TempDir::new().unwrap();
        let repo_path = temp.path().join(format!("bench_get_{}", size));
        fs::create_dir_all(&repo_path).unwrap();
        let items_dir = repo_path.join(".janus").join("items");
        fs::create_dir_all(&items_dir).unwrap();

        create_test_tickets(&items_dir, *size);

        std::env::set_current_dir(&repo_path).unwrap();

        let rt = Runtime::new().unwrap();

        // Warm the cache
        rt.block_on(async {
            let mut cache = TicketCache::open().await.unwrap();
            cache.sync().await.unwrap();
        });

        // Benchmark: Get single ticket by ID
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                rt.block_on(async {
                    let cache = TicketCache::open().await.unwrap();
                    black_box(cache.get_ticket("j-0050").await.unwrap())
                })
            });
        });
    }

    group.finish();
}

/// Benchmark: find_by_partial_id
///
/// Expected: <5ms (LIKE query on indexed column)
fn bench_find_by_partial_id(c: &mut Criterion) {
    let _lock = BENCH_MUTEX.lock().unwrap();

    let mut group = c.benchmark_group("find_by_partial_id");

    for size in [100, 1000, 5000].iter() {
        let temp = TempDir::new().unwrap();
        let repo_path = temp.path().join(format!("bench_find_{}", size));
        fs::create_dir_all(&repo_path).unwrap();
        let items_dir = repo_path.join(".janus").join("items");
        fs::create_dir_all(&items_dir).unwrap();

        create_test_tickets(&items_dir, *size);

        std::env::set_current_dir(&repo_path).unwrap();

        let rt = Runtime::new().unwrap();

        // Warm the cache
        rt.block_on(async {
            let mut cache = TicketCache::open().await.unwrap();
            cache.sync().await.unwrap();
        });

        // Benchmark: Find by partial ID
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                rt.block_on(async {
                    let cache = TicketCache::open().await.unwrap();
                    black_box(cache.find_by_partial_id("j-00").await.unwrap())
                })
            });
        });
    }

    group.finish();
}

/// Benchmark: build_ticket_map
///
/// Expected: Similar to get_all_tickets + HashMap construction
fn bench_build_ticket_map(c: &mut Criterion) {
    let _lock = BENCH_MUTEX.lock().unwrap();

    let mut group = c.benchmark_group("build_ticket_map");

    for size in [100, 1000, 5000].iter() {
        let temp = TempDir::new().unwrap();
        let repo_path = temp.path().join(format!("bench_map_{}", size));
        fs::create_dir_all(&repo_path).unwrap();
        let items_dir = repo_path.join(".janus").join("items");
        fs::create_dir_all(&items_dir).unwrap();

        create_test_tickets(&items_dir, *size);

        std::env::set_current_dir(&repo_path).unwrap();

        let rt = Runtime::new().unwrap();

        // Warm the cache
        rt.block_on(async {
            let mut cache = TicketCache::open().await.unwrap();
            cache.sync().await.unwrap();
        });

        // Benchmark: Build ticket map
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                rt.block_on(async {
                    let cache = TicketCache::open().await.unwrap();
                    black_box(cache.build_ticket_map().await.unwrap())
                })
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_warm_cache_sync_no_changes,
    bench_incremental_sync,
    bench_cold_cache_sync,
    bench_get_all_tickets,
    bench_get_ticket,
    bench_find_by_partial_id,
    bench_build_ticket_map,
);
criterion_main!(benches);

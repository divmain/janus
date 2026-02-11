use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use crate::cache::TicketStore;
use crate::embedding::model::EMBEDDING_DIMENSIONS;
use crate::types::janus_root;

/// Directory name for embedding storage within the Janus root.
const EMBEDDINGS_DIR: &str = "embeddings";

/// Return the path to the embeddings directory.
fn embeddings_dir() -> std::path::PathBuf {
    janus_root().join(EMBEDDINGS_DIR)
}

impl TicketStore {
    /// Compute the embedding key for a ticket file.
    ///
    /// The key is `hex(blake3(file_path_string + ":" + mtime_ns_string))`.
    /// This produces a content-addressable key that changes when the file is modified.
    ///
    /// # Filesystem Precision Caveat
    ///
    /// This key relies on file modification time (mtime) at nanosecond granularity,
    /// but filesystem timestamp precision varies:
    ///
    /// - **APFS** (modern macOS): nanosecond precision
    /// - **ext4** (Linux): nanosecond precision
    /// - **HFS+** (older macOS): 1-second precision
    /// - **FAT32**: 2-second precision
    /// - **NFS**: varies, often second-only
    ///
    /// On filesystems with low-precision timestamps, rapid successive edits within
    /// the precision window will not change the mtime, so the embedding key remains
    /// the same and stale embeddings may be served for modified content. A truly
    /// robust fix would hash file content instead of mtime, but that defeats the
    /// purpose of the fast mtime-based cache invalidation check.
    ///
    pub fn embedding_key(file_path: &Path, mtime_ns: u128) -> String {
        // Use a repo-relative path for stability. file_path.display() can vary
        // between runs depending on absolute/relative handling, symlink resolution,
        // and platform formatting—causing cache misses and orphaned .bin files.
        let root = janus_root();
        let relative = file_path.strip_prefix(&root).unwrap_or(file_path);
        // Use forward slashes for cross-platform consistency
        let stable_path: String = relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        let input = format!("{stable_path}:{mtime_ns}");
        let hash = blake3::hash(input.as_bytes());
        hash.to_hex().to_string()
    }

    /// Load all embeddings from `.janus/embeddings/` for current tickets.
    ///
    /// For each ticket, computes the expected key from file_path + mtime,
    /// checks if `.janus/embeddings/{key}.bin` exists, and loads it into
    /// the embeddings DashMap.
    ///
    /// As a secondary validation, embeddings whose dimension count does not
    /// match [`EMBEDDING_DIMENSIONS`] are silently skipped. This guards
    /// against loading corrupted or incompatible `.bin` files (e.g., from a
    /// model change).
    pub fn load_embeddings(&self) {
        self.embeddings().clear();

        let emb_dir = embeddings_dir();
        if !emb_dir.exists() {
            return;
        }

        let expected_bytes = EMBEDDING_DIMENSIONS * 4;

        // Snapshot the ticket data we need (id + file_path) into a local Vec,
        // so that all tickets DashMap shard locks are released before we touch
        // the embeddings DashMap. This prevents AB/BA deadlocks between the
        // two maps under concurrent access.
        let ticket_info: Vec<(String, std::path::PathBuf)> = self
            .tickets()
            .iter()
            .filter_map(|entry| {
                let ticket = entry.value();
                let id = ticket.id.clone()?.to_string();
                let file_path = ticket.file_path.clone()?;
                Some((id, file_path))
            })
            .collect();

        // Now iterate the snapshot and insert into embeddings without holding
        // any tickets map guards.
        for (id, file_path) in ticket_info {
            let mtime_ns = match file_mtime_ns(&file_path) {
                Some(ns) => ns,
                None => continue,
            };

            let key = Self::embedding_key(&file_path, mtime_ns);
            let bin_path = emb_dir.join(format!("{key}.bin"));

            if let Ok(data) = fs::read(&bin_path) {
                // Validate file size matches expected embedding dimensions
                // before parsing. Catches corruption or model-change mismatches.
                if data.len() != expected_bytes {
                    continue;
                }

                if let Some(vector) = bytes_to_f32_vec(&data) {
                    // Validate: no NaN or infinity values. A corrupted .bin
                    // file with non-finite floats would silently poison all
                    // cosine_similarity results.
                    if vector.iter().any(|v| !v.is_finite()) {
                        eprintln!(
                            "warning: skipping embedding for {file_path:?}: contains NaN or infinity"
                        );
                        continue;
                    }

                    self.embeddings().insert(id, vector);
                }
            }
        }
    }

    /// Save a single embedding to disk at `.janus/embeddings/{key}.bin`.
    ///
    /// The embedding is stored as raw little-endian f32 bytes (4 bytes per value).
    pub fn save_embedding(key: &str, vector: &[f32]) -> std::io::Result<()> {
        let emb_dir = embeddings_dir();
        fs::create_dir_all(&emb_dir)?;

        let bin_path = emb_dir.join(format!("{key}.bin"));
        let bytes = f32_vec_to_bytes(vector);
        fs::write(bin_path, bytes)
    }

    /// Delete orphaned `.bin` files not in the `valid_keys` set.
    ///
    /// Returns the number of files deleted.
    ///
    /// # Concurrency Warning
    ///
    /// This function is **not safe against concurrent ticket modifications**. It suffers
    /// from a TOCTOU (time-of-check-time-of-use) race: if a ticket file is modified
    /// between when the caller computes `valid_keys` and when this function deletes
    /// files, a newly-generated embedding (with an updated mtime-based key) could be
    /// incorrectly deleted. Callers should ensure that no concurrent processes are
    /// modifying tickets or generating embeddings (e.g., via `janus cache rebuild`)
    /// while pruning is in progress.
    pub fn prune_orphaned(valid_keys: &HashSet<String>) -> std::io::Result<usize> {
        let emb_dir = embeddings_dir();
        if !emb_dir.exists() {
            return Ok(0);
        }

        let mut pruned = 0;
        for entry in fs::read_dir(&emb_dir)? {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    eprintln!(
                        "warning: failed to read directory entry in {}: {e}",
                        emb_dir.display()
                    );
                    continue;
                }
            };
            let path = entry.path();

            if path.extension().is_some_and(|ext| ext == "bin") {
                let file_stem = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();

                if !valid_keys.contains(&file_stem) {
                    if let Err(e) = fs::remove_file(&path) {
                        eprintln!(
                            "warning: failed to remove orphaned embedding {}: {e}",
                            path.display()
                        );
                        continue;
                    }
                    pruned += 1;
                }
            }
        }

        Ok(pruned)
    }

    /// Get embedding coverage stats: `(with_embeddings, total_tickets)`.
    ///
    /// Only counts embeddings whose ticket ID still exists in the tickets store,
    /// as a defensive measure against orphaned embeddings inflating the count.
    pub fn embedding_coverage(&self) -> (usize, usize) {
        let total = self.tickets().len();

        // Snapshot embedding keys into a local Vec so that all embeddings
        // DashMap shard locks are released before we touch the tickets DashMap.
        // This prevents AB/BA deadlocks between the two maps.
        let embedding_keys: Vec<String> = self
            .embeddings()
            .iter()
            .map(|entry| entry.key().clone())
            .collect();

        let with_embeddings = embedding_keys
            .iter()
            .filter(|id| self.tickets().contains_key(id.as_str()))
            .count();
        (with_embeddings, total)
    }

    /// Check if a ticket has an embedding in the in-memory store.
    pub fn has_embedding_for(&self, ticket_id: &str) -> bool {
        self.embeddings().contains_key(ticket_id)
    }

    /// Ensure a ticket has an embedding generated and saved.
    ///
    /// This method:
    /// 1. Looks up the ticket from the store by ID
    /// 2. Gets file_path, title, body from metadata
    /// 3. Computes embedding key from file_path + current mtime
    /// 4. Checks if .bin file already exists for that key (optimization)
    /// 5. If not, generates embedding via generate_ticket_embedding()
    /// 6. Saves to disk via save_embedding()
    /// 7. Inserts into in-memory embeddings DashMap (CRITICAL for TUI freshness)
    ///
    /// Returns silently on error (errors logged to stderr in debug builds only)
    pub async fn ensure_embedding(&self, ticket_id: &str) -> Result<(), String> {
        // Snapshot required fields from the tickets DashMap into owned locals,
        // then drop the guard before touching the embeddings DashMap.
        // This prevents AB/BA lock-order inversion deadlocks.
        let (file_path, title, body) = {
            let ticket = self
                .tickets()
                .get(ticket_id)
                .ok_or_else(|| format!("Ticket '{ticket_id}' not found in store"))?;

            let file_path = ticket
                .file_path
                .clone()
                .ok_or_else(|| format!("Ticket '{ticket_id}' has no file_path"))?;
            let title = ticket.title.clone().unwrap_or_default();
            let body = ticket.body.clone();
            (file_path, title, body)
        }; // ticket guard (Ref) dropped here

        // Get current mtime (fresh, not cached)
        let mtime_ns = file_mtime_ns(&file_path)
            .ok_or_else(|| format!("Could not get mtime for file: {file_path:?}"))?;

        // Compute embedding key
        let key = Self::embedding_key(&file_path, mtime_ns);

        // Check if .bin file already exists
        let emb_dir = embeddings_dir();
        let bin_path = emb_dir.join(format!("{key}.bin"));

        if bin_path.exists() {
            // Embedding already exists on disk - load it into memory
            if let Ok(data) = fs::read(&bin_path) {
                let expected_bytes = EMBEDDING_DIMENSIONS * 4;
                if data.len() == expected_bytes {
                    if let Some(vector) = bytes_to_f32_vec(&data) {
                        // Validate no NaN/infinity
                        if !vector.iter().any(|v| !v.is_finite()) {
                            self.embeddings().insert(ticket_id.to_string(), vector);
                            return Ok(());
                        }
                    }
                }
            }
            // If loading failed, fall through to regeneration
        }

        // Generate new embedding
        let embedding = crate::embedding::model::generate_ticket_embedding(&title, body.as_deref())
            .await
            .map_err(|e| format!("Failed to generate embedding: {e}"))?;

        // Save to disk
        Self::save_embedding(&key, &embedding)
            .map_err(|e| format!("Failed to save embedding: {e}"))?;

        // Insert into in-memory store (CRITICAL for TUI freshness)
        self.embeddings().insert(ticket_id.to_string(), embedding);

        Ok(())
    }

    /// Ensure all tickets have embeddings generated.
    ///
    /// Processes tickets in batches of EMBEDDING_BATCH_SIZE (32).
    /// Returns (generated_count, total_count) for progress reporting.
    pub async fn ensure_all_embeddings(&self) -> Result<(usize, usize), String> {
        use crate::embedding::model::{EMBEDDING_BATCH_SIZE, get_embedding_model};

        // Two-phase collection to avoid nested DashMap guards:
        // Phase 1: Snapshot all candidate ticket data from the tickets DashMap.
        let all_candidates: Vec<(String, std::path::PathBuf, String, Option<String>)> = self
            .tickets()
            .iter()
            .filter_map(|entry| {
                let ticket = entry.value();
                let id = entry.key().clone();
                let file_path = ticket.file_path.clone()?;
                let title = ticket.title.clone().unwrap_or_default();
                let body = ticket.body.clone();
                Some((id, file_path, title, body))
            })
            .collect();
        // Phase 2: Filter against the embeddings DashMap now that all tickets
        // guards are released, preventing AB/BA lock-order inversion deadlocks.
        let tickets_to_embed: Vec<(String, std::path::PathBuf, String, Option<String>)> =
            all_candidates
                .into_iter()
                .filter(|(id, _, _, _)| !self.has_embedding_for(id))
                .collect();

        let total = tickets_to_embed.len();
        if total == 0 {
            return Ok((0, 0));
        }

        let mut generated = 0usize;
        let model = get_embedding_model().await?;

        // Process in batches
        for batch in tickets_to_embed.chunks(EMBEDDING_BATCH_SIZE) {
            // Prepare batch texts
            let texts: Vec<String> = batch
                .iter()
                .map(|(_, _, title, body)| match body {
                    Some(b) if !b.is_empty() => format!("{title}\n\n{b}"),
                    _ => title.clone(),
                })
                .collect();

            let texts_ref: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();

            // Generate batch embeddings
            match model.embed_batch(&texts_ref).await {
                Ok(embeddings) => {
                    // Save each embedding
                    for (i, (id, file_path, _, _)) in batch.iter().enumerate() {
                        if let Some(embedding) = embeddings.get(i) {
                            // Get current mtime (may have changed)
                            let mtime_ns = file_mtime_ns(file_path);

                            if let Some(mtime) = mtime_ns {
                                let key = TicketStore::embedding_key(file_path, mtime);

                                // Save and update memory
                                if TicketStore::save_embedding(&key, embedding).is_ok() {
                                    // Check ticket still exists before inserting
                                    if self.tickets().contains_key(id.as_str()) {
                                        self.embeddings().insert(id.clone(), embedding.clone());
                                        generated += 1;
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    // Log batch error but continue with next batch
                    #[cfg(debug_assertions)]
                    eprintln!("Warning: Batch embedding generation failed: {e}");
                }
            }
        }

        Ok((generated, total))
    }
}

/// Get file modification time as nanoseconds since UNIX epoch.
///
/// The actual precision depends on the underlying filesystem (see
/// [`TicketStore::embedding_key`] for details).
fn file_mtime_ns(path: &Path) -> Option<u128> {
    let metadata = fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    let duration = modified.duration_since(UNIX_EPOCH).ok()?;
    Some(duration.as_nanos())
}

/// Convert a slice of f32 values to little-endian bytes.
fn f32_vec_to_bytes(vector: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vector.len() * 4);
    for &val in vector {
        bytes.extend_from_slice(&val.to_le_bytes());
    }
    bytes
}

/// Convert little-endian bytes back to a Vec<f32>.
///
/// Returns `None` if the byte length is not a multiple of 4.
fn bytes_to_f32_vec(data: &[u8]) -> Option<Vec<f32>> {
    if data.len() % 4 != 0 {
        return None;
    }

    let vector: Vec<f32> = data
        .chunks_exact(4)
        .map(|chunk| {
            let bytes: [u8; 4] = chunk.try_into().expect("chunk is exactly 4 bytes");
            f32::from_le_bytes(bytes)
        })
        .collect();

    Some(vector)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use tempfile::TempDir;

    use super::*;
    use crate::cache::TicketStore;
    use crate::paths::JanusRootGuard;
    use crate::types::TicketId;
    use crate::types::{TicketMetadata, TicketStatus};

    #[test]
    fn test_embedding_key_deterministic() {
        let path = Path::new("/some/path/ticket.md");
        let mtime = 1234567890_u128;

        let key1 = TicketStore::embedding_key(path, mtime);
        let key2 = TicketStore::embedding_key(path, mtime);

        assert_eq!(key1, key2);
        assert!(!key1.is_empty());
        // blake3 hex output is 64 chars
        assert_eq!(key1.len(), 64);
    }

    #[test]
    fn test_embedding_key_changes_with_mtime() {
        let path = Path::new("/some/path/ticket.md");

        let key1 = TicketStore::embedding_key(path, 1000);
        let key2 = TicketStore::embedding_key(path, 2000);

        assert_ne!(key1, key2);
    }

    #[test]
    fn test_embedding_key_changes_with_path() {
        let path1 = Path::new("/some/path/ticket1.md");
        let path2 = Path::new("/some/path/ticket2.md");

        let key1 = TicketStore::embedding_key(path1, 1000);
        let key2 = TicketStore::embedding_key(path2, 1000);

        assert_ne!(key1, key2);
    }

    #[test]
    fn test_f32_bytes_roundtrip() {
        let original = vec![1.0_f32, -2.5, 0.0, 3.14159, f32::MAX, f32::MIN];
        let bytes = f32_vec_to_bytes(&original);
        let recovered = bytes_to_f32_vec(&bytes).expect("should parse successfully");
        assert_eq!(original, recovered);
    }

    #[test]
    fn test_f32_bytes_invalid_length() {
        // 5 bytes is not a multiple of 4
        let bad_data = vec![0u8; 5];
        assert!(bytes_to_f32_vec(&bad_data).is_none());
    }

    #[test]
    fn test_f32_bytes_empty() {
        let empty: Vec<f32> = vec![];
        let bytes = f32_vec_to_bytes(&empty);
        assert!(bytes.is_empty());
        let recovered = bytes_to_f32_vec(&bytes).unwrap();
        assert!(recovered.is_empty());
    }

    #[test]
    fn test_save_and_load_embedding() {
        let tmp = TempDir::new().unwrap();
        let janus = tmp.path().join(".janus");
        let items_dir = janus.join("items");
        std::fs::create_dir_all(&items_dir).unwrap();

        let _guard = JanusRootGuard::new(&janus);

        // Create a ticket file
        let ticket_path = items_dir.join("j-test.md");
        std::fs::write(
            &ticket_path,
            r#"---
id: j-test
uuid: 550e8400-e29b-41d4-a716-446655440000
status: new
---
# Test
"#,
        )
        .unwrap();

        let mtime_ns = file_mtime_ns(&ticket_path).expect("should get mtime");
        let key = TicketStore::embedding_key(&ticket_path, mtime_ns);

        // Save embedding (must match EMBEDDING_DIMENSIONS for load validation)
        let vector: Vec<f32> = (0..EMBEDDING_DIMENSIONS).map(|i| i as f32 * 0.1).collect();
        TicketStore::save_embedding(&key, &vector).expect("save should succeed");

        // Verify the file was created
        let bin_path = embeddings_dir().join(format!("{key}.bin"));
        assert!(bin_path.exists());

        // Load into store
        let store = TicketStore::empty();
        store.upsert_ticket(TicketMetadata {
            id: Some(TicketId::new_unchecked("j-test")),
            file_path: Some(ticket_path),
            status: Some(TicketStatus::New),
            ..Default::default()
        });

        store.load_embeddings();
        assert_eq!(store.embeddings().len(), 1);

        let loaded = store.embeddings().get("j-test").unwrap();
        assert_eq!(*loaded.value(), vector);
    }

    #[test]
    fn test_prune_orphaned() {
        let tmp = TempDir::new().unwrap();
        let janus = tmp.path().join(".janus");
        let emb_dir = janus.join("embeddings");
        std::fs::create_dir_all(&emb_dir).unwrap();

        let _guard = JanusRootGuard::new(&janus);

        // Create some .bin files
        std::fs::write(emb_dir.join("valid1.bin"), b"data").unwrap();
        std::fs::write(emb_dir.join("valid2.bin"), b"data").unwrap();
        std::fs::write(emb_dir.join("orphan1.bin"), b"data").unwrap();
        std::fs::write(emb_dir.join("orphan2.bin"), b"data").unwrap();

        let mut valid_keys = HashSet::new();
        valid_keys.insert("valid1".to_string());
        valid_keys.insert("valid2".to_string());

        let pruned = TicketStore::prune_orphaned(&valid_keys).expect("prune should succeed");
        assert_eq!(pruned, 2);

        // Verify only valid files remain
        assert!(emb_dir.join("valid1.bin").exists());
        assert!(emb_dir.join("valid2.bin").exists());
        assert!(!emb_dir.join("orphan1.bin").exists());
        assert!(!emb_dir.join("orphan2.bin").exists());
    }

    #[test]
    fn test_prune_orphaned_no_dir() {
        let tmp = TempDir::new().unwrap();
        let janus = tmp.path().join(".janus");
        // Don't create the embeddings dir

        let _guard = JanusRootGuard::new(&janus);

        let pruned = TicketStore::prune_orphaned(&HashSet::new()).expect("prune should succeed");
        assert_eq!(pruned, 0);
    }

    #[test]
    fn test_embedding_coverage_empty() {
        let store = TicketStore::empty();
        let (with, total) = store.embedding_coverage();
        assert_eq!(with, 0);
        assert_eq!(total, 0);
    }

    #[test]
    fn test_embedding_coverage_partial() {
        let store = TicketStore::empty();

        store.upsert_ticket(TicketMetadata {
            id: Some(TicketId::new_unchecked("j-1")),
            ..Default::default()
        });
        store.upsert_ticket(TicketMetadata {
            id: Some(TicketId::new_unchecked("j-2")),
            ..Default::default()
        });
        store.upsert_ticket(TicketMetadata {
            id: Some(TicketId::new_unchecked("j-3")),
            ..Default::default()
        });

        // Add embeddings for only 2 of 3 tickets
        store.embeddings().insert("j-1".to_string(), vec![1.0]);
        store.embeddings().insert("j-2".to_string(), vec![2.0]);

        let (with, total) = store.embedding_coverage();
        assert_eq!(with, 2);
        assert_eq!(total, 3);
    }

    #[test]
    fn test_load_embeddings_no_dir() {
        let tmp = TempDir::new().unwrap();
        let janus = tmp.path().join(".janus");
        // Don't create the embeddings dir

        let _guard = JanusRootGuard::new(&janus);

        let store = TicketStore::empty();
        store.upsert_ticket(TicketMetadata {
            id: Some(TicketId::new_unchecked("j-1")),
            file_path: Some(tmp.path().join("ticket.md")),
            ..Default::default()
        });

        // Should not panic, just return without loading
        store.load_embeddings();
        assert_eq!(store.embeddings().len(), 0);
    }

    #[test]
    fn test_load_embeddings_ticket_without_filepath() {
        let store = TicketStore::empty();
        store.upsert_ticket(TicketMetadata {
            id: Some(TicketId::new_unchecked("j-nofp")),
            file_path: None,
            ..Default::default()
        });

        // Should skip tickets without file_path
        store.load_embeddings();
        assert_eq!(store.embeddings().len(), 0);
    }

    #[test]
    fn test_load_embeddings_rejects_wrong_dimension() {
        let tmp = TempDir::new().unwrap();
        let janus = tmp.path().join(".janus");
        let items_dir = janus.join("items");
        std::fs::create_dir_all(&items_dir).unwrap();

        let _guard = JanusRootGuard::new(&janus);

        // Create a ticket file
        let ticket_path = items_dir.join("j-dim.md");
        std::fs::write(
            &ticket_path,
            r#"---
id: j-dim
uuid: 550e8400-e29b-41d4-a716-446655440001
status: new
---
# Dimension test
"#,
        )
        .unwrap();

        let mtime_ns = file_mtime_ns(&ticket_path).expect("should get mtime");
        let key = TicketStore::embedding_key(&ticket_path, mtime_ns);

        // Save an embedding with wrong dimensions (4 floats instead of EMBEDDING_DIMENSIONS)
        let wrong_vector = vec![1.0_f32, 2.0, 3.0, 4.0];
        TicketStore::save_embedding(&key, &wrong_vector).expect("save should succeed");

        let store = TicketStore::empty();
        store.upsert_ticket(TicketMetadata {
            id: Some(TicketId::new_unchecked("j-dim")),
            file_path: Some(ticket_path),
            status: Some(TicketStatus::New),
            ..Default::default()
        });

        // load_embeddings should reject the wrong-dimension file
        store.load_embeddings();
        assert_eq!(store.embeddings().len(), 0);
    }

    #[test]
    fn test_load_embeddings_rejects_non_finite_values() {
        let tmp = TempDir::new().unwrap();
        let janus = tmp.path().join(".janus");
        let items_dir = janus.join("items");
        std::fs::create_dir_all(&items_dir).unwrap();

        let _guard = JanusRootGuard::new(&janus);

        // Create a ticket file
        let ticket_path = items_dir.join("j-nan.md");
        std::fs::write(
            &ticket_path,
            r#"---
id: j-nan
uuid: 550e8400-e29b-41d4-a716-446655440002
status: new
---
# NaN test
"#,
        )
        .unwrap();

        let mtime_ns = file_mtime_ns(&ticket_path).expect("should get mtime");
        let key = TicketStore::embedding_key(&ticket_path, mtime_ns);

        // Build a vector with the correct number of dimensions but containing NaN
        let mut nan_vector: Vec<f32> = (0..EMBEDDING_DIMENSIONS).map(|i| i as f32 * 0.1).collect();
        nan_vector[0] = f32::NAN;

        TicketStore::save_embedding(&key, &nan_vector).expect("save should succeed");

        let store = TicketStore::empty();
        store.upsert_ticket(TicketMetadata {
            id: Some(TicketId::new_unchecked("j-nan")),
            file_path: Some(ticket_path),
            status: Some(TicketStatus::New),
            ..Default::default()
        });

        // load_embeddings should reject the embedding containing NaN
        store.load_embeddings();
        assert_eq!(store.embeddings().len(), 0);

        // Also test with infinity
        let ticket_path2 = items_dir.join("j-inf.md");
        std::fs::write(
            &ticket_path2,
            r#"---
id: j-inf
uuid: 550e8400-e29b-41d4-a716-446655440003
status: new
---
# Infinity test
"#,
        )
        .unwrap();

        let mtime_ns2 = file_mtime_ns(&ticket_path2).expect("should get mtime");
        let key2 = TicketStore::embedding_key(&ticket_path2, mtime_ns2);

        let mut inf_vector: Vec<f32> = (0..EMBEDDING_DIMENSIONS).map(|i| i as f32 * 0.1).collect();
        inf_vector[5] = f32::INFINITY;

        TicketStore::save_embedding(&key2, &inf_vector).expect("save should succeed");

        let store2 = TicketStore::empty();
        store2.upsert_ticket(TicketMetadata {
            id: Some(TicketId::new_unchecked("j-inf")),
            file_path: Some(ticket_path2),
            status: Some(TicketStatus::New),
            ..Default::default()
        });

        // load_embeddings should reject the embedding containing infinity
        store2.load_embeddings();
        assert_eq!(store2.embeddings().len(), 0);
    }
}

use std::path::PathBuf;
use std::time::SystemTime;

use tokio::fs;

use serde_json;
use turso::params;
use turso::transaction::Transaction;

use crate::error::{JanusError, Result};
use crate::formatting::extract_ticket_body;
use crate::plan::parser::parse_plan_content;
use crate::plan::types::PlanMetadata;
use crate::ticket::parse_ticket;
use crate::types::{TicketMetadata, plans_dir, tickets_items_dir};
use crate::utils::generate_uuid;

use crate::embedding::model::generate_ticket_embedding;
use crate::remote::config::Config;

/// Trait for items that can be cached in the SQLite database.
///
/// This trait abstracts over the common caching operations for both
/// tickets and plans, enabling a single generic sync implementation.
pub trait CacheableItem: Sized {
    /// The directory where items of this type are stored (e.g., ".janus/items")
    /// Returns a PathBuf to support dynamic paths via JANUS_ROOT environment variable.
    fn directory() -> PathBuf;

    /// The name of the ID column in the database (e.g., "ticket_id")
    fn id_column() -> &'static str;

    /// The name of the database table (e.g., "tickets")
    fn table_name() -> &'static str;

    /// The human-readable name of this item type (e.g., "ticket", "plan")
    fn item_name() -> &'static str;

    /// The plural form of the item name (e.g., "tickets", "plans")
    fn item_name_plural() -> &'static str;

    /// Parse an item from its file on disk.
    /// Returns the parsed item and the file's mtime in nanoseconds.
    #[allow(clippy::manual_async_fn)]
    fn parse_from_file(id: &str) -> impl std::future::Future<Output = Result<(Self, i64)>> + Send;

    /// Insert or replace this item in the database within a transaction.
    fn insert_into_cache<'a>(
        &self,
        tx: &'a Transaction<'a>,
        mtime_ns: i64,
    ) -> impl std::future::Future<Output = Result<()>> + Send + 'a;
}

// =============================================================================
// TicketMetadata implementation
// =============================================================================

impl CacheableItem for TicketMetadata {
    fn directory() -> PathBuf {
        tickets_items_dir()
    }

    fn id_column() -> &'static str {
        "ticket_id"
    }

    fn table_name() -> &'static str {
        "tickets"
    }

    fn item_name() -> &'static str {
        "ticket"
    }

    fn item_name_plural() -> &'static str {
        "tickets"
    }

    #[allow(clippy::manual_async_fn)]
    fn parse_from_file(id: &str) -> impl std::future::Future<Output = Result<(Self, i64)>> + Send {
        async move {
            let path = Self::directory().join(format!("{}.md", id));

            let content = fs::read_to_string(&path).await.map_err(JanusError::Io)?;

            let mut metadata = parse_ticket(&content).map_err(|e| {
                JanusError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
            })?;

            // Extract body for caching
            metadata.body = extract_ticket_body(&content);

            // Set the file_path so it gets cached
            metadata.file_path = Some(path.clone());

            let file_mtime = fs::metadata(&path)
                .await
                .map_err(JanusError::Io)?
                .modified()
                .map_err(JanusError::Io)?;

            let mtime_ns = file_mtime
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_err(|e| JanusError::Io(std::io::Error::other(e)))?
                .as_nanos() as i64;

            Ok((metadata, mtime_ns))
        }
    }

    fn insert_into_cache<'a>(
        &self,
        tx: &'a Transaction<'a>,
        mtime_ns: i64,
    ) -> impl std::future::Future<Output = Result<()>> + Send + 'a {
        let ticket_id = self
            .id
            .clone()
            .ok_or_else(|| {
                JanusError::CacheDataIntegrity("Ticket missing ID field - cannot cache".to_string())
            })
            .and_then(|id| {
                if id.is_empty() {
                    Err(JanusError::CacheDataIntegrity(
                        "Ticket has empty ID field - cannot cache".to_string(),
                    ))
                } else {
                    Ok(id)
                }
            });
        let uuid = self.uuid.clone().unwrap_or_else(generate_uuid);
        let status = self.status.map(|s| s.to_string());
        let priority = self.priority.map(|p| p.as_num() as i64);
        let ticket_type = self.ticket_type.map(|t| t.to_string());
        let deps_json = Some(serialize_array(&self.deps));
        let links_json = Some(serialize_array(&self.links));
        let title = self.title.clone();
        let parent = self.parent.clone();
        let created = self.created.clone();
        let external_ref = self.external_ref.clone();
        let remote = self.remote.clone();
        let completion_summary = self.completion_summary.clone();
        let spawned_from = self.spawned_from.clone();
        let spawn_context = self.spawn_context.clone();
        let depth = self.depth.map(|d| d as i64);
        let triaged = self.triaged.map(|t| if t { 1 } else { 0 });
        let file_path = self
            .file_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string());
        let body = self.body.clone();
        let size = self.size.map(|s| s.to_string());

        async move {
            let ticket_id = ticket_id?;

            // Generate embedding for the ticket only if semantic search is enabled
            let embedding_blob = match Config::load() {
                Ok(config) => {
                    if config.semantic_search_enabled() {
                        let title_ref = title.as_deref().unwrap_or("");
                        let body_ref = body.as_deref();
                        match generate_ticket_embedding(title_ref, body_ref).await {
                            Ok(embedding) => Some(embedding_to_blob(&embedding)),
                            Err(e) => {
                                eprintln!(
                                    "Warning: failed to generate embedding for ticket: {}",
                                    e
                                );
                                None
                            }
                        }
                    } else {
                        // Semantic search disabled, skip embedding generation
                        None
                    }
                }
                Err(e) => {
                    eprintln!(
                        "Warning: failed to load config: {}. Skipping embedding generation.",
                        e
                    );
                    None
                }
            };

            {
                // Include embedding column
                tx.execute(
                    "INSERT OR REPLACE INTO tickets (
                        ticket_id, uuid, mtime_ns, status, title, priority, ticket_type,
                        deps, links, parent, created, external_ref, remote, completion_summary,
                        spawned_from, spawn_context, depth, file_path, triaged, body, size, embedding
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)",
                    params![
                        ticket_id,
                        uuid,
                        mtime_ns,
                        status,
                        title,
                        priority,
                        ticket_type,
                        deps_json,
                        links_json,
                        parent,
                        created,
                        external_ref,
                        remote,
                        completion_summary,
                        spawned_from,
                        spawn_context,
                        depth,
                        file_path,
                        triaged,
                        body,
                        size,
                        embedding_blob,
                    ],
                )
                .await?;
            }

            Ok(())
        }
    }
}

// =============================================================================
// PlanMetadata implementation
// =============================================================================

impl CacheableItem for PlanMetadata {
    fn directory() -> PathBuf {
        plans_dir()
    }

    fn id_column() -> &'static str {
        "plan_id"
    }

    fn table_name() -> &'static str {
        "plans"
    }

    fn item_name() -> &'static str {
        "plan"
    }

    fn item_name_plural() -> &'static str {
        "plans"
    }

    #[allow(clippy::manual_async_fn)]
    fn parse_from_file(id: &str) -> impl std::future::Future<Output = Result<(Self, i64)>> + Send {
        async move {
            let path = Self::directory().join(format!("{}.md", id));

            let content = fs::read_to_string(&path).await.map_err(JanusError::Io)?;

            let metadata = parse_plan_content(&content).map_err(|e| {
                JanusError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
            })?;

            let file_mtime = fs::metadata(&path)
                .await
                .map_err(JanusError::Io)?
                .modified()
                .map_err(JanusError::Io)?;

            let mtime_ns = file_mtime
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_err(|e| JanusError::Io(std::io::Error::other(e)))?
                .as_nanos() as i64;

            Ok((metadata, mtime_ns))
        }
    }

    fn insert_into_cache<'a>(
        &self,
        tx: &'a Transaction<'a>,
        mtime_ns: i64,
    ) -> impl std::future::Future<Output = Result<()>> + Send + 'a {
        let plan_id = self
            .id
            .clone()
            .ok_or_else(|| {
                JanusError::CacheDataIntegrity("Plan missing ID field - cannot cache".to_string())
            })
            .and_then(|id| {
                if id.is_empty() {
                    Err(JanusError::CacheDataIntegrity(
                        "Plan has empty ID field - cannot cache".to_string(),
                    ))
                } else {
                    Ok(id)
                }
            });
        let uuid = self.uuid.clone().unwrap_or_else(generate_uuid);
        let title = self.title.clone();
        let created = self.created.clone();

        // Determine structure type and serialize tickets/phases
        let (structure_type, tickets_json, phases_json) = if self.is_phased() {
            let phases = self.phases();
            let phases_data: Vec<_> = phases
                .iter()
                .map(|p| {
                    serde_json::json!({
                        "number": p.number,
                        "name": p.name,
                        "tickets": p.tickets
                    })
                })
                .collect();
            let phases_json = serde_json::to_string(&phases_data).ok();
            ("phased".to_string(), None, phases_json)
        } else if self.is_simple() {
            let tickets = self.all_tickets();
            let tickets_json = serde_json::to_string(&tickets).ok();
            ("simple".to_string(), tickets_json, None)
        } else {
            // Empty plan
            ("empty".to_string(), None, None)
        };

        async move {
            let plan_id = plan_id?;
            tx.execute(
                "INSERT OR REPLACE INTO plans (
                    plan_id, uuid, mtime_ns, title, created, structure_type, tickets_json, phases_json
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    plan_id,
                    uuid,
                    mtime_ns,
                    title,
                    created,
                    structure_type,
                    tickets_json,
                    phases_json,
                ],
            )
            .await?;

            Ok(())
        }
    }
}

// =============================================================================
// Helper functions
// =============================================================================

/// Serialize an array to JSON, returning None for empty arrays.
fn serialize_array(arr: &[String]) -> String {
    if arr.is_empty() {
        "[]".to_string()
    } else {
        serde_json::to_string(arr).unwrap_or_else(|_| "[]".to_string())
    }
}

/// Convert embedding vector to byte blob for storage.
/// Each f32 is serialized as 4 little-endian bytes.
fn embedding_to_blob(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// Convert byte blob back to embedding vector.
/// Each 4-byte chunk is deserialized as a little-endian f32.
#[allow(dead_code)]
fn blob_to_embedding(blob: &[u8]) -> Vec<f32> {
    blob.chunks_exact(4)
        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

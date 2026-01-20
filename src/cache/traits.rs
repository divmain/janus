use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use serde_json;
use turso::params;
use turso::transaction::Transaction;

use crate::error::{JanusError, Result};
use crate::plan::parser::parse_plan_content;
use crate::plan::types::PlanMetadata;
use crate::ticket::parse_ticket;
use crate::types::{PLANS_DIR, TICKETS_ITEMS_DIR, TicketMetadata};

/// Trait for items that can be cached in the SQLite database.
///
/// This trait abstracts over the common caching operations for both
/// tickets and plans, enabling a single generic sync implementation.
pub trait CacheableItem: Sized {
    /// The directory where items of this type are stored (e.g., ".janus/items")
    fn directory() -> &'static str;

    /// The name of the ID column in the database (e.g., "ticket_id")
    fn id_column() -> &'static str;

    /// The name of the database table (e.g., "tickets")
    fn table_name() -> &'static str;

    /// The human-readable name of this item type (e.g., "ticket", "plan")
    fn item_name() -> &'static str;

    /// Parse an item from its file on disk.
    /// Returns the parsed item and the file's mtime in nanoseconds.
    fn parse_from_file(id: &str) -> Result<(Self, i64)>;

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
    fn directory() -> &'static str {
        TICKETS_ITEMS_DIR
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

    fn parse_from_file(id: &str) -> Result<(Self, i64)> {
        let path = PathBuf::from(Self::directory()).join(format!("{}.md", id));

        let content = fs::read_to_string(&path).map_err(JanusError::Io)?;

        let metadata = parse_ticket(&content)
            .map_err(|e| JanusError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;

        let file_mtime = fs::metadata(&path)
            .map_err(JanusError::Io)?
            .modified()
            .map_err(JanusError::Io)?;

        let mtime_ns = file_mtime
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|e| JanusError::Io(std::io::Error::other(e)))?
            .as_nanos() as i64;

        Ok((metadata, mtime_ns))
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
        let uuid = self.uuid.clone();
        let status = self.status.map(|s| s.to_string());
        let priority = self.priority.map(|p| p.as_num() as i64);
        let ticket_type = self.ticket_type.map(|t| t.to_string());
        let deps_json = serialize_array(&self.deps);
        let links_json = serialize_array(&self.links);
        let title = self.title.clone();
        let parent = self.parent.clone();
        let created = self.created.clone();
        let external_ref = self.external_ref.clone();
        let remote = self.remote.clone();
        let completion_summary = self.completion_summary.clone();
        let spawned_from = self.spawned_from.clone();
        let spawn_context = self.spawn_context.clone();
        let depth = self.depth.map(|d| d as i64);

        async move {
            let ticket_id = ticket_id?;
            tx.execute(
                "INSERT OR REPLACE INTO tickets (
                    ticket_id, uuid, mtime_ns, status, title, priority, ticket_type,
                    deps, links, parent, created, external_ref, remote, completion_summary,
                    spawned_from, spawn_context, depth
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
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
                ],
            )
            .await?;

            Ok(())
        }
    }
}

// =============================================================================
// PlanMetadata implementation
// =============================================================================

impl CacheableItem for PlanMetadata {
    fn directory() -> &'static str {
        PLANS_DIR
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

    fn parse_from_file(id: &str) -> Result<(Self, i64)> {
        let path = PathBuf::from(Self::directory()).join(format!("{}.md", id));

        let content = fs::read_to_string(&path).map_err(JanusError::Io)?;

        let metadata = parse_plan_content(&content)
            .map_err(|e| JanusError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;

        let file_mtime = fs::metadata(&path)
            .map_err(JanusError::Io)?
            .modified()
            .map_err(JanusError::Io)?;

        let mtime_ns = file_mtime
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|e| JanusError::Io(std::io::Error::other(e)))?
            .as_nanos() as i64;

        Ok((metadata, mtime_ns))
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
        let uuid = self.uuid.clone();
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
fn serialize_array(arr: &[String]) -> Option<String> {
    if arr.is_empty() {
        None
    } else {
        serde_json::to_string(arr).ok()
    }
}

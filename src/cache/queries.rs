//! Query operations for reading data from the cache.
//!
//! This module handles:
//! - Retrieving tickets (get_all_tickets, get_ticket, find_by_partial_id)
//! - Retrieving plans (get_all_plans, get_plan, find_plan_by_partial_id)
//! - Building lookup maps (build_ticket_map)
//! - Row-to-object conversion helpers

use std::collections::HashMap;

use serde_json;

use crate::error::{JanusError as CacheError, Result};
use crate::types::TicketMetadata;

use super::database::TicketCache;
use super::types::{CachedPhase, CachedPlanMetadata};

impl TicketCache {
    // =========================================================================
    // Ticket queries
    // =========================================================================

    /// Get all cached tickets.
    pub async fn get_all_tickets(&self) -> Result<Vec<TicketMetadata>> {
        let conn = self.create_connection().await?;
        let mut rows = conn
            .query(
                "SELECT ticket_id, uuid, status, title, priority, ticket_type,
                deps, links, parent, created, external_ref, remote, completion_summary,
                spawned_from, spawn_context, depth
         FROM tickets",
                (),
            )
            .await?;

        let mut tickets = Vec::new();
        while let Some(row) = rows.next().await? {
            let metadata = Self::row_to_ticket_metadata(&row).await?;
            tickets.push(metadata);
        }
        Ok(tickets)
    }

    /// Get a single ticket by ID.
    pub async fn get_ticket(&self, id: &str) -> Result<Option<TicketMetadata>> {
        let conn = self.create_connection().await?;
        let mut rows = conn
            .query(
                "SELECT ticket_id, uuid, status, title, priority, ticket_type,
                deps, links, parent, created, external_ref, remote, completion_summary,
                spawned_from, spawn_context, depth
         FROM tickets WHERE ticket_id = ?1",
                [id],
            )
            .await?;

        if let Some(row) = rows.next().await? {
            let metadata = Self::row_to_ticket_metadata(&row).await?;
            Ok(Some(metadata))
        } else {
            Ok(None)
        }
    }

    /// Find tickets by partial ID (prefix match).
    pub async fn find_by_partial_id(&self, partial: &str) -> Result<Vec<String>> {
        let conn = self.create_connection().await?;
        let mut rows = conn
            .query(
                "SELECT ticket_id FROM tickets WHERE ticket_id LIKE ?1",
                [format!("{}%", partial)],
            )
            .await?;

        let mut matches = Vec::new();
        while let Some(row) = rows.next().await? {
            let id: String = row.get(0)?;
            matches.push(id);
        }
        Ok(matches)
    }

    /// Build a map of ticket ID to metadata for fast lookups.
    pub async fn build_ticket_map(&self) -> Result<HashMap<String, TicketMetadata>> {
        let tickets = self.get_all_tickets().await?;

        let mut map = HashMap::new();
        for ticket in tickets {
            if let Some(id) = &ticket.id {
                map.insert(id.clone(), ticket);
            }
        }
        Ok(map)
    }

    // =========================================================================
    // Plan queries
    // =========================================================================

    /// Get all cached plans.
    pub async fn get_all_plans(&self) -> Result<Vec<CachedPlanMetadata>> {
        let conn = self.create_connection().await?;
        let mut rows = conn
            .query(
                "SELECT plan_id, uuid, title, created, structure_type, tickets_json, phases_json
             FROM plans",
                (),
            )
            .await?;

        let mut plans = Vec::new();
        while let Some(row) = rows.next().await? {
            let metadata = Self::row_to_plan_metadata(&row).await?;
            plans.push(metadata);
        }
        Ok(plans)
    }

    /// Get a single plan by ID.
    pub async fn get_plan(&self, id: &str) -> Result<Option<CachedPlanMetadata>> {
        let conn = self.create_connection().await?;
        let mut rows = conn
            .query(
                "SELECT plan_id, uuid, title, created, structure_type, tickets_json, phases_json
             FROM plans WHERE plan_id = ?1",
                [id],
            )
            .await?;

        if let Some(row) = rows.next().await? {
            let metadata = Self::row_to_plan_metadata(&row).await?;
            Ok(Some(metadata))
        } else {
            Ok(None)
        }
    }

    /// Find plans by partial ID (prefix match).
    pub async fn find_plan_by_partial_id(&self, partial: &str) -> Result<Vec<String>> {
        let conn = self.create_connection().await?;
        let mut rows = conn
            .query(
                "SELECT plan_id FROM plans WHERE plan_id LIKE ?1",
                [format!("{}%", partial)],
            )
            .await?;

        let mut matches = Vec::new();
        while let Some(row) = rows.next().await? {
            let id: String = row.get(0)?;
            matches.push(id);
        }
        Ok(matches)
    }

    /// Get the count of tickets spawned from a given ticket.
    ///
    /// This queries the cache for tickets where `spawned_from` matches the given ID.
    pub async fn get_children_count(&self, id: &str) -> Result<usize> {
        let conn = self.create_connection().await?;
        let mut rows = conn
            .query("SELECT COUNT(*) FROM tickets WHERE spawned_from = ?1", [id])
            .await?;

        if let Some(row) = rows.next().await? {
            let count: i64 = row.get(0)?;
            Ok(count as usize)
        } else {
            Ok(0)
        }
    }

    // =========================================================================
    // Row conversion helpers
    // =========================================================================

    async fn row_to_ticket_metadata(row: &turso::Row) -> Result<TicketMetadata> {
        let id: Option<String> =
            Some(
                row.get::<String>(0)
                    .map_err(|e| CacheError::CacheColumnExtraction {
                        column: 0,
                        error: e.to_string(),
                    })?,
            );

        let uuid: Option<String> =
            row.get::<Option<String>>(1)
                .map_err(|e| CacheError::CacheColumnExtraction {
                    column: 1,
                    error: e.to_string(),
                })?;

        let status: Option<crate::types::TicketStatus> = row
            .get::<Option<String>>(2)
            .map_err(|e| CacheError::CacheColumnExtraction {
                column: 2,
                error: e.to_string(),
            })?
            .and_then(|s| s.parse().ok());

        let title: Option<String> =
            row.get::<Option<String>>(3)
                .map_err(|e| CacheError::CacheColumnExtraction {
                    column: 3,
                    error: e.to_string(),
                })?;

        let priority_num: Option<i64> =
            row.get::<Option<i64>>(4)
                .map_err(|e| CacheError::CacheColumnExtraction {
                    column: 4,
                    error: e.to_string(),
                })?;

        let type_str: Option<String> =
            row.get::<Option<String>>(5)
                .map_err(|e| CacheError::CacheColumnExtraction {
                    column: 5,
                    error: e.to_string(),
                })?;

        let deps_json: Option<String> =
            row.get::<Option<String>>(6)
                .map_err(|e| CacheError::CacheColumnExtraction {
                    column: 6,
                    error: e.to_string(),
                })?;

        let links_json: Option<String> =
            row.get::<Option<String>>(7)
                .map_err(|e| CacheError::CacheColumnExtraction {
                    column: 7,
                    error: e.to_string(),
                })?;

        let parent: Option<String> =
            row.get::<Option<String>>(8)
                .map_err(|e| CacheError::CacheColumnExtraction {
                    column: 8,
                    error: e.to_string(),
                })?;

        let created: Option<String> =
            row.get::<Option<String>>(9)
                .map_err(|e| CacheError::CacheColumnExtraction {
                    column: 9,
                    error: e.to_string(),
                })?;

        let external_ref: Option<String> =
            row.get::<Option<String>>(10)
                .map_err(|e| CacheError::CacheColumnExtraction {
                    column: 10,
                    error: e.to_string(),
                })?;

        let remote: Option<String> =
            row.get::<Option<String>>(11)
                .map_err(|e| CacheError::CacheColumnExtraction {
                    column: 11,
                    error: e.to_string(),
                })?;

        let completion_summary: Option<String> =
            row.get::<Option<String>>(12)
                .map_err(|e| CacheError::CacheColumnExtraction {
                    column: 12,
                    error: e.to_string(),
                })?;

        let spawned_from: Option<String> =
            row.get::<Option<String>>(13)
                .map_err(|e| CacheError::CacheColumnExtraction {
                    column: 13,
                    error: e.to_string(),
                })?;

        let spawn_context: Option<String> =
            row.get::<Option<String>>(14)
                .map_err(|e| CacheError::CacheColumnExtraction {
                    column: 14,
                    error: e.to_string(),
                })?;

        let depth_num: Option<i64> =
            row.get::<Option<i64>>(15)
                .map_err(|e| CacheError::CacheColumnExtraction {
                    column: 15,
                    error: e.to_string(),
                })?;

        // Parse ticket_type (optional domain field)
        let ticket_type: Option<crate::types::TicketType> = type_str.and_then(|s| {
            match s.parse() {
                Ok(tt) => Some(tt),
                Err(_) => {
                    eprintln!(
                        "Warning: Failed to parse ticket_type '{}' for ticket '{:?}'. Type will be None.",
                        s, id
                    );
                    None
                }
            }
        });

        // Parse priority (optional domain field)
        let priority: Option<crate::types::TicketPriority> = priority_num.and_then(|n| match n {
            0 => Some(crate::types::TicketPriority::P0),
            1 => Some(crate::types::TicketPriority::P1),
            2 => Some(crate::types::TicketPriority::P2),
            3 => Some(crate::types::TicketPriority::P3),
            4 => Some(crate::types::TicketPriority::P4),
            _ => {
                eprintln!(
                    "Warning: Invalid priority value {} for ticket '{:?}'. Priority will be None.",
                    n, id
                );
                None
            }
        });

        // Convert depth from i64 to u32
        let depth: Option<u32> = depth_num.and_then(|n| u32::try_from(n).ok());

        let deps = Self::deserialize_array(deps_json.as_deref())?;
        let links = Self::deserialize_array(links_json.as_deref())?;

        let metadata = TicketMetadata {
            id,
            uuid,
            title,
            status,
            priority,
            ticket_type,
            deps,
            links,
            parent,
            created,
            external_ref,
            remote,
            file_path: None,
            completion_summary,
            spawned_from,
            spawn_context,
            depth,
        };

        if metadata.id.is_none() {
            return Err(CacheError::CacheDataIntegrity(
                "ticket metadata missing required field 'id'".to_string(),
            ));
        }

        if metadata.uuid.is_none() {
            return Err(CacheError::CacheDataIntegrity(
                "ticket metadata missing required field 'uuid'".to_string(),
            ));
        }

        Ok(metadata)
    }

    async fn row_to_plan_metadata(row: &turso::Row) -> Result<CachedPlanMetadata> {
        let id: Option<String> =
            Some(
                row.get::<String>(0)
                    .map_err(|e| CacheError::CacheColumnExtraction {
                        column: 0,
                        error: e.to_string(),
                    })?,
            );

        let uuid: Option<String> =
            row.get::<Option<String>>(1)
                .map_err(|e| CacheError::CacheColumnExtraction {
                    column: 1,
                    error: e.to_string(),
                })?;

        let title: Option<String> =
            row.get::<Option<String>>(2)
                .map_err(|e| CacheError::CacheColumnExtraction {
                    column: 2,
                    error: e.to_string(),
                })?;

        let created: Option<String> =
            row.get::<Option<String>>(3)
                .map_err(|e| CacheError::CacheColumnExtraction {
                    column: 3,
                    error: e.to_string(),
                })?;

        let structure_type: Option<String> =
            row.get::<Option<String>>(4)
                .map_err(|e| CacheError::CacheColumnExtraction {
                    column: 4,
                    error: e.to_string(),
                })?;

        let tickets_json: Option<String> =
            row.get::<Option<String>>(5)
                .map_err(|e| CacheError::CacheColumnExtraction {
                    column: 5,
                    error: e.to_string(),
                })?;

        let phases_json: Option<String> =
            row.get::<Option<String>>(6)
                .map_err(|e| CacheError::CacheColumnExtraction {
                    column: 6,
                    error: e.to_string(),
                })?;

        // Deserialize tickets for simple plans with explicit error handling
        let tickets: Vec<String> = if let Some(json_str) = tickets_json.as_deref() {
            match serde_json::from_str(json_str) {
                Ok(tickets) => tickets,
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to deserialize plan tickets JSON for plan '{:?}': {}. Using empty array.",
                        id, e
                    );
                    vec![]
                }
            }
        } else {
            vec![]
        };

        // Deserialize phases for phased plans with explicit error handling
        let phases: Vec<CachedPhase> = if let Some(json_str) = phases_json.as_deref() {
            match serde_json::from_str(json_str) {
                Ok(phases) => phases,
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to deserialize plan phases JSON for plan '{:?}': {}. Using empty array.",
                        id, e
                    );
                    vec![]
                }
            }
        } else {
            vec![]
        };

        // Validate structure_type is valid
        let structure_type = match structure_type {
            Some(s) if matches!(s.as_str(), "simple" | "phased" | "empty") => s,
            Some(s) => {
                eprintln!(
                    "Warning: Invalid structure_type '{}' for plan '{:?}'. Defaulting to 'empty'.",
                    s, id
                );
                "empty".to_string()
            }
            None => {
                eprintln!(
                    "Warning: Missing structure_type for plan '{:?}'. Defaulting to 'empty'.",
                    id
                );
                "empty".to_string()
            }
        };

        let metadata = CachedPlanMetadata {
            id,
            uuid,
            title,
            created,
            structure_type,
            tickets,
            phases,
        };

        if metadata.id.is_none() {
            return Err(CacheError::CacheDataIntegrity(
                "plan metadata missing required field 'id'".to_string(),
            ));
        }

        if metadata.uuid.is_none() {
            return Err(CacheError::CacheDataIntegrity(
                "plan metadata missing required field 'uuid'".to_string(),
            ));
        }

        Ok(metadata)
    }

    /// Deserialize a JSON array from a database column.
    pub(crate) fn deserialize_array(s: Option<&str>) -> Result<Vec<String>> {
        match s {
            Some(json_str) if !json_str.is_empty() => serde_json::from_str(json_str).map_err(|e| {
                CacheError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
            }),
            _ => Ok(vec![]),
        }
    }

    /// Serialize an array to JSON, returning None for empty arrays.
    /// Exposed for testing purposes.
    #[cfg(test)]
    pub(crate) fn serialize_array(arr: &[String]) -> Result<Option<String>> {
        if arr.is_empty() {
            Ok(None)
        } else {
            serde_json::to_string(arr).map(Some).map_err(|e| {
                CacheError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
            })
        }
    }
}

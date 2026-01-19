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
        let mut rows = self
            .conn
            .query(
                "SELECT ticket_id, uuid, status, title, priority, ticket_type,
                    deps, links, parent, created, external_ref, remote, completion_summary
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
        let mut rows = self
            .conn
            .query(
                "SELECT ticket_id, uuid, status, title, priority, ticket_type,
                    deps, links, parent, created, external_ref, remote, completion_summary
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
        let mut rows = self
            .conn
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
        let mut rows = self
            .conn
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
        let mut rows = self
            .conn
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
        let mut rows = self
            .conn
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

    // =========================================================================
    // Row conversion helpers
    // =========================================================================

    async fn row_to_ticket_metadata(row: &turso::Row) -> Result<TicketMetadata> {
        let id: Option<String> = row.get(0).ok();
        let uuid: Option<String> = row.get(1).ok();
        let status_str: Option<String> = row.get(2).ok();
        let title: Option<String> = row.get(3).ok();
        let priority_num: Option<i64> = row.get(4).ok();
        let type_str: Option<String> = row.get(5).ok();
        let deps_json: Option<String> = row.get(6).ok();
        let links_json: Option<String> = row.get(7).ok();
        let parent: Option<String> = row.get(8).ok();
        let created: Option<String> = row.get(9).ok();
        let external_ref: Option<String> = row.get(10).ok();
        let remote: Option<String> = row.get(11).ok();
        let completion_summary: Option<String> = row.get(12).ok();

        // Parse status with explicit error handling
        let status = if let Some(ref s) = status_str {
            match s.parse() {
                Ok(status) => Some(status),
                Err(_) => {
                    eprintln!(
                        "Warning: Failed to parse status '{}' for ticket '{:?}'. Status will be None.",
                        s, id
                    );
                    None
                }
            }
        } else {
            None
        };

        // Parse ticket_type with explicit error handling
        let ticket_type = if let Some(ref s) = type_str {
            match s.parse() {
                Ok(ticket_type) => Some(ticket_type),
                Err(_) => {
                    eprintln!(
                        "Warning: Failed to parse ticket_type '{}' for ticket '{:?}'. Type will be None.",
                        s, id
                    );
                    None
                }
            }
        } else {
            None
        };

        // Parse priority with explicit error handling
        let priority = match priority_num {
            Some(n) => match n {
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
            },
            None => None,
        };

        let deps = Self::deserialize_array(deps_json.as_deref())?;
        let links = Self::deserialize_array(links_json.as_deref())?;

        Ok(TicketMetadata {
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
        })
    }

    async fn row_to_plan_metadata(row: &turso::Row) -> Result<CachedPlanMetadata> {
        let id: Option<String> = row.get(0).ok();
        let uuid: Option<String> = row.get(1).ok();
        let title: Option<String> = row.get(2).ok();
        let created: Option<String> = row.get(3).ok();
        let structure_type: Option<String> = row.get(4).ok();
        let tickets_json: Option<String> = row.get(5).ok();
        let phases_json: Option<String> = row.get(6).ok();

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

        Ok(CachedPlanMetadata {
            id,
            uuid,
            title,
            created,
            structure_type,
            tickets,
            phases,
        })
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

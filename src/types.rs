use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use crate::error::JanusError;

pub const TICKETS_DIR: &str = ".janus";
pub const TICKETS_ITEMS_DIR: &str = ".janus/items";
pub const PLANS_DIR: &str = ".janus/plans";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TicketStatus {
    #[default]
    New,
    Next,
    InProgress,
    Complete,
    Cancelled,
}

impl TicketStatus {
    /// Returns true if this status represents a terminal state (complete or cancelled).
    /// Terminal states indicate no further work is expected on the ticket.
    ///
    /// This method delegates to `crate::status::is_terminal()` for centralized status logic.
    pub fn is_terminal(self) -> bool {
        crate::status::is_terminal(self)
    }

    /// Returns true if this status indicates work has not yet started (new or next).
    /// These are pre-work states where the ticket is queued but not actively being worked on.
    ///
    /// This method delegates to `crate::status::is_not_started()` for centralized status logic.
    pub fn is_not_started(self) -> bool {
        crate::status::is_not_started(self)
    }
}

impl fmt::Display for TicketStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TicketStatus::New => write!(f, "new"),
            TicketStatus::Next => write!(f, "next"),
            TicketStatus::InProgress => write!(f, "in_progress"),
            TicketStatus::Complete => write!(f, "complete"),
            TicketStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl FromStr for TicketStatus {
    type Err = JanusError;

    /// Parses a ticket status from a string.
    ///
    /// Parsing is case-insensitive: "new", "NEW", and "New" all parse to TicketStatus::New.
    /// Valid values: "new", "next", "in_progress", "complete", "cancelled"
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "new" => Ok(TicketStatus::New),
            "next" => Ok(TicketStatus::Next),
            "in_progress" => Ok(TicketStatus::InProgress),
            "complete" => Ok(TicketStatus::Complete),
            "cancelled" => Ok(TicketStatus::Cancelled),
            _ => Err(JanusError::InvalidStatus(s.to_string())),
        }
    }
}

pub const VALID_STATUSES: &[&str] = &["new", "next", "in_progress", "complete", "cancelled"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TicketType {
    Bug,
    Feature,
    #[default]
    Task,
    Epic,
    Chore,
}

impl fmt::Display for TicketType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TicketType::Bug => write!(f, "bug"),
            TicketType::Feature => write!(f, "feature"),
            TicketType::Task => write!(f, "task"),
            TicketType::Epic => write!(f, "epic"),
            TicketType::Chore => write!(f, "chore"),
        }
    }
}

impl FromStr for TicketType {
    type Err = JanusError;

    /// Parses a ticket type from a string.
    ///
    /// Parsing is case-insensitive: "bug", "BUG", and "Bug" all parse to TicketType::Bug.
    /// Valid values: "bug", "feature", "task", "epic", "chore"
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "bug" => Ok(TicketType::Bug),
            "feature" => Ok(TicketType::Feature),
            "task" => Ok(TicketType::Task),
            "epic" => Ok(TicketType::Epic),
            "chore" => Ok(TicketType::Chore),
            _ => Err(JanusError::InvalidTicketType(s.to_string())),
        }
    }
}

pub const VALID_TYPES: &[&str] = &["bug", "feature", "task", "epic", "chore"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TicketPriority {
    #[serde(rename = "0")]
    P0,
    #[serde(rename = "1")]
    P1,
    #[default]
    #[serde(rename = "2")]
    P2,
    #[serde(rename = "3")]
    P3,
    #[serde(rename = "4")]
    P4,
}

impl TicketPriority {
    pub fn as_num(&self) -> u8 {
        match self {
            TicketPriority::P0 => 0,
            TicketPriority::P1 => 1,
            TicketPriority::P2 => 2,
            TicketPriority::P3 => 3,
            TicketPriority::P4 => 4,
        }
    }
}

impl fmt::Display for TicketPriority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_num())
    }
}

impl FromStr for TicketPriority {
    type Err = JanusError;

    /// Parses a ticket priority from a string.
    ///
    /// Accepts numeric strings "0" through "4" (P0 = highest priority, P4 = lowest).
    /// Case is not applicable for numeric values, but maintained for API consistency.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "0" => Ok(TicketPriority::P0),
            "1" => Ok(TicketPriority::P1),
            "2" => Ok(TicketPriority::P2),
            "3" => Ok(TicketPriority::P3),
            "4" => Ok(TicketPriority::P4),
            _ => Err(JanusError::InvalidPriority(s.to_string())),
        }
    }
}

pub const VALID_PRIORITIES: &[&str] = &["0", "1", "2", "3", "4"];

pub const VALID_TICKET_FIELDS: &[&str] = &[
    "id",
    "uuid",
    "status",
    "deps",
    "links",
    "created",
    "type",
    "priority",
    "external-ref",
    "remote",
    "parent",
];

pub const IMMUTABLE_TICKET_FIELDS: &[&str] = &["id", "uuid"];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TicketMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,

    #[serde(skip)]
    pub title: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<TicketStatus>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deps: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,

    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub ticket_type: Option<TicketType>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<TicketPriority>,

    #[serde(rename = "external-ref", skip_serializing_if = "Option::is_none")]
    pub external_ref: Option<String>,

    /// Remote issue reference (e.g., "github:owner/repo/123" or "linear:org/PROJ-123")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,

    #[serde(skip)]
    pub file_path: Option<PathBuf>,

    /// Completion summary extracted from `## Completion Summary` section in body
    /// Only meaningful for tickets with status: complete
    #[serde(skip)]
    pub completion_summary: Option<String>,
}

impl TicketMetadata {
    /// Get priority as a number for sorting (defaults to 2)
    pub fn priority_num(&self) -> u8 {
        self.priority.map(|p| p.as_num()).unwrap_or(2)
    }
}

/// Helper struct for tickets with computed blockers
#[derive(Debug, Clone)]
pub struct TicketWithBlockers {
    pub metadata: TicketMetadata,
    pub open_blockers: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ticket_status_is_terminal() {
        assert!(TicketStatus::Complete.is_terminal());
        assert!(TicketStatus::Cancelled.is_terminal());
        assert!(!TicketStatus::New.is_terminal());
        assert!(!TicketStatus::Next.is_terminal());
        assert!(!TicketStatus::InProgress.is_terminal());
    }

    #[test]
    fn test_ticket_status_is_not_started() {
        assert!(TicketStatus::New.is_not_started());
        assert!(TicketStatus::Next.is_not_started());
        assert!(!TicketStatus::InProgress.is_not_started());
        assert!(!TicketStatus::Complete.is_not_started());
        assert!(!TicketStatus::Cancelled.is_not_started());
    }

    #[test]
    fn test_ticket_status_from_str_case_insensitive() {
        // Lowercase
        assert_eq!("new".parse::<TicketStatus>().unwrap(), TicketStatus::New);
        assert_eq!(
            "in_progress".parse::<TicketStatus>().unwrap(),
            TicketStatus::InProgress
        );

        // Uppercase
        assert_eq!("NEW".parse::<TicketStatus>().unwrap(), TicketStatus::New);
        assert_eq!("NEXT".parse::<TicketStatus>().unwrap(), TicketStatus::Next);
        assert_eq!(
            "IN_PROGRESS".parse::<TicketStatus>().unwrap(),
            TicketStatus::InProgress
        );
        assert_eq!(
            "COMPLETE".parse::<TicketStatus>().unwrap(),
            TicketStatus::Complete
        );
        assert_eq!(
            "CANCELLED".parse::<TicketStatus>().unwrap(),
            TicketStatus::Cancelled
        );

        // Mixed case
        assert_eq!("New".parse::<TicketStatus>().unwrap(), TicketStatus::New);
        assert_eq!("Next".parse::<TicketStatus>().unwrap(), TicketStatus::Next);
        assert_eq!(
            "In_Progress".parse::<TicketStatus>().unwrap(),
            TicketStatus::InProgress
        );

        // Invalid
        assert!("invalid".parse::<TicketStatus>().is_err());
        assert!("".parse::<TicketStatus>().is_err());
    }

    #[test]
    fn test_ticket_type_from_str_case_insensitive() {
        // Lowercase
        assert_eq!("bug".parse::<TicketType>().unwrap(), TicketType::Bug);
        assert_eq!(
            "feature".parse::<TicketType>().unwrap(),
            TicketType::Feature
        );
        assert_eq!("task".parse::<TicketType>().unwrap(), TicketType::Task);
        assert_eq!("epic".parse::<TicketType>().unwrap(), TicketType::Epic);
        assert_eq!("chore".parse::<TicketType>().unwrap(), TicketType::Chore);

        // Uppercase
        assert_eq!("BUG".parse::<TicketType>().unwrap(), TicketType::Bug);
        assert_eq!(
            "FEATURE".parse::<TicketType>().unwrap(),
            TicketType::Feature
        );
        assert_eq!("TASK".parse::<TicketType>().unwrap(), TicketType::Task);
        assert_eq!("EPIC".parse::<TicketType>().unwrap(), TicketType::Epic);
        assert_eq!("CHORE".parse::<TicketType>().unwrap(), TicketType::Chore);

        // Mixed case
        assert_eq!("Bug".parse::<TicketType>().unwrap(), TicketType::Bug);
        assert_eq!(
            "Feature".parse::<TicketType>().unwrap(),
            TicketType::Feature
        );
        assert_eq!("Task".parse::<TicketType>().unwrap(), TicketType::Task);

        // Invalid
        assert!("invalid".parse::<TicketType>().is_err());
        assert!("".parse::<TicketType>().is_err());
    }

    #[test]
    fn test_ticket_priority_from_str() {
        // Valid numeric strings
        assert_eq!("0".parse::<TicketPriority>().unwrap(), TicketPriority::P0);
        assert_eq!("1".parse::<TicketPriority>().unwrap(), TicketPriority::P1);
        assert_eq!("2".parse::<TicketPriority>().unwrap(), TicketPriority::P2);
        assert_eq!("3".parse::<TicketPriority>().unwrap(), TicketPriority::P3);
        assert_eq!("4".parse::<TicketPriority>().unwrap(), TicketPriority::P4);

        // Invalid
        assert!("5".parse::<TicketPriority>().is_err());
        assert!("-1".parse::<TicketPriority>().is_err());
        assert!("p0".parse::<TicketPriority>().is_err());
        assert!("P0".parse::<TicketPriority>().is_err());
        assert!("".parse::<TicketPriority>().is_err());
    }
}

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use crate::error::JanusError;

pub const TICKETS_DIR: &str = ".janus";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TicketStatus {
    #[default]
    New,
    Complete,
    Cancelled,
}

impl fmt::Display for TicketStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TicketStatus::New => write!(f, "new"),
            TicketStatus::Complete => write!(f, "complete"),
            TicketStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl FromStr for TicketStatus {
    type Err = JanusError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "new" => Ok(TicketStatus::New),
            "complete" => Ok(TicketStatus::Complete),
            "cancelled" => Ok(TicketStatus::Cancelled),
            _ => Err(JanusError::InvalidStatus(s.to_string())),
        }
    }
}

pub const VALID_STATUSES: &[&str] = &["new", "complete", "cancelled"];

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

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "bug" => Ok(TicketType::Bug),
            "feature" => Ok(TicketType::Feature),
            "task" => Ok(TicketType::Task),
            "epic" => Ok(TicketType::Epic),
            "chore" => Ok(TicketType::Chore),
            _ => Err(JanusError::Other(format!("invalid ticket type: {}", s))),
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

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "0" => Ok(TicketPriority::P0),
            "1" => Ok(TicketPriority::P1),
            "2" => Ok(TicketPriority::P2),
            "3" => Ok(TicketPriority::P3),
            "4" => Ok(TicketPriority::P4),
            _ => Err(JanusError::Other(format!("invalid priority: {}", s))),
        }
    }
}

pub const VALID_PRIORITIES: &[&str] = &["0", "1", "2", "3", "4"];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TicketMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

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

    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,

    #[serde(rename = "external-ref", skip_serializing_if = "Option::is_none")]
    pub external_ref: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,

    #[serde(skip)]
    pub file_path: Option<PathBuf>,
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

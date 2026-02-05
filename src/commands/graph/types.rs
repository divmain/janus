//! Shared types for graph generation

use crate::error::{JanusError, Result};

/// Output format for the graph
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GraphFormat {
    #[default]
    Dot,
    Mermaid,
}

impl std::str::FromStr for GraphFormat {
    type Err = JanusError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "dot" => Ok(GraphFormat::Dot),
            "mermaid" => Ok(GraphFormat::Mermaid),
            _ => Err(JanusError::Other(format!(
                "Invalid graph format '{s}'. Must be 'dot' or 'mermaid'"
            ))),
        }
    }
}

/// What types of relationships to include in the graph
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RelationshipFilter {
    /// Only show dependency relationships (blocks/blocked-by)
    Deps,
    /// Only show spawning relationships (parent/child via spawned_from)
    Spawn,
    /// Show both deps and spawning relationships
    #[default]
    All,
}

/// Edge in the graph
#[derive(Debug, Clone)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub edge_type: EdgeType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeType {
    /// Dependency: from blocks to (to must complete before from can start)
    Blocks,
    /// Spawning: from spawned to
    Spawned,
}

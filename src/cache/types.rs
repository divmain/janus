//! Cache-specific types for representing cached data.
//!
//! These types are lightweight representations optimized for fast queries,
//! distinct from the full domain types used for file operations.

use crate::plan::types::{HasPhaseContent, HasPhaseIdentity};

/// Cached plan metadata - a lightweight representation for fast queries.
///
/// This type is optimized for reading from the cache database rather than
/// for full plan operations.
#[derive(Debug, Clone)]
pub struct CachedPlanMetadata {
    pub id: Option<String>,
    pub uuid: Option<String>,
    pub title: Option<String>,
    pub created: Option<String>,
    /// "simple", "phased", or "empty"
    pub structure_type: String,
    /// For simple plans: the ordered list of ticket IDs
    pub tickets: Vec<String>,
    /// For phased plans: phase information with their tickets
    pub phases: Vec<CachedPhase>,
}

impl CachedPlanMetadata {
    /// Get all tickets across all phases (or from tickets field for simple plans).
    pub fn all_tickets(&self) -> Vec<&str> {
        if self.structure_type == "simple" {
            self.tickets.iter().map(|s| s.as_str()).collect()
        } else {
            self.phases
                .iter()
                .flat_map(|p| p.tickets.iter().map(|s| s.as_str()))
                .collect()
        }
    }

    /// Check if this is a phased plan.
    pub fn is_phased(&self) -> bool {
        self.structure_type == "phased"
    }

    /// Check if this is a simple plan.
    pub fn is_simple(&self) -> bool {
        self.structure_type == "simple"
    }
}

/// Cached phase information for phased plans.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CachedPhase {
    pub number: String,
    pub name: String,
    pub tickets: Vec<String>,
}

impl HasPhaseIdentity for CachedPhase {
    fn number(&self) -> &str {
        &self.number
    }

    fn name(&self) -> &str {
        &self.name
    }
}

impl HasPhaseContent for CachedPhase {
    fn tickets(&self) -> &[String] {
        &self.tickets
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cached_plan_metadata_all_tickets_phased() {
        let plan = CachedPlanMetadata {
            id: Some("plan-test".to_string()),
            uuid: None,
            title: Some("Test Plan".to_string()),
            created: None,
            structure_type: "phased".to_string(),
            tickets: vec![],
            phases: vec![
                CachedPhase {
                    number: "1".to_string(),
                    name: "Phase One".to_string(),
                    tickets: vec!["t1".to_string(), "t2".to_string()],
                },
                CachedPhase {
                    number: "2".to_string(),
                    name: "Phase Two".to_string(),
                    tickets: vec!["t3".to_string()],
                },
            ],
        };

        let all = plan.all_tickets();
        assert_eq!(all, vec!["t1", "t2", "t3"]);
        assert!(plan.is_phased());
        assert!(!plan.is_simple());
    }

    #[tokio::test]
    async fn test_cached_plan_metadata_all_tickets_simple() {
        let plan = CachedPlanMetadata {
            id: Some("plan-test".to_string()),
            uuid: None,
            title: Some("Test Plan".to_string()),
            created: None,
            structure_type: "simple".to_string(),
            tickets: vec!["t1".to_string(), "t2".to_string(), "t3".to_string()],
            phases: vec![],
        };

        let all = plan.all_tickets();
        assert_eq!(all, vec!["t1", "t2", "t3"]);
        assert!(!plan.is_phased());
        assert!(plan.is_simple());
    }

    // ============================================================
    // Phase Identity Trait Tests
    // ============================================================

    #[test]
    fn test_cached_phase_has_phase_identity() {
        let phase = CachedPhase {
            number: "1".to_string(),
            name: "Infrastructure".to_string(),
            tickets: vec![],
        };

        assert_eq!(phase.number(), "1");
        assert_eq!(phase.name(), "Infrastructure");
    }

    #[test]
    fn test_cached_phase_has_phase_content() {
        let phase = CachedPhase {
            number: "1".to_string(),
            name: "Test".to_string(),
            tickets: vec!["j-a1b2".to_string(), "j-c3d4".to_string()],
        };

        let tickets = phase.tickets();
        assert_eq!(tickets, &["j-a1b2", "j-c3d4"]);
    }

    #[test]
    fn test_cached_phase_get_phase_identity_generic() {
        let phase = CachedPhase {
            number: "2a".to_string(),
            name: "Sync Part A".to_string(),
            tickets: vec![],
        };

        let (num, name) = crate::plan::types::get_phase_identity(&phase);
        assert_eq!(num, "2a");
        assert_eq!(name, "Sync Part A");
    }

    #[test]
    fn test_cached_phase_get_phase_tickets_generic() {
        let phase = CachedPhase {
            number: "1".to_string(),
            name: "Test".to_string(),
            tickets: vec!["j-a1b2".to_string(), "j-c3d4".to_string()],
        };

        let tickets = crate::plan::types::get_phase_tickets(&phase);
        assert_eq!(tickets, &["j-a1b2", "j-c3d4"]);
    }
}

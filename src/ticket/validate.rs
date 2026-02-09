use crate::types::{TicketId, TicketMetadata};

/// Enforce filename-stem-is-authoritative policy for ticket IDs.
///
/// Compares the frontmatter `id` field (if present) against the filename stem.
/// - If the frontmatter ID exists and differs from the filename stem, a warning
///   is printed to stderr and the ID is overwritten with the filename stem.
/// - If the frontmatter ID is missing, the filename stem is used.
/// - If they match, no action is taken.
///
/// This function is the single source of truth for this policy and must be
/// called by every code path that loads ticket metadata from disk.
pub fn enforce_filename_authority(metadata: &mut TicketMetadata, filename_stem: &str) {
    match &metadata.id {
        Some(frontmatter_id) if frontmatter_id.as_ref() != filename_stem => {
            eprintln!(
                "Warning: ticket file '{filename_stem}' has frontmatter id '{frontmatter_id}' — \
                 using filename stem as authoritative ID",
            );
            metadata.id = Some(TicketId::new_unchecked(filename_stem));
        }
        None => {
            metadata.id = Some(TicketId::new_unchecked(filename_stem));
        }
        Some(_) => {
            // IDs match, no action needed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TicketMetadata;

    #[test]
    fn test_enforce_filename_authority_matching_ids() {
        let mut metadata = TicketMetadata {
            id: Some(TicketId::new_unchecked("j-a1b2")),
            ..Default::default()
        };
        enforce_filename_authority(&mut metadata, "j-a1b2");
        assert_eq!(metadata.id.as_deref(), Some("j-a1b2"));
    }

    #[test]
    fn test_enforce_filename_authority_missing_id() {
        let mut metadata = TicketMetadata {
            id: None,
            ..Default::default()
        };
        enforce_filename_authority(&mut metadata, "j-a1b2");
        assert_eq!(metadata.id.as_deref(), Some("j-a1b2"));
    }

    #[test]
    fn test_enforce_filename_authority_mismatched_id() {
        let mut metadata = TicketMetadata {
            id: Some(TicketId::new_unchecked("j-wrong")),
            ..Default::default()
        };
        enforce_filename_authority(&mut metadata, "j-a1b2");
        // Filename stem wins
        assert_eq!(metadata.id.as_deref(), Some("j-a1b2"));
    }
}

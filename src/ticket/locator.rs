use crate::cache;
use crate::error::{JanusError, Result};
use crate::types::TICKETS_ITEMS_DIR;
use std::fs;
use std::path::PathBuf;

pub fn find_tickets() -> Vec<String> {
    fs::read_dir(TICKETS_ITEMS_DIR)
        .ok()
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    if name.ends_with(".md") {
                        Some(name)
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

fn validate_partial_id(id: &str) -> Result<String> {
    let trimmed = id.trim();

    if trimmed.is_empty() {
        return Err(JanusError::EmptyTicketId);
    }

    if !trimmed
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(JanusError::InvalidTicketIdCharacters);
    }

    Ok(trimmed.to_string())
}

fn find_ticket_by_id_impl(partial_id: &str) -> Result<PathBuf> {
    let files = find_tickets();

    let exact_name = format!("{}.md", partial_id);
    if files.iter().any(|f| f == &exact_name) {
        return Ok(PathBuf::from(TICKETS_ITEMS_DIR).join(&exact_name));
    }

    let matches: Vec<_> = files.iter().filter(|f| f.contains(partial_id)).collect();

    match matches.len() {
        0 => Err(JanusError::TicketNotFound(partial_id.to_string())),
        1 => Ok(PathBuf::from(TICKETS_ITEMS_DIR).join(matches[0])),
        _ => Err(JanusError::AmbiguousId(partial_id.to_string())),
    }
}

pub async fn find_ticket_by_id(partial_id: &str) -> Result<PathBuf> {
    let partial_id = validate_partial_id(partial_id)?;

    if let Some(cache) = cache::get_or_init_cache().await {
        let exact_match_path = PathBuf::from(TICKETS_ITEMS_DIR).join(format!("{}.md", partial_id));
        if exact_match_path.exists() {
            return Ok(exact_match_path);
        }

        if let Ok(matches) = cache.find_by_partial_id(&partial_id).await {
            match matches.len() {
                0 => {}
                1 => {
                    let filename = format!("{}.md", &matches[0]);
                    return Ok(PathBuf::from(TICKETS_ITEMS_DIR).join(filename));
                }
                _ => return Err(JanusError::AmbiguousId(partial_id.to_string())),
            }
        }
    }

    find_ticket_by_id_impl(&partial_id)
}

#[derive(Debug, Clone)]
pub struct TicketLocator {
    pub file_path: PathBuf,
    pub id: String,
}

impl TicketLocator {
    pub fn new(file_path: PathBuf) -> Self {
        let id = file_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        TicketLocator { file_path, id }
    }

    pub async fn find(partial_id: &str) -> Result<Self> {
        let file_path = find_ticket_by_id(partial_id).await?;
        Ok(TicketLocator::new(file_path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_partial_id_empty() {
        let result = validate_partial_id("");
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::EmptyTicketId => {}
            _ => panic!("Expected EmptyTicketId error for empty ID"),
        }
    }

    #[test]
    fn test_validate_partial_id_whitespace() {
        let result = validate_partial_id("   ");
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::EmptyTicketId => {}
            _ => panic!("Expected EmptyTicketId error for whitespace-only ID"),
        }
    }

    #[test]
    fn test_validate_partial_id_special_chars() {
        let result = validate_partial_id("j@b1");
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::InvalidTicketIdCharacters => {}
            _ => panic!("Expected InvalidTicketIdCharacters error for invalid characters"),
        }
    }

    #[test]
    fn test_validate_partial_id_valid() {
        let result = validate_partial_id("j-a1b2");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "j-a1b2");
    }

    #[test]
    fn test_validate_partial_id_valid_with_underscore() {
        let result = validate_partial_id("j_a1b2");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "j_a1b2");
    }

    #[test]
    fn test_validate_partial_id_trims_whitespace() {
        let result = validate_partial_id("  j-a1b2  ");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "j-a1b2");
    }
}

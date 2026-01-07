use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::cache;
use crate::error::{JanusError, Result};
use crate::parser::parse_ticket_content;
use crate::types::{TICKETS_ITEMS_DIR, TicketMetadata};

/// Find all ticket files in the tickets directory
fn find_tickets() -> Vec<String> {
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

/// Find a ticket file by partial ID
pub async fn find_ticket_by_id(partial_id: &str) -> Result<PathBuf> {
    // Try cache first
    if let Some(cache) = cache::get_or_init_cache().await {
        // Exact match check - file exists?
        let exact_match_path = PathBuf::from(TICKETS_ITEMS_DIR).join(format!("{}.md", partial_id));
        if exact_match_path.exists() {
            return Ok(exact_match_path);
        }

        // Partial match via cache
        if let Ok(matches) = cache.find_by_partial_id(partial_id).await {
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

    // FALLBACK: Original file-based implementation
    let files = find_tickets();

    // Check for exact match first
    let exact_name = format!("{}.md", partial_id);
    if files.iter().any(|f| f == &exact_name) {
        return Ok(PathBuf::from(TICKETS_ITEMS_DIR).join(&exact_name));
    }

    // Then check for partial matches
    let matches: Vec<_> = files.iter().filter(|f| f.contains(partial_id)).collect();

    match matches.len() {
        0 => Err(JanusError::TicketNotFound(partial_id.to_string())),
        1 => Ok(PathBuf::from(TICKETS_ITEMS_DIR).join(matches[0])),
        _ => Err(JanusError::AmbiguousId(partial_id.to_string())),
    }
}

/// Find a ticket file by partial ID (sync wrapper for backward compatibility)
pub fn find_ticket_by_id_sync(partial_id: &str) -> Result<PathBuf> {
    // Try to use cache if we're in a tokio runtime, otherwise use fallback
    use tokio::runtime::Handle;

    // Check if we're already in a tokio runtime
    if Handle::try_current().is_err() {
        // Not in a tokio runtime, create one
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| JanusError::Other(format!("Failed to create tokio runtime: {}", e)))?;
        return rt.block_on(find_ticket_by_id(partial_id));
    }

    // We're in a tokio runtime, cannot use block_on
    // This shouldn't happen if called from async functions properly
    // Fall back to file-based implementation
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

/// A ticket handle for reading and writing ticket files
pub struct Ticket {
    pub file_path: PathBuf,
    pub id: String,
}

impl Ticket {
    /// Find a ticket by its (partial) ID
    pub fn find(partial_id: &str) -> Result<Self> {
        let file_path = find_ticket_by_id_sync(partial_id)?;
        Ok(Ticket::new(file_path))
    }

    /// Find a ticket by its (partial) ID (async version)
    pub async fn find_async(partial_id: &str) -> Result<Self> {
        let file_path = find_ticket_by_id(partial_id).await?;
        Ok(Ticket::new(file_path))
    }

    /// Create a ticket handle for a given file path
    pub fn new(file_path: PathBuf) -> Self {
        let id = file_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ticket { file_path, id }
    }

    /// Read and parse the ticket's metadata
    pub fn read(&self) -> Result<TicketMetadata> {
        let content = fs::read_to_string(&self.file_path)?;
        let mut metadata = parse_ticket_content(&content)?;
        metadata.file_path = Some(self.file_path.clone());
        Ok(metadata)
    }

    /// Read the raw content of the ticket file
    pub fn read_content(&self) -> Result<String> {
        Ok(fs::read_to_string(&self.file_path)?)
    }

    /// Write content to the ticket file
    pub fn write(&self, content: &str) -> Result<()> {
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.file_path, content)?;
        Ok(())
    }

    /// Update a single field in the frontmatter
    pub fn update_field(&self, field: &str, value: &str) -> Result<()> {
        let content = fs::read_to_string(&self.file_path)?;
        let field_pattern = Regex::new(&format!(r"(?m)^{}:\s*.*$", regex::escape(field))).unwrap();

        let new_content = if field_pattern.is_match(&content) {
            field_pattern
                .replace(&content, format!("{}: {}", field, value))
                .into_owned()
        } else {
            // Add field after opening ---
            content.replacen("---\n", &format!("---\n{}: {}\n", field, value), 1)
        };

        fs::write(&self.file_path, new_content)?;
        Ok(())
    }

    /// Add a value to an array field (deps, links)
    pub fn add_to_array_field(&self, field: &str, value: &str) -> Result<bool> {
        let metadata = self.read()?;
        let current_array = match field {
            "deps" => &metadata.deps,
            "links" => &metadata.links,
            _ => return Err(JanusError::Other(format!("unknown array field: {}", field))),
        };

        if current_array.contains(&value.to_string()) {
            return Ok(false);
        }

        let mut new_array = current_array.clone();
        new_array.push(value.to_string());
        self.update_field(field, &serde_json::to_string(&new_array)?)?;
        Ok(true)
    }

    /// Remove a value from an array field (deps, links)
    pub fn remove_from_array_field(&self, field: &str, value: &str) -> Result<bool> {
        let metadata = self.read()?;
        let current_array = match field {
            "deps" => &metadata.deps,
            "links" => &metadata.links,
            _ => return Err(JanusError::Other(format!("unknown array field: {}", field))),
        };

        if !current_array.contains(&value.to_string()) {
            return Ok(false);
        }

        let new_array: Vec<_> = current_array
            .iter()
            .filter(|v| v.as_str() != value)
            .collect();
        let json_value = if new_array.is_empty() {
            "[]".to_string()
        } else {
            serde_json::to_string(&new_array)?
        };
        self.update_field(field, &json_value)?;
        Ok(true)
    }
}

/// Get all tickets from the tickets directory (original implementation, used as fallback)
pub fn get_all_tickets_from_disk() -> Vec<TicketMetadata> {
    let files = find_tickets();
    let mut tickets = Vec::new();

    for file in files {
        let file_path = PathBuf::from(TICKETS_ITEMS_DIR).join(&file);
        match fs::read_to_string(&file_path) {
            Ok(content) => match parse_ticket_content(&content) {
                Ok(mut metadata) => {
                    metadata.id = Some(file.strip_suffix(".md").unwrap_or(&file).to_string());
                    metadata.file_path = Some(file_path);
                    tickets.push(metadata);
                }
                Err(e) => {
                    eprintln!("Warning: failed to parse {}: {}", file, e);
                }
            },
            Err(e) => {
                eprintln!("Warning: failed to read {}: {}", file, e);
            }
        }
    }

    tickets
}

/// Get all tickets from the tickets directory
pub async fn get_all_tickets() -> Vec<TicketMetadata> {
    // Try cache first
    if let Some(cache) = cache::get_or_init_cache().await {
        if let Ok(tickets) = cache.get_all_tickets().await {
            return tickets;
        }
        eprintln!("Warning: cache read failed, falling back to file reads");
    }

    // FALLBACK: Original implementation
    get_all_tickets_from_disk()
}

/// Get all tickets (sync wrapper for backward compatibility)
pub fn get_all_tickets_sync() -> Vec<TicketMetadata> {
    use tokio::runtime::Handle;

    if Handle::try_current().is_err() {
        let rt = tokio::runtime::Runtime::new().ok();
        if let Some(rt) = rt {
            return rt.block_on(get_all_tickets());
        }
    }

    // Fallback to disk reads
    get_all_tickets_from_disk()
}

/// Build a map of all tickets by ID
pub async fn build_ticket_map() -> HashMap<String, TicketMetadata> {
    // Try cache first
    if let Some(cache) = cache::get_or_init_cache().await {
        if let Ok(map) = cache.build_ticket_map().await {
            return map;
        }
        eprintln!("Warning: cache read failed, falling back to file reads");
    }

    // FALLBACK: Use get_all_tickets which has cache logic
    get_all_tickets()
        .await
        .into_iter()
        .filter_map(|t| t.id.clone().map(|id| (id, t)))
        .collect()
}

/// Build a map of all tickets by ID (sync wrapper for backward compatibility)
pub fn build_ticket_map_sync() -> HashMap<String, TicketMetadata> {
    use tokio::runtime::Handle;

    if Handle::try_current().is_err() {
        let rt = tokio::runtime::Runtime::new().ok();
        if let Some(rt) = rt {
            return rt.block_on(build_ticket_map());
        }
    }

    // Fallback to disk reads
    get_all_tickets_from_disk()
        .into_iter()
        .filter_map(|t| t.id.clone().map(|id| (id, t)))
        .collect()
}

/// Get file stats (modification time) for a path
pub fn get_file_mtime(path: &Path) -> Option<std::time::SystemTime> {
    fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

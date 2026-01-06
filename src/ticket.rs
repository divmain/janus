use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{JanusError, Result};
use crate::parser::parse_ticket_content;
use crate::types::{TICKETS_DIR, TicketMetadata};

/// Find all ticket files in the tickets directory
fn find_tickets() -> Vec<String> {
    fs::read_dir(TICKETS_DIR)
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
fn find_ticket_by_id(partial_id: &str) -> Result<PathBuf> {
    let files = find_tickets();

    // Check for exact match first
    let exact_name = format!("{}.md", partial_id);
    if files.iter().any(|f| f == &exact_name) {
        return Ok(PathBuf::from(TICKETS_DIR).join(&exact_name));
    }

    // Then check for partial matches
    let matches: Vec<_> = files.iter().filter(|f| f.contains(partial_id)).collect();

    match matches.len() {
        0 => Err(JanusError::TicketNotFound(partial_id.to_string())),
        1 => Ok(PathBuf::from(TICKETS_DIR).join(matches[0])),
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
        let file_path = find_ticket_by_id(partial_id)?;
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

/// Get all tickets from the tickets directory
pub fn get_all_tickets() -> Vec<TicketMetadata> {
    let files = find_tickets();
    let mut tickets = Vec::new();

    for file in files {
        let file_path = PathBuf::from(TICKETS_DIR).join(&file);
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

/// Build a map of all tickets by ID
pub fn build_ticket_map() -> HashMap<String, TicketMetadata> {
    get_all_tickets()
        .into_iter()
        .filter_map(|t| t.id.clone().map(|id| (id, t)))
        .collect()
}

/// Get file stats (modification time) for a path
pub fn get_file_mtime(path: &Path) -> Option<std::time::SystemTime> {
    fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

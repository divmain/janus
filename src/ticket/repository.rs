use crate::repository::ItemRepository;
use crate::ticket::content;
use crate::utils::DirScanner;
use crate::{Ticket, TicketMetadata, cache};
use async_trait::async_trait;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub fn find_tickets() -> Result<Vec<String>, std::io::Error> {
    use crate::types::tickets_items_dir;

    DirScanner::find_markdown_files(tickets_items_dir())
}

pub struct TicketRepository;

#[async_trait]
impl ItemRepository for TicketRepository {
    type Item = Ticket;
    type Metadata = TicketMetadata;

    async fn get_all() -> Result<Vec<TicketMetadata>, crate::error::JanusError> {
        if let Some(cache) = cache::get_or_init_cache().await {
            if let Ok(tickets) = cache.get_all_tickets().await {
                return Ok(tickets);
            }
            eprintln!("Warning: cache read failed, falling back to file reads");
        }

        get_all_tickets_from_disk().map_err(crate::error::JanusError::Io)
    }
}

impl TicketRepository {
    fn get_all_from_disk() -> Result<Vec<TicketMetadata>, std::io::Error> {
        use crate::types::tickets_items_dir;

        let files = find_tickets()?;
        let mut tickets = Vec::new();
        let items_dir = tickets_items_dir();

        for file in files {
            let file_path = items_dir.join(&file);
            match fs::read_to_string(&file_path) {
                Ok(content_str) => match content::parse(&content_str) {
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

        Ok(tickets)
    }

    pub async fn build_map() -> Result<HashMap<String, TicketMetadata>, crate::error::JanusError> {
        if let Some(cache) = cache::get_or_init_cache().await {
            if let Ok(map) = cache.build_ticket_map().await {
                return Ok(map);
            }
            eprintln!("Warning: cache read failed, falling back to file reads");
        }

        // Use the trait method for consistency
        <Self as ItemRepository>::build_map().await
    }

    pub async fn get_all_with_map()
    -> Result<(Vec<TicketMetadata>, HashMap<String, TicketMetadata>), crate::error::JanusError>
    {
        // Use the trait method for efficiency
        <Self as ItemRepository>::get_all_with_map().await
    }
}

pub async fn get_all_tickets() -> Result<Vec<TicketMetadata>, crate::error::JanusError> {
    TicketRepository::get_all().await
}

pub fn get_all_tickets_from_disk() -> Result<Vec<TicketMetadata>, std::io::Error> {
    TicketRepository::get_all_from_disk()
}

pub async fn build_ticket_map() -> Result<HashMap<String, TicketMetadata>, crate::error::JanusError>
{
    TicketRepository::build_map().await
}

pub async fn get_all_tickets_with_map()
-> Result<(Vec<TicketMetadata>, HashMap<String, TicketMetadata>), crate::error::JanusError> {
    TicketRepository::get_all_with_map().await
}

pub fn get_file_mtime(path: &Path) -> Option<std::time::SystemTime> {
    DirScanner::get_file_mtime(path)
}

/// Get the count of tickets spawned from a given ticket.
///
/// This function uses the cache when available, falling back to
/// scanning all tickets and counting matches.
pub async fn get_children_count(ticket_id: &str) -> Result<usize, crate::error::JanusError> {
    if let Some(cache) = cache::get_or_init_cache().await {
        if let Ok(count) = cache.get_children_count(ticket_id).await {
            return Ok(count);
        }
        eprintln!("Warning: cache read failed, falling back to file reads");
    }

    // Fallback: scan all tickets and count matches
    let tickets = get_all_tickets().await?;
    Ok(tickets
        .iter()
        .filter(|t| t.spawned_from.as_ref() == Some(&ticket_id.to_string()))
        .count())
}

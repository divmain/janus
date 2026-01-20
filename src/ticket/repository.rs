use crate::ticket::content;
use crate::utils::DirScanner;
use crate::{TicketMetadata, cache};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub fn find_tickets() -> Vec<String> {
    use crate::types::TICKETS_ITEMS_DIR;

    DirScanner::find_markdown_files(TICKETS_ITEMS_DIR).unwrap_or_else(|e| {
        eprintln!("Warning: failed to read tickets directory: {}", e);
        Vec::new()
    })
}

pub struct TicketRepository;

impl TicketRepository {
    pub async fn get_all() -> Vec<TicketMetadata> {
        if let Some(cache) = cache::get_or_init_cache().await {
            if let Ok(tickets) = cache.get_all_tickets().await {
                return tickets;
            }
            eprintln!("Warning: cache read failed, falling back to file reads");
        }

        Self::get_all_from_disk()
    }

    fn get_all_from_disk() -> Vec<TicketMetadata> {
        use crate::types::TICKETS_ITEMS_DIR;

        let files = find_tickets();
        let mut tickets = Vec::new();

        for file in files {
            let file_path = PathBuf::from(TICKETS_ITEMS_DIR).join(&file);
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

        tickets
    }

    pub async fn build_map() -> HashMap<String, TicketMetadata> {
        if let Some(cache) = cache::get_or_init_cache().await {
            if let Ok(map) = cache.build_ticket_map().await {
                return map;
            }
            eprintln!("Warning: cache read failed, falling back to file reads");
        }

        Self::get_all()
            .await
            .into_iter()
            .filter_map(|t| t.id.clone().map(|id| (id, t)))
            .collect()
    }

    pub async fn get_all_with_map() -> (Vec<TicketMetadata>, HashMap<String, TicketMetadata>) {
        let tickets = Self::get_all().await;
        let map = tickets
            .iter()
            .filter_map(|t| t.id.clone().map(|id| (id, t.clone())))
            .collect();
        (tickets, map)
    }
}

pub async fn get_all_tickets() -> Vec<TicketMetadata> {
    TicketRepository::get_all().await
}

pub fn get_all_tickets_from_disk() -> Vec<TicketMetadata> {
    TicketRepository::get_all_from_disk()
}

pub async fn build_ticket_map() -> HashMap<String, TicketMetadata> {
    TicketRepository::build_map().await
}

pub async fn get_all_tickets_with_map() -> (Vec<TicketMetadata>, HashMap<String, TicketMetadata>) {
    TicketRepository::get_all_with_map().await
}

pub fn get_file_mtime(path: &Path) -> Option<std::time::SystemTime> {
    DirScanner::get_file_mtime(path)
}

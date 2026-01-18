use crate::ticket::content::TicketContent;
use crate::ticket::locator::find_tickets;
use crate::{TicketMetadata, cache};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct TicketRepository;

impl TicketRepository {
    pub fn get_all() -> Vec<TicketMetadata> {
        use tokio::runtime::Handle;

        if Handle::try_current().is_err() {
            let rt = tokio::runtime::Runtime::new().ok();
            if let Some(rt) = rt {
                return rt.block_on(async { Self::get_all_async().await });
            }
        }

        Self::get_all_from_disk()
    }

    pub async fn get_all_async() -> Vec<TicketMetadata> {
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
                Ok(content) => match TicketContent::parse(&content) {
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

    pub fn build_map() -> HashMap<String, TicketMetadata> {
        use tokio::runtime::Handle;

        if Handle::try_current().is_err() {
            let rt = tokio::runtime::Runtime::new().ok();
            if let Some(rt) = rt {
                return rt.block_on(async { Self::build_map_async().await });
            }
        }

        Self::get_all_from_disk()
            .into_iter()
            .filter_map(|t| t.id.clone().map(|id| (id, t)))
            .collect()
    }

    pub async fn build_map_async() -> HashMap<String, TicketMetadata> {
        if let Some(cache) = cache::get_or_init_cache().await {
            if let Ok(map) = cache.build_ticket_map().await {
                return map;
            }
            eprintln!("Warning: cache read failed, falling back to file reads");
        }

        Self::get_all_async()
            .await
            .into_iter()
            .filter_map(|t| t.id.clone().map(|id| (id, t)))
            .collect()
    }
}

pub async fn get_all_tickets() -> Vec<TicketMetadata> {
    TicketRepository::get_all_async().await
}

pub fn get_all_tickets_sync() -> Vec<TicketMetadata> {
    TicketRepository::get_all()
}

pub fn get_all_tickets_from_disk() -> Vec<TicketMetadata> {
    TicketRepository::get_all_from_disk()
}

pub async fn build_ticket_map() -> HashMap<String, TicketMetadata> {
    TicketRepository::build_map_async().await
}

pub fn build_ticket_map_sync() -> HashMap<String, TicketMetadata> {
    TicketRepository::build_map()
}

pub fn get_file_mtime(path: &Path) -> Option<std::time::SystemTime> {
    fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

//! Semantic search command implementation
//!
//! This command performs semantic search over tickets using vector embeddings
//! to find tickets semantically similar to the query text.

use crate::cache::get_or_init_store;
use crate::cache::search::SearchResult;
use crate::commands::print_json;
use crate::error::{JanusError, Result};
use crate::remote::config::Config;
use serde_json::json;
use tabled::settings::Style;
use tabled::{Table, Tabled};

/// A row in the search results table
#[derive(Tabled)]
struct SearchResultRow {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "Similarity")]
    similarity: String,
    #[tabled(rename = "Title")]
    title: String,
    #[tabled(rename = "Status")]
    status: String,
}

/// Execute the search command
///
/// Performs semantic search over all tickets with embeddings and displays
/// results ordered by similarity score.
pub async fn cmd_search(
    query: &str,
    limit: usize,
    threshold: Option<f32>,
    json: bool,
) -> Result<()> {
    // Validate query is not empty
    if query.trim().is_empty() {
        return Err(JanusError::InvalidInput(
            "Search query cannot be empty".to_string(),
        ));
    }

    // Check if semantic search is enabled
    let config = Config::load()?;
    if !config.semantic_search_enabled() {
        eprintln!(
            "Semantic search is disabled. Enable with: janus config set semantic_search.enabled true"
        );
        return Err(JanusError::Config(
            "Semantic search is disabled".to_string(),
        ));
    }

    let store = get_or_init_store().await?;

    // Check embedding coverage
    let (with_embedding, total) = store.embedding_coverage();

    if total == 0 {
        return Err(JanusError::EmbeddingsNotAvailable);
    }

    if with_embedding == 0 {
        return Err(JanusError::EmbeddingsNotAvailable);
    }

    if with_embedding < total {
        let percentage = (with_embedding * 100) / total;
        eprintln!(
            "Warning: Only {with_embedding}/{total} tickets have embeddings ({percentage}%). Search results may be incomplete."
        );
        eprintln!("Run 'janus cache rebuild' to generate embeddings for all tickets.");
    }

    // Generate query embedding and perform search
    let query_embedding = crate::embedding::model::generate_embedding(query)
        .await
        .map_err(JanusError::EmbeddingModel)?;
    let results = store.semantic_search(&query_embedding, limit);

    // Filter by threshold if specified
    let results: Vec<SearchResult> = if let Some(t) = threshold {
        results.into_iter().filter(|r| r.similarity >= t).collect()
    } else {
        results
    };

    // Output results
    if json {
        // Output as JSON
        let json_results: Vec<serde_json::Value> = results
            .iter()
            .map(|r| {
                json!({
                    "ticket": {
                        "id": r.ticket.id.as_ref(),
                        "title": r.ticket.title.as_ref(),
                        "status": r.ticket.status.map(|s| s.to_string()),
                        "type": r.ticket.ticket_type.map(|t| t.to_string()),
                        "priority": r.ticket.priority.map(|p| p.to_string()),
                        "size": r.ticket.size.map(|s| s.to_string()),
                    },
                    "similarity": r.similarity,
                })
            })
            .collect();
        print_json(&json!(json_results))?;
    } else {
        // Output as formatted table
        println!("Search results for: \"{query}\"\n");

        if results.is_empty() {
            println!("No matching tickets found.");
        } else {
            let rows: Vec<SearchResultRow> = results
                .iter()
                .map(|r| SearchResultRow {
                    id: r.ticket.id.as_deref().unwrap_or("unknown").to_string(),
                    similarity: format!("{:.2}", r.similarity),
                    title: r
                        .ticket
                        .title
                        .as_deref()
                        .unwrap_or("(no title)")
                        .to_string(),
                    status: r
                        .ticket
                        .status
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "unknown".to_string()),
                })
                .collect();

            let mut table = Table::new(rows);
            table.with(Style::rounded());
            println!("{table}");
        }

        println!("\n{} result(s)", results.len());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_empty_query_error() {
        let result = cmd_search("", 10, None, false).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("cannot be empty"));
    }

    #[tokio::test]
    async fn test_whitespace_query_error() {
        let result = cmd_search("   ", 10, None, false).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("cannot be empty"));
    }
}

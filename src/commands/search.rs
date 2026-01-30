//! Semantic search command implementation
//!
//! This command performs semantic search over tickets using vector embeddings
//! to find tickets semantically similar to the query text.

use crate::cache::get_or_init_cache;
use crate::commands::print_json;
use crate::embedding::search::SearchResult;
use crate::error::{JanusError, Result};
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
        return Err(JanusError::Other(
            "Search query cannot be empty".to_string(),
        ));
    }

    let cache = get_or_init_cache()
        .await
        .ok_or_else(|| JanusError::CacheNotAvailable)?;

    // Check embedding coverage
    let (with_embedding, total) = cache.embedding_coverage().await?;

    if total == 0 {
        return Err(JanusError::EmbeddingsNotAvailable);
    }

    if with_embedding == 0 {
        return Err(JanusError::Other(
            "No ticket embeddings available. Run 'janus cache rebuild' with semantic-search feature enabled.".to_string()
        ));
    }

    if with_embedding < total {
        let percentage = (with_embedding * 100) / total;
        eprintln!(
            "Warning: Only {}/{} tickets have embeddings ({}%). Search results may be incomplete.",
            with_embedding, total, percentage
        );
        eprintln!("Run 'janus cache rebuild' to generate embeddings for all tickets.");
    }

    // Check for model version mismatch
    let needs_reembed = cache.needs_reembedding().await.unwrap_or(false);
    if needs_reembed {
        eprintln!("Warning: Embedding model version mismatch detected.");
        eprintln!("Run 'janus cache rebuild' to update embeddings to the current model.");
    }

    // Perform search
    let results = cache.semantic_search(query, limit).await?;

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
        println!("Search results for: \"{}\"\n", query);

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
            println!("{}", table);
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

use tabled::settings::Style;
use tabled::{Table, Tabled};

use crate::cli::OutputOptions;
use crate::commands::print_json;
use crate::config::Config;
use crate::error::{JanusError, Result};
use crate::store::get_or_init_store;

/// A row in the document search results table
#[derive(Tabled)]
struct DocSearchRow {
    #[tabled(rename = "Label")]
    label: String,
    #[tabled(rename = "Similarity")]
    similarity: String,
    #[tabled(rename = "Title")]
    title: String,
    #[tabled(rename = "Lines")]
    lines: String,
}

/// Search documents using semantic similarity
pub async fn cmd_doc_search(
    query: &str,
    document: Option<&str>,
    limit: usize,
    threshold: Option<f32>,
    output: OutputOptions,
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

    // Resolve document label if specified
    let resolved_label = if let Some(doc_label) = document {
        // Try to find the document by exact or partial match
        let docs: Vec<_> = store
            .docs()
            .iter()
            .filter(|entry| {
                let label = entry.key();
                label == doc_label || label.starts_with(doc_label)
            })
            .map(|entry| entry.key().clone())
            .collect();

        match docs.as_slice() {
            [] => {
                return Err(JanusError::DocNotFound(format!(
                    "No document found matching '{doc_label}'"
                )));
            }
            [single] => Some(single.clone()),
            multiple => {
                return Err(JanusError::AmbiguousDocLabel(
                    doc_label.to_string(),
                    multiple.to_vec(),
                ));
            }
        }
    } else {
        None
    };

    // Check embedding coverage
    let doc_embeddings: Vec<_> = store
        .embeddings()
        .iter()
        .filter(|entry| entry.key().starts_with("doc:"))
        .collect();

    if doc_embeddings.is_empty() {
        return Err(JanusError::DocNotFound(
            "No documents with embeddings found. Run 'janus cache rebuild' to generate embeddings."
                .to_string(),
        ));
    }

    // Generate query embedding and perform search
    let query_embedding = crate::embedding::model::generate_embedding(query)
        .await
        .map_err(JanusError::EmbeddingModel)?;

    let results = match resolved_label {
        Some(ref label) => store.doc_search_by_document(&query_embedding, label, limit),
        None => store.doc_search(&query_embedding, limit),
    };

    // Filter by threshold if specified
    let results: Vec<_> = if let Some(t) = threshold {
        results.into_iter().filter(|r| r.similarity >= t).collect()
    } else {
        results
    };

    // Output results
    if output.json {
        let json_results: Vec<serde_json::Value> = results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "label": r.label,
                    "title": r.doc.title(),
                    "description": r.doc.description,
                    "heading_path": r.heading_path,
                    "content_snippet": r.content_snippet,
                    "line_range": r.line_range,
                    "similarity": r.similarity,
                })
            })
            .collect();
        print_json(&serde_json::json!(json_results))?;
    } else {
        let target_info = if let Some(ref label) = resolved_label {
            format!(" in document '{label}'")
        } else {
            String::new()
        };
        println!("Document search results for: \"{query}\"{target_info}\n");

        if results.is_empty() {
            println!("No matching documents found.");
        } else {
            let rows: Vec<DocSearchRow> = results
                .iter()
                .map(|r| DocSearchRow {
                    label: r.label.clone(),
                    similarity: format!("{:.2}", r.similarity),
                    title: r.doc.title().unwrap_or("(no title)").to_string(),
                    lines: if r.line_range.0 == 0 {
                        "-".to_string()
                    } else {
                        format!("{}-{}", r.line_range.0, r.line_range.1)
                    },
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

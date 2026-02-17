use tabled::settings::Style;
use tabled::{Table, Tabled};

use crate::cli::OutputOptions;
use crate::commands::{display_init_warnings, print_json};
use crate::error::Result;
use crate::store::get_or_init_store;

/// A row in the document list table
#[derive(Tabled)]
struct DocRow {
    #[tabled(rename = "Label")]
    label: String,
    #[tabled(rename = "Title")]
    title: String,
    #[tabled(rename = "Description")]
    description: String,
    #[tabled(rename = "Tags")]
    tags: String,
}

/// List all documents with metadata
pub async fn cmd_doc_ls(output: OutputOptions) -> Result<()> {
    let store = get_or_init_store().await?;

    if !output.json {
        display_init_warnings(store);
    }

    let docs: Vec<_> = store
        .docs()
        .iter()
        .map(|entry| entry.value().clone())
        .collect();

    if output.json {
        let json_docs: Vec<serde_json::Value> = docs
            .iter()
            .map(|doc| {
                serde_json::json!({
                    "label": doc.label(),
                    "title": doc.title(),
                    "description": doc.description,
                    "tags": doc.tags,
                    "created": doc.created.as_ref().map(|c| c.to_string()),
                    "updated": doc.updated.as_ref().map(|c| c.to_string()),
                })
            })
            .collect();
        print_json(&serde_json::json!(json_docs))?;
    } else {
        if docs.is_empty() {
            println!("No documents found.");
            return Ok(());
        }

        let rows: Vec<DocRow> = docs
            .iter()
            .map(|doc| DocRow {
                label: doc.label().unwrap_or("(no label)").to_string(),
                title: doc.title().unwrap_or("(no title)").to_string(),
                description: doc.description.clone().unwrap_or_default(),
                tags: if doc.tags.is_empty() {
                    "-".to_string()
                } else {
                    doc.tags.join(", ")
                },
            })
            .collect();

        let mut table = Table::new(rows);
        table.with(Style::rounded());
        println!("{table}");

        println!("\n{} document(s)", docs.len());
    }

    Ok(())
}

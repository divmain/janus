use crate::cli::OutputOptions;
use crate::commands::CommandOutput;
use crate::doc::types::{DocLabel, DocMetadata};
use crate::doc::{Doc, ensure_docs_dir};
use crate::error::Result;
use crate::utils::open_in_editor;

/// Create a new document
pub async fn cmd_doc_create(
    label: &str,
    title: Option<String>,
    description: Option<String>,
    tags: Vec<String>,
    output: OutputOptions,
) -> Result<()> {
    // Ensure docs directory exists
    ensure_docs_dir()?;

    // Validate and sanitize the label
    let label = DocLabel::new(label)?;
    let file_path = crate::paths::docs_dir().join(format!("{label}.md"));

    // Check if document already exists
    if file_path.exists() {
        return Err(crate::error::JanusError::DocAlreadyExists(
            label.to_string(),
        ));
    }

    // Create initial metadata
    let now_str = {
        use jiff::Timestamp;
        let now = Timestamp::now();
        now.strftime("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
    };
    let now = crate::types::CreatedAt::new_unchecked(&now_str);
    let doc_title = title.unwrap_or_else(|| label.to_string());

    let metadata = DocMetadata {
        label: Some(label.clone()),
        description,
        tags,
        created: Some(now.clone()),
        updated: Some(now.clone()),
        title: Some(doc_title.clone()),
        file_path: Some(file_path.clone()),
        extra_frontmatter: None,
    };

    // Create the document
    let doc = Doc::new(file_path.clone())?;
    let content = crate::doc::parser::serialize_doc(&metadata)?;
    doc.write(&content)?;

    // Refresh store
    if let Some(store) = crate::store::get_store() {
        store.upsert_doc(metadata.clone());
    }

    // Open in editor if interactive
    if !output.json {
        if crate::utils::is_stdin_tty() {
            open_in_editor(&file_path)?;
        } else {
            println!("Created document: {}", file_path.to_string_lossy());
        }
    }

    CommandOutput::new(serde_json::json!({
        "label": label.to_string(),
        "title": doc_title,
        "file_path": file_path.to_string_lossy().to_string(),
    }))
    .with_text(label.to_string())
    .print(output)
}

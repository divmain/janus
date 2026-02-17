use crate::cli::OutputOptions;
use crate::commands::{CommandOutput, open_in_editor_for_entity};
use crate::doc::Doc;
use crate::error::Result;

/// Edit an existing document
pub async fn cmd_doc_edit(label: &str, output: OutputOptions) -> Result<()> {
    let doc = Doc::find(label).await?;

    // Output in JSON format if requested (skip editor)
    if output.json {
        return CommandOutput::new(serde_json::json!({
            "label": doc.label,
            "file_path": doc.file_path.to_string_lossy(),
            "action": "edit",
        }))
        .print(output);
    }

    open_in_editor_for_entity("doc", &doc.file_path, output)
}

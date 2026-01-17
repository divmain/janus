/// Format options for ticket display
#[derive(Default)]
pub struct FormatOptions {
    pub show_priority: bool,
    pub suffix: Option<String>,
}

/// Format dependencies for display
pub fn format_deps(deps: &[String]) -> String {
    let deps_str = deps.join(", ");
    if deps_str.is_empty() {
        " <- []".to_string()
    } else {
        format!(" <- [{}]", deps_str)
    }
}

/// Sort tickets by priority (ascending) then by ID
pub fn sort_by_priority(tickets: &mut [crate::types::TicketMetadata]) {
    tickets.sort_by(|a, b| {
        let pa = a.priority_num();
        let pb = b.priority_num();
        if pa != pb {
            pa.cmp(&pb)
        } else {
            a.id.cmp(&b.id)
        }
    });
}

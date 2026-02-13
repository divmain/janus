use std::collections::{HashMap, HashSet};

use owo_colors::OwoColorize;

use crate::types::TicketMetadata;

pub struct TreeBuilder;

impl TreeBuilder {
    pub fn build_json_tree(
        id: &str,
        path: &mut HashSet<String>,
        ticket_map: &HashMap<String, TicketMetadata>,
        ticket_minimal_fn: &dyn Fn(&str, Option<&TicketMetadata>) -> serde_json::Value,
    ) -> serde_json::Value {
        let ticket = ticket_map.get(id);

        let deps_json: Vec<serde_json::Value> = if path.contains(id) {
            vec![]
        } else {
            path.insert(id.to_string());
            let deps = ticket_map
                .get(id)
                .map(|t| &t.deps)
                .cloned()
                .unwrap_or_default();
            let result: Vec<_> = deps
                .iter()
                .map(|dep| Self::build_json_tree(dep, path, ticket_map, ticket_minimal_fn))
                .collect();
            path.remove(id);
            result
        };

        let mut base = ticket_minimal_fn(id, ticket);
        base["deps"] = serde_json::to_value(deps_json).expect("JSON serialization should succeed");
        base
    }
}

pub struct DepthCalculator;

impl DepthCalculator {
    pub fn calculate_depths(
        root: &str,
        ticket_map: &HashMap<String, TicketMetadata>,
    ) -> (HashMap<String, usize>, HashMap<String, usize>) {
        let mut max_depth: HashMap<String, usize> = HashMap::new();
        let mut subtree_depth: HashMap<String, usize> = HashMap::new();

        let mut path = HashSet::new();
        Self::find_max_depth(root, 0, &mut path, &mut max_depth, ticket_map);
        Self::compute_subtree_depth(root, &max_depth, &mut subtree_depth, ticket_map);

        (max_depth, subtree_depth)
    }

    fn find_max_depth(
        id: &str,
        current_depth: usize,
        path: &mut HashSet<String>,
        max_depth: &mut HashMap<String, usize>,
        ticket_map: &HashMap<String, TicketMetadata>,
    ) {
        if path.contains(id) {
            return;
        }

        let current_max = max_depth.get(id).copied().unwrap_or(0);
        max_depth.insert(id.to_string(), current_max.max(current_depth));

        if let Some(ticket) = ticket_map.get(id) {
            path.insert(id.to_string());
            for dep in &ticket.deps {
                Self::find_max_depth(dep, current_depth + 1, path, max_depth, ticket_map);
            }
            path.remove(id);
        }
    }

    fn compute_subtree_depth(
        id: &str,
        max_depth: &HashMap<String, usize>,
        subtree_depth: &mut HashMap<String, usize>,
        ticket_map: &HashMap<String, TicketMetadata>,
    ) -> usize {
        let mut max = max_depth.get(id).copied().unwrap_or(0);
        if let Some(ticket) = ticket_map.get(id) {
            for dep in &ticket.deps {
                max = max.max(Self::compute_subtree_depth(
                    dep,
                    max_depth,
                    subtree_depth,
                    ticket_map,
                ));
            }
        }
        subtree_depth.insert(id.to_string(), max);
        max
    }
}

pub struct TreeFormatter<'a> {
    ticket_map: &'a HashMap<String, TicketMetadata>,
    max_depth: &'a HashMap<String, usize>,
    subtree_depth: &'a HashMap<String, usize>,
}

impl<'a> TreeFormatter<'a> {
    pub fn new(
        ticket_map: &'a HashMap<String, TicketMetadata>,
        max_depth: &'a HashMap<String, usize>,
        subtree_depth: &'a HashMap<String, usize>,
    ) -> Self {
        Self {
            ticket_map,
            max_depth,
            subtree_depth,
        }
    }

    pub fn print_root(&self, id: &str) {
        let ticket = self.ticket_map.get(id);
        let status = ticket
            .and_then(|t| t.status)
            .map(|s| s.to_string())
            .unwrap_or_else(|| "?".to_string());
        let title = ticket
            .and_then(|t| t.title.as_ref())
            .map(|s| s.as_str())
            .unwrap_or("");

        println!("{} [{}] {}", id.cyan(), status, title);
    }

    pub fn print_tree(&self, id: &str, depth: usize, prefix: &str, full_mode: bool) {
        let children = self.get_printable_children(id, depth, full_mode);

        for (i, child) in children.iter().enumerate() {
            let is_last = i == children.len() - 1;
            let connector = if is_last { "└── " } else { "├── " };
            let child_prefix = if is_last { "    " } else { "│   " };

            let ticket = self.ticket_map.get(child);
            let status = ticket
                .and_then(|t| t.status)
                .map(|s| s.to_string())
                .unwrap_or_else(|| "?".to_string());
            let title = ticket
                .and_then(|t| t.title.as_ref())
                .map(|s| s.as_str())
                .unwrap_or("");

            println!(
                "{}{}{} [{}] {}",
                prefix.dimmed(),
                connector.dimmed(),
                child.cyan(),
                status,
                title
            );

            self.print_tree(
                child,
                depth + 1,
                &format!("{prefix}{child_prefix}"),
                full_mode,
            );
        }
    }

    fn get_printable_children(&self, id: &str, depth: usize, full_mode: bool) -> Vec<String> {
        let deps = self
            .ticket_map
            .get(id)
            .map(|t| &t.deps)
            .cloned()
            .unwrap_or_default();

        let mut children: Vec<String> = deps
            .into_iter()
            .map(|dep| dep.to_string())
            .filter(|dep| {
                if !self.max_depth.contains_key(dep) {
                    return false;
                }
                full_mode || depth + 1 == self.max_depth.get(dep).copied().unwrap_or(0)
            })
            .collect();

        children.sort_by(|a, b| {
            let depth_diff = self
                .subtree_depth
                .get(b)
                .copied()
                .unwrap_or(0)
                .cmp(&self.subtree_depth.get(a).copied().unwrap_or(0));
            if depth_diff != std::cmp::Ordering::Equal {
                depth_diff
            } else {
                a.cmp(b)
            }
        });

        children
    }
}

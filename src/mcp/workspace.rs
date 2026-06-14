//! Multi-workspace registry for the MCP server.
//!
//! By default a single `janus mcp` server is rooted at one `.janus/` directory
//! (the ambient [`janus_root`]). Registering workspaces with
//! `janus mcp --workspace <name>=<path>` lets one server address several
//! `.janus/` roots, so an agent can read tickets across repositories (a
//! monorepo's packages, or sibling repos) without launching a server per root.
//!
//! A tool's optional `workspace` argument selects a registered workspace by
//! name; omitting it (or naming the configured default) uses the ambient root,
//! which keeps every existing single-root caller unchanged.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::types::janus_root;

/// Error parsing a `--workspace name=path` specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceSpecError {
    /// The spec was not of the form `name=path`.
    Malformed(String),
    /// The name (left of `=`) was empty.
    EmptyName(String),
    /// The path (right of `=`) was empty.
    EmptyPath(String),
    /// The same workspace name was given more than once.
    Duplicate(String),
}

impl std::fmt::Display for WorkspaceSpecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed(s) => write!(
                f,
                "workspace spec {s:?} must be of the form name=path (e.g. api=/repo/api/.janus)"
            ),
            Self::EmptyName(s) => write!(f, "workspace spec {s:?} has an empty name"),
            Self::EmptyPath(s) => write!(f, "workspace spec {s:?} has an empty path"),
            Self::Duplicate(name) => write!(f, "workspace {name:?} was specified more than once"),
        }
    }
}

impl std::error::Error for WorkspaceSpecError {}

/// A registry of named Janus workspaces (each a `.janus/` root), plus the
/// ambient root used when no workspace is named.
#[derive(Debug, Clone, Default)]
pub struct WorkspaceRegistry {
    /// name → `.janus/` root. `BTreeMap` so `names()` is stable/sorted.
    roots: BTreeMap<String, PathBuf>,
}

impl WorkspaceRegistry {
    /// An empty registry: only the ambient root is addressable.
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a registry from `name=path` specs (the `--workspace` flag values).
    ///
    /// Each `path` is the workspace's `.janus/` directory. Names must be unique
    /// and non-empty; paths must be non-empty. Returns the first error found.
    pub fn from_specs<I, S>(specs: I) -> Result<Self, WorkspaceSpecError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut roots = BTreeMap::new();
        for spec in specs {
            let spec = spec.as_ref();
            let (name, path) = spec
                .split_once('=')
                .ok_or_else(|| WorkspaceSpecError::Malformed(spec.to_string()))?;
            let name = name.trim();
            let path = path.trim();
            if name.is_empty() {
                return Err(WorkspaceSpecError::EmptyName(spec.to_string()));
            }
            if path.is_empty() {
                return Err(WorkspaceSpecError::EmptyPath(spec.to_string()));
            }
            if roots.contains_key(name) {
                return Err(WorkspaceSpecError::Duplicate(name.to_string()));
            }
            roots.insert(name.to_string(), PathBuf::from(path));
        }
        Ok(Self { roots })
    }

    /// Whether any named workspaces are registered.
    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }

    /// The registered workspace names, sorted.
    pub fn names(&self) -> Vec<String> {
        self.roots.keys().cloned().collect()
    }

    /// All registered workspaces as `(name, root)` pairs, sorted by name.
    pub fn entries(&self) -> Vec<(String, PathBuf)> {
        self.roots
            .iter()
            .map(|(n, p)| (n.clone(), p.clone()))
            .collect()
    }

    /// Resolve a tool's optional `workspace` argument to a `.janus/` root.
    ///
    /// - `None` → the ambient [`janus_root`] (the single-root default).
    /// - `Some(name)` of a registered workspace → that workspace's root.
    /// - `Some(name)` that is unknown → `Err` naming the known set, so the tool
    ///   surfaces a clear message instead of silently reading the wrong root.
    pub fn resolve(&self, workspace: Option<&str>) -> Result<PathBuf, String> {
        match workspace {
            None => Ok(janus_root()),
            Some(name) => self.roots.get(name).cloned().ok_or_else(|| {
                let known = if self.roots.is_empty() {
                    "(none registered; start the server with --workspace name=path)".to_string()
                } else {
                    self.names().join(", ")
                };
                format!(
                    "unknown workspace {name:?}. Known workspaces: {known}. \
                     Omit `workspace` to use the default root."
                )
            }),
        }
    }

    /// True when `workspace` names a non-ambient root (a registered workspace),
    /// i.e. the operation must target a root other than [`janus_root`]. Used by
    /// tools that do not yet support non-ambient roots to reject early with a
    /// clear message rather than write to the wrong place.
    pub fn is_non_ambient(&self, workspace: Option<&str>) -> bool {
        match self.resolve(workspace) {
            Ok(root) => root != janus_root(),
            // An unknown name is handled by resolve()'s error elsewhere; treat
            // it as non-ambient so the caller routes to the error path.
            Err(_) => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_specs_parses_name_path_pairs() {
        let reg = WorkspaceRegistry::from_specs(["api=/repo/api/.janus", "web=/repo/web/.janus"])
            .expect("valid specs");
        assert_eq!(reg.names(), vec!["api", "web"]);
        assert_eq!(
            reg.resolve(Some("api")).unwrap(),
            PathBuf::from("/repo/api/.janus")
        );
    }

    #[test]
    fn from_specs_rejects_malformed_and_empty() {
        assert!(matches!(
            WorkspaceRegistry::from_specs(["noequalssign"]),
            Err(WorkspaceSpecError::Malformed(_))
        ));
        assert!(matches!(
            WorkspaceRegistry::from_specs(["=/p/.janus"]),
            Err(WorkspaceSpecError::EmptyName(_))
        ));
        assert!(matches!(
            WorkspaceRegistry::from_specs(["api="]),
            Err(WorkspaceSpecError::EmptyPath(_))
        ));
    }

    #[test]
    fn from_specs_rejects_duplicate_names() {
        assert!(matches!(
            WorkspaceRegistry::from_specs(["api=/a/.janus", "api=/b/.janus"]),
            Err(WorkspaceSpecError::Duplicate(_))
        ));
    }

    #[test]
    fn resolve_none_is_ambient_root() {
        let reg = WorkspaceRegistry::new();
        assert_eq!(reg.resolve(None).unwrap(), janus_root());
    }

    #[test]
    fn resolve_unknown_names_the_known_set() {
        let reg = WorkspaceRegistry::from_specs(["api=/repo/api/.janus"]).unwrap();
        let err = reg.resolve(Some("nope")).unwrap_err();
        assert!(err.contains("unknown workspace"));
        assert!(err.contains("api"));
    }

    #[test]
    fn is_non_ambient_flags_registered_but_not_default() {
        let reg = WorkspaceRegistry::from_specs(["api=/repo/api/.janus"]).unwrap();
        assert!(!reg.is_non_ambient(None));
        assert!(reg.is_non_ambient(Some("api")));
        // Unknown names route to the error path (treated as non-ambient).
        assert!(reg.is_non_ambient(Some("nope")));
    }
}

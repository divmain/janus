//! Declarative builder for TUI shortcuts

use super::Shortcut;

/// Builder for creating shortcut lists with common patterns
#[derive(Default)]
pub struct ShortcutsBuilder {
    shortcuts: Vec<Shortcut>,
    has_navigation: bool,
    has_search: bool,
    has_edit: bool,
    has_quit: bool,
}

impl ShortcutsBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add j/k, g/G for navigation
    pub fn with_navigation(mut self) -> Self {
        self.has_navigation = true;
        self.shortcuts.push(Shortcut::new("j/k", "Up/Down"));
        self.shortcuts.push(Shortcut::new("g/G", "Top/Bottom"));
        self.shortcuts
            .push(Shortcut::new("PgUp/PgDn", "Page Up/Dn"));
        self
    }

    /// Add / for search, Esc to clear
    pub fn with_search(mut self) -> Self {
        self.has_search = true;
        self.shortcuts.push(Shortcut::new("/", "Search"));
        self.shortcuts.push(Shortcut::new("Esc", "Clear"));
        self
    }

    /// Add e for edit, n for new
    pub fn with_edit(mut self) -> Self {
        self.has_edit = true;
        self.shortcuts.push(Shortcut::new("e", "Edit"));
        self.shortcuts.push(Shortcut::new("n", "New"));
        self
    }

    /// Add Ctrl+q for quit
    pub fn with_quit(mut self) -> Self {
        self.has_quit = true;
        self.shortcuts.push(Shortcut::new("C-q", "Quit"));
        self
    }

    /// Add a single custom shortcut
    pub fn add(mut self, key: &str, description: &str) -> Self {
        self.shortcuts.push(Shortcut::new(key, description));
        self
    }

    /// Add multiple custom shortcuts at once
    pub fn add_all<'a>(mut self, shortcuts: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        for (key, description) in shortcuts {
            self.shortcuts.push(Shortcut::new(key, description));
        }
        self
    }

    /// Build the shortcuts vector
    pub fn build(self) -> Vec<Shortcut> {
        self.shortcuts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_navigation_shortcuts() {
        let shortcuts = ShortcutsBuilder::new().with_navigation().build();

        assert_eq!(shortcuts.len(), 3);
        assert!(shortcuts.iter().any(|s| s.key == "j/k"));
        assert!(shortcuts.iter().any(|s| s.key == "g/G"));
        assert!(shortcuts.iter().any(|s| s.key == "PgUp/PgDn"));
    }

    #[test]
    fn test_full_browser_shortcuts() {
        let shortcuts = ShortcutsBuilder::new()
            .with_navigation()
            .with_search()
            .with_edit()
            .with_quit()
            .add("s", "Cycle Status")
            .add("Tab", "Switch Pane")
            .add("C-t", "Triage")
            .build();

        assert_eq!(shortcuts.len(), 11);
        assert!(shortcuts.iter().any(|s| s.key == "j/k"));
        assert!(shortcuts.iter().any(|s| s.key == "/"));
        assert!(shortcuts.iter().any(|s| s.key == "e"));
        assert!(shortcuts.iter().any(|s| s.key == "C-q"));
        assert!(shortcuts.iter().any(|s| s.key == "s"));
        assert!(shortcuts.iter().any(|s| s.key == "Tab"));
        assert!(shortcuts.iter().any(|s| s.key == "C-t"));
    }

    #[test]
    fn test_empty_shortcuts() {
        let shortcuts = ShortcutsBuilder::new().build();

        assert_eq!(shortcuts.len(), 0);
    }
}

//! Markdown formatting and normalization using rumdl.
//!
//! Provides functions to normalize markdown content using rumdl's linting rules.
//! Configured to use only formatting rules that don't change semantic content.

use std::collections::BTreeMap;

use rumdl_lib::HeadingStyle;
use rumdl_lib::{
    config::{Config, RuleConfig},
    fix_coordinator::FixCoordinator,
    rule::Rule,
    rules::{
        ListStyle, MD003HeadingStyle, MD004UnorderedListStyle, MD005ListIndent, MD007ULIndent,
        MD009TrailingSpaces, MD010NoHardTabs, MD012NoMultipleBlanks, MD022BlanksAroundHeadings,
        MD023HeadingStartLeft, MD029OrderedListPrefix, MD031BlanksAroundFences,
        MD032BlanksAroundLists, MD038NoSpaceInCode, MD039NoSpaceInLinks,
        MD047SingleTrailingNewline, MD064NoMultipleConsecutiveSpaces, MD069NoDuplicateListMarkers,
        MD071BlankLineAfterFrontmatter, UnorderedListStyle,
    },
};

/// Format and normalize markdown content using rumdl rules.
///
/// Applies safe formatting rules that preserve content while ensuring
/// consistent whitespace, heading formatting, list styles, etc.
///
/// # Arguments
/// * `content` - The markdown content to format
///
/// # Returns
/// The formatted markdown content, or the original content if formatting fails.
pub fn format_markdown(content: &str) -> String {
    // Get the configured rules
    let rules = get_formatting_rules();

    // Create the fix coordinator
    let coordinator = FixCoordinator::new();

    // Apply fixes iteratively
    let mut content = content.to_string();

    // Create configuration for rules
    let config = create_formatting_config();

    match coordinator.apply_fixes_iterative(
        &rules,
        &[], // No pre-computed warnings
        &mut content,
        &config,
        10,   // Max iterations
        None, // No file path needed for in-memory processing
    ) {
        Ok(_) => content,
        Err(e) => {
            // Log the error but return the original content
            tracing::warn!("Markdown formatting failed: {}", e);
            content
        }
    }
}

/// Create the configuration for formatting rules.
fn create_formatting_config() -> Config {
    let mut config = Config::default();
    let mut rules: BTreeMap<String, RuleConfig> = BTreeMap::new();

    // Configure MD003: Use ATX style headings
    let mut md003_config = RuleConfig::default();
    md003_config
        .values
        .insert("style".to_string(), "atx".into());
    rules.insert("MD003".to_string(), md003_config);

    // Configure MD004: Use dashes for unordered lists
    let mut md004_config = RuleConfig::default();
    md004_config
        .values
        .insert("style".to_string(), "dash".into());
    rules.insert("MD004".to_string(), md004_config);

    // Configure MD009: Allow 2 trailing spaces for intentional line breaks
    let mut md009_config = RuleConfig::default();
    md009_config
        .values
        .insert("br_spaces".to_string(), 2i64.into());
    md009_config
        .values
        .insert("strict".to_string(), false.into());
    rules.insert("MD009".to_string(), md009_config);

    // Configure MD010: Convert tabs to 4 spaces
    let mut md010_config = RuleConfig::default();
    md010_config
        .values
        .insert("spaces_per_tab".to_string(), 4i64.into());
    rules.insert("MD010".to_string(), md010_config);

    // Configure MD012: Allow up to 2 consecutive blank lines
    let mut md012_config = RuleConfig::default();
    md012_config
        .values
        .insert("maximum".to_string(), 2i64.into());
    rules.insert("MD012".to_string(), md012_config);

    // Configure MD022: Blank lines around headings
    let mut md022_config = RuleConfig::default();
    md022_config
        .values
        .insert("lines_above".to_string(), 1i64.into());
    md022_config
        .values
        .insert("lines_below".to_string(), 1i64.into());
    md022_config
        .values
        .insert("allowed_at_start".to_string(), true.into());
    rules.insert("MD022".to_string(), md022_config);

    // Configure MD029: Allow both one-based and ordered numbering
    let mut md029_config = RuleConfig::default();
    md029_config
        .values
        .insert("style".to_string(), "one-or-ordered".into());
    rules.insert("MD029".to_string(), md029_config);

    // Configure MD031: Blanks around fences, including in list items
    let mut md031_config = RuleConfig::default();
    md031_config
        .values
        .insert("list_items".to_string(), true.into());
    rules.insert("MD031".to_string(), md031_config);

    config.rules = rules;
    config
}

/// Get the list of formatting rules to apply.
///
/// These are content-preserving rules that only affect formatting,
/// whitespace, and style consistency.
fn get_formatting_rules() -> Vec<Box<dyn Rule>> {
    vec![
        // Heading rules
        Box::new(MD003HeadingStyle::new(HeadingStyle::Atx)),
        Box::new(MD022BlanksAroundHeadings::new()),
        Box::new(MD023HeadingStartLeft),
        // List rules
        Box::new(MD004UnorderedListStyle::new(UnorderedListStyle::Dash)),
        Box::new(MD005ListIndent::default()),
        Box::new(MD007ULIndent::default()),
        Box::new(MD029OrderedListPrefix::new(ListStyle::OneOrOrdered)),
        Box::new(MD032BlanksAroundLists::default()),
        Box::new(MD069NoDuplicateListMarkers::new()),
        // Whitespace rules
        Box::new(MD009TrailingSpaces::new(2, false)),
        Box::new(MD010NoHardTabs::new(4)),
        Box::new(MD012NoMultipleBlanks::new(2)),
        Box::new(MD031BlanksAroundFences::new(true)),
        Box::new(MD047SingleTrailingNewline),
        Box::new(MD064NoMultipleConsecutiveSpaces::new()),
        // Formatting rules
        Box::new(MD038NoSpaceInCode::new()),
        Box::new(MD039NoSpaceInLinks::new()),
        // Frontmatter rules
        Box::new(MD071BlankLineAfterFrontmatter::new()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_basic_markdown() {
        let input = r#"# Title
Description here.
## Section
Content."#;

        let result = format_markdown(input);

        // Should have blank lines around headings
        assert!(result.contains("# Title\n\n"));
        assert!(result.contains("\n\n## Section\n\n"));
    }

    #[test]
    fn test_format_preserves_content() {
        let input = r#"# Plan Title

This is the description.

## Acceptance Criteria

- Criterion 1
- Criterion 2

## Phase 1: Infrastructure

### Success Criteria

- Database tables created

### Tickets

1. j-a1b2
2. j-c3d4
"#;

        let result = format_markdown(input);

        // Content should be preserved
        assert!(result.contains("Plan Title"));
        assert!(result.contains("Acceptance Criteria"));
        assert!(result.contains("Criterion 1"));
        assert!(result.contains("j-a1b2"));
        assert!(result.contains("j-c3d4"));
    }

    #[test]
    fn test_format_list_style_consistency() {
        let input = r#"# Title

* Item 1
* Item 2
+ Item 3
- Item 4"#;

        let result = format_markdown(input);

        // All items should use dashes
        assert!(result.contains("- Item 1"));
        assert!(result.contains("- Item 2"));
        assert!(result.contains("- Item 3"));
        assert!(result.contains("- Item 4"));
    }

    #[test]
    fn test_format_removes_trailing_spaces() {
        let input = "# Title   \nContent   \n";

        let result = format_markdown(input);

        // Should not have trailing spaces
        assert!(!result.contains("Title   "));
        assert!(!result.contains("Content   "));
    }

    #[test]
    fn test_format_frontmatter_blank_line() {
        let input = r#"---
id: plan-123
---
# Title

Content."#;

        let result = format_markdown(input);

        // Should have blank line after frontmatter
        assert!(result.contains("---\n\n# Title"));
    }
}

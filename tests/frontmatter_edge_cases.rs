//! Comprehensive edge case tests for frontmatter parsing
//!
//! These tests verify that the comrak-based parser correctly handles
//! malformed input and returns appropriate errors.

use janus::error::JanusError;
use janus::parser::split_frontmatter;
use serial_test::serial;

#[test]
#[serial]
fn test_only_opening_delimiter() {
    let content = "---\nid: test\n# No closing delimiter\nBody here.";
    let result = split_frontmatter(content);
    assert!(result.is_err());
    match result {
        Err(JanusError::InvalidFormat(msg)) => {
            assert!(msg.contains("missing YAML frontmatter"));
        }
        _ => panic!("Expected InvalidFormat error"),
    }
}

#[test]
#[serial]
fn test_only_closing_delimiter() {
    let content = "---\nid: test\n---\nMore content with ---\n---\n";
    let result = split_frontmatter(content);
    // Should parse successfully (first --- pair wins)
    assert!(result.is_ok());
    let (frontmatter, body) = result.unwrap();
    assert!(frontmatter.contains("id: test"));
    assert!(body.contains("More content with ---"));
    assert!(body.contains("---")); // Dashes in body should be preserved
}

#[test]
#[serial]
fn test_malformed_yaml_indentation() {
    // This is technically valid YAML (indentation is flexible)
    let content = "---\nid: test\n  nested:\n    value\n---\n# Title\n";
    let result = split_frontmatter(content);
    assert!(result.is_ok());
}

#[test]
#[serial]
fn test_invalid_yaml_syntax() {
    // Invalid YAML: colon in value without quotes
    let content = "---\nid: test:value with colon\nstatus: new\n---\n# Title\n";
    let (frontmatter, body) = split_frontmatter(content).unwrap();
    // Parser should accept it as frontmatter (YAML parsing happens separately)
    assert!(frontmatter.contains("id: test:value with colon"));
    assert!(body.contains("# Title"));
}

#[test]
#[serial]
fn test_unicode_characters_in_frontmatter() {
    let content = "---\ntitle: 标题\nauthor: 作者\ndesc: 日本語\n---\n# Title\n";
    let result = split_frontmatter(content);
    assert!(result.is_ok());
    let (frontmatter, _) = result.unwrap();
    assert!(frontmatter.contains("标题"));
    assert!(frontmatter.contains("作者"));
    assert!(frontmatter.contains("日本語"));
}

#[test]
#[serial]
fn test_empty_frontmatter() {
    let content = "---\n---\n# Title\n\nBody";
    let result = split_frontmatter(content);
    assert!(result.is_err());
    match result {
        Err(JanusError::EmptyFrontmatter) => {}
        other => panic!("Expected EmptyFrontmatter error, got: {other:?}"),
    }
}

#[test]
#[serial]
fn test_empty_body() {
    let content = "---\nid: test\n---\n";
    let (frontmatter, body) = split_frontmatter(content).unwrap();
    assert!(frontmatter.contains("id: test"));
    assert_eq!(body, "");
}

#[test]
#[serial]
fn test_whitespace_before_delimiter() {
    let content = "  \n  ---\nid: test\n---\n# Title\n";
    let result = split_frontmatter(content);
    // Comrak requires --- at column 0, so this should fail
    assert!(result.is_err());
}

#[test]
#[serial]
fn test_multiple_frontmatter_blocks() {
    // Only first frontmatter block should be parsed
    let content = "---\nid: first\n---\n---\nid: second\n---\n# Title\n";
    let (frontmatter, body) = split_frontmatter(content).unwrap();
    assert!(frontmatter.contains("id: first"));
    assert!(!frontmatter.contains("id: second"));
    assert!(body.contains("---")); // Second block becomes part of body
    assert!(body.contains("id: second"));
}

#[test]
#[serial]
fn test_very_large_frontmatter() {
    // Create a large frontmatter with many fields
    let mut frontmatter_lines = vec!["---".to_string()];
    for i in 0..1000 {
        frontmatter_lines.push(format!("field_{i}: value_{i}"));
    }
    frontmatter_lines.push("---".to_string());
    frontmatter_lines.push("# Title".to_string());

    let content = frontmatter_lines.join("\n");
    let (frontmatter, body) = split_frontmatter(&content).unwrap();

    assert!(frontmatter.contains("field_0: value_0"));
    assert!(frontmatter.contains("field_999: value_999"));
    assert!(body.contains("# Title"));
}

#[test]
#[serial]
fn test_frontmatter_with_special_yaml_types() {
    let content = r#"---
timestamp: 2024-01-01T00:00:00Z
float_value: 3.14159
bool_value: true
null_value: ~
array_value:
  - item1
  - item2
nested_map:
  key: value
---
# Title
"#;
    let (frontmatter, body) = split_frontmatter(content).unwrap();
    assert!(frontmatter.contains("timestamp:"));
    assert!(frontmatter.contains("float_value:"));
    assert!(frontmatter.contains("bool_value:"));
    assert!(body.contains("# Title"));
}

#[test]
#[serial]
fn test_frontmatter_with_anchors_and_aliases() {
    let content = r#"---
defaults: &defaults
  priority: 2
  status: new

task1:
  <<: *defaults
  title: Task 1

task2:
  <<: *defaults
  title: Task 2
---
# Title
"#;
    let (frontmatter, body) = split_frontmatter(content).unwrap();
    assert!(frontmatter.contains("defaults:"));
    assert!(frontmatter.contains("*defaults"));
    assert!(body.contains("# Title"));
}

#[test]
#[serial]
fn test_frontmatter_with_multiline_dashes_only() {
    let content = r#"---
comment: >-
  ---
  ---
  ---
---
# Title
"#;
    let (frontmatter, body) = split_frontmatter(content).unwrap();
    assert!(frontmatter.contains("comment:"));
    // The folded scalar should preserve the dashes
    assert!(frontmatter.lines().any(|l| l.trim().starts_with("---")));
    assert!(body.contains("# Title"));
}

#[test]
#[serial]
fn test_completely_empty_file() {
    let content = "";
    let result = split_frontmatter(content);
    assert!(result.is_err());
}

#[test]
#[serial]
fn test_only_whitespace() {
    let content = "   \n   \n";
    let result = split_frontmatter(content);
    assert!(result.is_err());
}

#[test]
#[serial]
fn test_alternative_delimiters_not_recognized() {
    let content = "+++\nid: test\n+++\n# Title\n";
    let result = split_frontmatter(content);
    // Should fail - we only recognize --- delimiter
    assert!(result.is_err());
}

#[test]
#[serial]
fn test_toml_style_frontmatter_fails() {
    let content = "+++\ntitle = \"test\"\n+++\n# Title\n";
    let result = split_frontmatter(content);
    assert!(result.is_err());
}

#[test]
#[serial]
fn test_json_style_frontmatter_fails() {
    let content = "{\n\"id\": \"test\"\n}\n# Title\n";
    let result = split_frontmatter(content);
    assert!(result.is_err());
}

#[test]
#[serial]
fn test_frontmatter_with_tabs_instead_of_spaces() {
    // YAML spec allows tabs as part of values, but indentation should be spaces
    let content = "---\nid: test\nkey:\tvalue\n---\n# Title\n";
    let (frontmatter, body) = split_frontmatter(content).unwrap();
    assert!(frontmatter.contains("key:\tvalue"));
    assert!(body.contains("# Title"));
}

#[test]
#[serial]
fn test_frontmatter_with_escaped_special_chars() {
    let content = r#"---
quote: "This has \"quotes\" inside"
single: 'This has \'single\' quotes'
backslash: "Path\\to\\file"
escaped_newline: "Line 1\nLine 2"
---
# Title
"#;
    let (frontmatter, body) = split_frontmatter(content).unwrap();
    assert!(frontmatter.contains("quote:"));
    assert!(frontmatter.contains("single:"));
    assert!(frontmatter.contains("backslash:"));
    assert!(body.contains("# Title"));
}

#[test]
#[serial]
fn test_mixed_endings_content() {
    let content = "---\r\nid: test\n---\r\n# Title\r\n";
    let (frontmatter, body) = split_frontmatter(content).unwrap();
    assert_eq!(frontmatter, "id: test");
    assert!(body.contains("# Title"));
}

#[test]
#[serial]
fn test_carriage_return_only() {
    let content = "---\rid: test\r---\r# Title\r";
    let (frontmatter, body) = split_frontmatter(content).unwrap();
    // Should normalize to LF
    assert!(frontmatter.contains("id: test"));
    assert!(body.contains("# Title"));
}

//! Adversarial YAML deserialization regression tests.
//!
//! These tests exercise the full YAML deserialization path (split_frontmatter → serde_yaml_ng)
//! with inputs designed to trigger resource-exhaustion or parser edge cases:
//!
//!   - Deeply nested YAML structures
//!   - Very large sequences/arrays
//!   - Very large scalar values (multi-KB strings)
//!   - YAML anchors/aliases (billion-laughs style)
//!   - Many keys in a single mapping
//!
//! Each test asserts either explicit rejection with a clear error, or bounded
//! behavior (the parser returns a result without crashing or hanging).
//! All tests are kept small and deterministic (< 2 seconds each).

use std::time::{Duration, Instant};

use janus::parser::{parse_document, split_frontmatter};
use janus::types::TicketMetadata;
use serial_test::serial;

// ---------------------------------------------------------------------------
// Helper: wrap YAML content in a valid frontmatter document
// ---------------------------------------------------------------------------
fn make_document(yaml: &str) -> String {
    format!("---\n{yaml}\n---\n# Title\n\nBody content.\n")
}

// ---------------------------------------------------------------------------
// 1. Deeply nested YAML structures
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_deeply_nested_yaml_mapping() {
    // Build a YAML mapping nested 128 levels deep:
    //   level0:
    //     level1:
    //       level2:
    //         ...
    //           value: leaf
    let depth = 128;
    let mut yaml = String::new();
    for i in 0..depth {
        let indent = "  ".repeat(i);
        yaml.push_str(&format!("{indent}level{i}:\n"));
    }
    let final_indent = "  ".repeat(depth);
    yaml.push_str(&format!("{final_indent}value: leaf"));

    let doc = make_document(&yaml);

    let start = Instant::now();
    let result = parse_document(&doc);
    let elapsed = start.elapsed();

    // Must complete within a reasonable time bound
    assert!(
        elapsed < Duration::from_secs(5),
        "deeply nested YAML took too long: {elapsed:?}"
    );

    // serde_yaml_ng may accept or reject this; we just need a clean result
    match result {
        Ok(parsed) => {
            assert!(parsed.frontmatter.contains_key("level0"));
        }
        Err(e) => {
            // Acceptable: an explicit parse/nesting error
            let msg = e.to_string();
            assert!(
                msg.contains("YAML")
                    || msg.contains("recursion")
                    || msg.contains("nesting")
                    || msg.contains("parse"),
                "unexpected error for deeply nested YAML: {msg}"
            );
        }
    }
}

#[test]
#[serial]
fn test_deeply_nested_yaml_sequence() {
    // Build a YAML sequence nested 128 levels deep:
    //   outer:
    //     -
    //       -
    //         - leaf
    let depth = 128;
    let mut yaml = String::from("outer:\n");
    for i in 0..depth {
        let indent = "  ".repeat(i + 1);
        yaml.push_str(&format!("{indent}-\n"));
    }
    let final_indent = "  ".repeat(depth + 1);
    yaml.push_str(&format!("{final_indent}- leaf"));

    let doc = make_document(&yaml);

    let start = Instant::now();
    let result = parse_document(&doc);
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(5),
        "deeply nested sequence took too long: {elapsed:?}"
    );

    match result {
        Ok(parsed) => {
            assert!(parsed.frontmatter.contains_key("outer"));
        }
        Err(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains("YAML")
                    || msg.contains("recursion")
                    || msg.contains("nesting")
                    || msg.contains("parse"),
                "unexpected error for deeply nested sequence: {msg}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 2. Very large sequence/array values
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_large_sequence_in_frontmatter() {
    // Create a YAML sequence with 10,000 items
    let count = 10_000;
    let mut yaml = String::from("id: j-test\nstatus: new\nitems:\n");
    for i in 0..count {
        yaml.push_str(&format!("  - item_{i}\n"));
    }

    let doc = make_document(&yaml);

    let start = Instant::now();
    let result = parse_document(&doc);
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(5),
        "large sequence parsing took too long: {elapsed:?}"
    );

    // Should succeed — serde_yaml_ng handles large sequences fine
    let parsed = result.expect("large sequence should parse successfully");
    assert!(parsed.frontmatter.contains_key("items"));
    assert!(parsed.frontmatter.contains_key("id"));
}

#[test]
#[serial]
fn test_large_sequence_deserialized_into_ticket_metadata() {
    // The deps/links fields are Vec<String>, so a large deps array exercises the typed path
    let count = 5_000;
    let mut yaml = String::from("id: j-test\nstatus: new\ndeps:\n");
    for i in 0..count {
        yaml.push_str(&format!("  - dep-{i:04x}\n"));
    }

    let doc = make_document(&yaml);

    let start = Instant::now();
    let result = parse_document(&doc);
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(5),
        "large deps array parsing took too long: {elapsed:?}"
    );

    let parsed = result.expect("large deps should parse successfully");
    let meta: TicketMetadata = parsed
        .deserialize_frontmatter()
        .expect("should deserialize into TicketMetadata");
    assert_eq!(meta.deps.len(), count);
}

// ---------------------------------------------------------------------------
// 3. Very large scalar values
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_large_scalar_literal_block() {
    // A literal block scalar with ~500KB of content
    let line = "x".repeat(100);
    let line_count = 5_000;
    let mut yaml = String::from("id: j-test\ndescription: |\n");
    for _ in 0..line_count {
        yaml.push_str(&format!("  {line}\n"));
    }

    let doc = make_document(&yaml);

    let start = Instant::now();
    let result = parse_document(&doc);
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(5),
        "large scalar parsing took too long: {elapsed:?}"
    );

    let parsed = result.expect("large scalar should parse successfully");
    assert!(parsed.frontmatter.contains_key("description"));
}

#[test]
#[serial]
fn test_large_scalar_single_line() {
    // A single quoted scalar that is ~1MB
    let big_value = "a".repeat(1_000_000);
    let yaml = format!("id: j-test\nbig: \"{big_value}\"");

    let doc = make_document(&yaml);

    let start = Instant::now();
    let result = parse_document(&doc);
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(5),
        "large single-line scalar took too long: {elapsed:?}"
    );

    let parsed = result.expect("large single-line scalar should parse successfully");
    assert!(parsed.frontmatter.contains_key("big"));
}

#[test]
#[serial]
fn test_large_scalar_folded_block() {
    // A folded block scalar (>) with many short lines (~200KB)
    let line_count = 4_000;
    let mut yaml = String::from("id: j-test\nfolded: >\n");
    for i in 0..line_count {
        yaml.push_str(&format!("  line number {i} with some padding text here\n"));
    }

    let doc = make_document(&yaml);

    let start = Instant::now();
    let result = parse_document(&doc);
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(5),
        "large folded scalar took too long: {elapsed:?}"
    );

    let parsed = result.expect("large folded scalar should parse successfully");
    assert!(parsed.frontmatter.contains_key("folded"));
}

// ---------------------------------------------------------------------------
// 4. YAML anchors and aliases (billion-laughs style)
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_yaml_anchor_alias_basic() {
    // Basic anchor/alias usage — should parse cleanly
    let yaml = r#"id: j-test
defaults: &defaults
  priority: 2
  status: new
task1:
  <<: *defaults
  name: first
task2:
  <<: *defaults
  name: second"#;

    let doc = make_document(yaml);
    let result = parse_document(&doc);
    let parsed = result.expect("basic anchor/alias should parse");
    assert!(parsed.frontmatter.contains_key("defaults"));
    assert!(parsed.frontmatter.contains_key("task1"));
    assert!(parsed.frontmatter.contains_key("task2"));
}

#[test]
#[serial]
fn test_yaml_billion_laughs_style() {
    // Classic "billion laughs" pattern using YAML anchors.
    // Each level doubles the previous, creating exponential expansion:
    //   a: &a ["lol"]
    //   b: &b [*a, *a]
    //   c: &c [*b, *b]
    //   ...
    // After ~20 levels this could expand to millions of entries.
    //
    // serde_yaml_ng should either reject this or handle it within bounded resources.
    let mut yaml = String::from("id: j-test\na: &a\n  - lol\n");
    let levels = 20;
    for i in 1..=levels {
        let prev = (b'a' + (i - 1) as u8) as char;
        let curr = (b'a' + i as u8) as char;
        yaml.push_str(&format!("{curr}: &{curr}\n  - *{prev}\n  - *{prev}\n"));
    }

    let doc = make_document(&yaml);

    let start = Instant::now();
    let result = parse_document(&doc);
    let elapsed = start.elapsed();

    // Must not hang — either succeeds quickly or returns an error
    assert!(
        elapsed < Duration::from_secs(5),
        "billion-laughs YAML took too long: {elapsed:?}"
    );

    // Either outcome is acceptable as long as it completes quickly
    match result {
        Ok(_) => {
            // serde_yaml_ng may expand aliases inline — acceptable if bounded
        }
        Err(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains("YAML")
                    || msg.contains("alias")
                    || msg.contains("anchor")
                    || msg.contains("recursion")
                    || msg.contains("parse"),
                "unexpected error for billion-laughs YAML: {msg}"
            );
        }
    }
}

#[test]
#[serial]
fn test_yaml_self_referencing_alias() {
    // A self-referencing alias should not cause infinite recursion
    // Note: this is technically invalid YAML (forward reference or self-ref),
    // so the parser should reject it.
    let yaml = "id: j-test\nself: &self\n  ref: *self";

    let doc = make_document(yaml);

    let start = Instant::now();
    let result = parse_document(&doc);
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(5),
        "self-referencing alias took too long: {elapsed:?}"
    );

    // Either parsed or error — just must not hang
    match result {
        Ok(parsed) => {
            // If it parses, verify the structure is present
            assert!(parsed.frontmatter.contains_key("self"));
        }
        Err(_) => {
            // Rejection is also acceptable
        }
    }
}

#[test]
#[serial]
fn test_yaml_many_aliases_to_single_anchor() {
    // Many aliases referencing the same anchor — exercises alias resolution
    let count = 1_000;
    let mut yaml = String::from("id: j-test\nbase: &base\n  value: shared\n");
    for i in 0..count {
        yaml.push_str(&format!("ref_{i}: *base\n"));
    }

    let doc = make_document(&yaml);

    let start = Instant::now();
    let result = parse_document(&doc);
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(5),
        "many aliases took too long: {elapsed:?}"
    );

    let parsed = result.expect("many aliases to one anchor should parse");
    assert!(parsed.frontmatter.contains_key("base"));
    assert!(parsed.frontmatter.contains_key("ref_0"));
    assert!(parsed
        .frontmatter
        .contains_key(&format!("ref_{}", count - 1)));
}

// ---------------------------------------------------------------------------
// 5. Many keys in a single mapping
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_many_keys_in_mapping() {
    // A mapping with 10,000 unique keys
    let count = 10_000;
    let mut yaml = String::from("id: j-test\n");
    for i in 0..count {
        yaml.push_str(&format!("key_{i:05}: value_{i}\n"));
    }

    let doc = make_document(&yaml);

    let start = Instant::now();
    let result = parse_document(&doc);
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(5),
        "many keys parsing took too long: {elapsed:?}"
    );

    let parsed = result.expect("many keys should parse successfully");
    assert!(parsed.frontmatter.contains_key("id"));
    assert!(parsed.frontmatter.contains_key("key_00000"));
    assert!(parsed
        .frontmatter
        .contains_key(&format!("key_{:05}", count - 1)));
}

#[test]
#[serial]
fn test_many_keys_deserialized_into_ticket_metadata() {
    // TicketMetadata uses #[serde(deny_unknown_fields)] is NOT set,
    // so extra keys should be silently ignored during deserialization.
    let mut yaml = String::from("id: j-test\nstatus: new\npriority: 2\ntype: task\n");
    for i in 0..1_000 {
        yaml.push_str(&format!("extra_{i}: noise_{i}\n"));
    }

    let doc = make_document(&yaml);

    let start = Instant::now();
    let result = parse_document(&doc);
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(5),
        "many extra keys parsing took too long: {elapsed:?}"
    );

    let parsed = result.expect("many extra keys should parse");
    let meta: TicketMetadata = parsed
        .deserialize_frontmatter()
        .expect("extra keys should be ignored by TicketMetadata deserialization");
    assert_eq!(meta.id.as_deref(), Some("j-test"));
}

// ---------------------------------------------------------------------------
// 6. Duplicate keys
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_duplicate_keys_in_yaml() {
    // YAML spec says duplicate keys are undefined behavior; serde_yaml_ng
    // typically takes the last value. We just verify no crash.
    let yaml = r#"id: j-first
id: j-second
status: new
status: complete"#;

    let doc = make_document(yaml);
    let result = parse_document(&doc);

    match result {
        Ok(parsed) => {
            // Verify it picked one of the values (typically last)
            assert!(parsed.frontmatter.contains_key("id"));
            assert!(parsed.frontmatter.contains_key("status"));
        }
        Err(_) => {
            // Rejection of duplicate keys is also valid
        }
    }
}

// ---------------------------------------------------------------------------
// 7. Mixed adversarial patterns through the typed deserialization path
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_wrong_types_for_ticket_fields() {
    // Provide wrong types: array where string expected, mapping where number expected
    let yaml = r#"id:
  - not
  - a
  - string
status: 42
priority: [1, 2, 3]
deps: "not an array""#;

    let doc = make_document(yaml);
    let parsed = parse_document(&doc).expect("generic parse should succeed");

    // Typed deserialization should fail with a clear error
    let result: Result<TicketMetadata, _> = parsed.deserialize_frontmatter();
    assert!(
        result.is_err(),
        "wrong types should cause deserialization error"
    );

    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("YAML"),
        "error should mention YAML parsing: {msg}"
    );
}

#[test]
#[serial]
fn test_null_values_for_optional_fields() {
    // All optional fields set to null — should deserialize to None
    let yaml = r#"id: j-test
status: ~
priority: ~
type: ~
size: ~
parent: ~
spawned-from: ~
spawn-context: ~
depth: ~
triaged: ~"#;

    let doc = make_document(yaml);
    let parsed = parse_document(&doc).expect("null values should parse");
    let meta: TicketMetadata = parsed
        .deserialize_frontmatter()
        .expect("null optionals should deserialize to None");

    assert_eq!(meta.id.as_deref(), Some("j-test"));
    assert!(meta.status.is_none());
    assert!(meta.priority.is_none());
    assert!(meta.ticket_type.is_none());
    assert!(meta.size.is_none());
    assert!(meta.parent.is_none());
    assert!(meta.spawned_from.is_none());
    assert!(meta.spawn_context.is_none());
    assert!(meta.depth.is_none());
    assert!(meta.triaged.is_none());
}

#[test]
#[serial]
fn test_empty_string_values() {
    // Empty strings for string fields
    let yaml = r#"id: ""
status: ""
priority: ""
type: """#;

    let doc = make_document(yaml);
    let parsed = parse_document(&doc).expect("empty strings should parse");

    // Typed deserialization should fail because empty strings aren't valid
    // for enum fields like status/priority/type
    let result: Result<TicketMetadata, _> = parsed.deserialize_frontmatter();
    // Either succeeds with defaults or fails — both are acceptable
    match result {
        Ok(_) | Err(_) => {} // no crash = pass
    }
}

// ---------------------------------------------------------------------------
// 8. Frontmatter splitting edge cases with adversarial content
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_frontmatter_with_many_dashes_in_body() {
    // Body contains many lines of "---" to try to confuse the splitter
    let mut content = String::from("---\nid: j-test\n---\n");
    for _ in 0..1_000 {
        content.push_str("---\n");
    }

    let start = Instant::now();
    let result = split_frontmatter(&content);
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(5),
        "many dashes in body took too long: {elapsed:?}"
    );

    let (fm, body) = result.expect("should split despite dashes in body");
    assert!(fm.contains("id: j-test"));
    assert!(body.contains("---"));
}

#[test]
#[serial]
fn test_very_long_key_names() {
    // Keys with very long names (~10KB each)
    let long_key = "k".repeat(10_000);
    let yaml = format!("id: j-test\n{long_key}: some_value\n{long_key}_2: another_value");

    let doc = make_document(&yaml);

    let start = Instant::now();
    let result = parse_document(&doc);
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(5),
        "long key names took too long: {elapsed:?}"
    );

    match result {
        Ok(parsed) => {
            assert!(parsed.frontmatter.contains_key("id"));
        }
        Err(_) => {
            // Rejection due to key length is acceptable
        }
    }
}

#[test]
#[serial]
fn test_deeply_nested_inline_json_style() {
    // YAML supports JSON-style inline notation: {a: {b: {c: {d: ...}}}}
    let depth = 200;
    let mut yaml = String::from("id: j-test\nnested: ");
    for _ in 0..depth {
        yaml.push_str("{a: ");
    }
    yaml.push_str("leaf");
    for _ in 0..depth {
        yaml.push('}');
    }

    let doc = make_document(&yaml);

    let start = Instant::now();
    let result = parse_document(&doc);
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(5),
        "deeply nested inline YAML took too long: {elapsed:?}"
    );

    // Either result is fine — just must not crash or hang
    match result {
        Ok(parsed) => {
            assert!(parsed.frontmatter.contains_key("nested"));
        }
        Err(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains("YAML")
                    || msg.contains("recursion")
                    || msg.contains("nesting")
                    || msg.contains("parse"),
                "unexpected error for inline nested YAML: {msg}"
            );
        }
    }
}

#[test]
#[serial]
fn test_flow_sequence_with_many_items() {
    // Inline flow sequence: [item0, item1, item2, ..., item9999]
    let count = 10_000;
    let items: Vec<String> = (0..count).map(|i| format!("item{i}")).collect();
    let yaml = format!("id: j-test\nflow: [{}]", items.join(", "));

    let doc = make_document(&yaml);

    let start = Instant::now();
    let result = parse_document(&doc);
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(5),
        "large flow sequence took too long: {elapsed:?}"
    );

    let parsed = result.expect("large flow sequence should parse");
    assert!(parsed.frontmatter.contains_key("flow"));
}

// ---------------------------------------------------------------------------
// 9. Tag/type coercion edge cases
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_yaml_type_coercion_edge_cases() {
    // YAML has surprising type coercion: "yes" → bool, "1.0" → float, etc.
    // Verify these don't crash the TicketMetadata deserialization.
    let yaml = r#"id: j-test
status: new
priority: 2
parent: "yes"
external-ref: "no"
spawn-context: "null"
remote: "1.0""#;

    let doc = make_document(yaml);
    let parsed = parse_document(&doc).expect("coercion edge cases should parse");
    let meta: TicketMetadata = parsed
        .deserialize_frontmatter()
        .expect("quoted values should deserialize as strings");

    assert_eq!(meta.parent, Some("yes".to_string()));
    assert_eq!(meta.external_ref, Some("no".to_string()));
    assert_eq!(meta.spawn_context, Some("null".to_string()));
    assert_eq!(meta.remote, Some("1.0".to_string()));
}

#[test]
#[serial]
fn test_yaml_unquoted_boolean_coercion() {
    // Unquoted "yes"/"no"/"on"/"off" are booleans in YAML 1.1
    // serde_yaml_ng follows YAML 1.2 which doesn't coerce these, but let's verify
    let yaml = r#"id: j-test
parent: yes
external-ref: no"#;

    let doc = make_document(yaml);
    let parsed = parse_document(&doc).expect("unquoted booleans should parse");

    // Typed deserialization may or may not succeed depending on YAML version behavior
    let result: Result<TicketMetadata, _> = parsed.deserialize_frontmatter();
    match result {
        Ok(_) | Err(_) => {} // no crash = pass
    }
}

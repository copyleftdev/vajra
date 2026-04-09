//! CST-to-JSON conversion.
//!
//! Walks a tree-sitter concrete syntax tree and produces a `serde_json::Value`
//! tree matching the vajra-source schema.

use serde_json::{json, Map, Value};
use tree_sitter::Node;

use crate::config::SourceConfig;
use crate::semantic::semantic_label;

/// Convert a tree-sitter node into a JSON value.
///
/// This is a recursive walk that produces the vajra-source JSON schema:
/// - `kind`: node type string
/// - `named`: whether it's a named grammar node
/// - `semantic`: optional human-readable category (when `semantic_paths` is on)
/// - `field`: optional field name from parent
/// - `text`: source text for leaf nodes
/// - `error`: true for ERROR/MISSING nodes
/// - `span`: optional line/column range
/// - `children`: array of child nodes
pub fn node_to_json(
    node: Node<'_>,
    source: &[u8],
    config: &SourceConfig,
    field_name: Option<&str>,
    language_name: &str,
) -> Value {
    let mut obj = Map::new();

    obj.insert("kind".to_owned(), json!(node.kind()));
    obj.insert("named".to_owned(), json!(node.is_named()));

    if config.semantic_paths {
        if let Some(label) = semantic_label(language_name, node.kind()) {
            obj.insert(
                "semantic".to_owned(),
                serde_json::Value::String(label.to_owned()),
            );
        }
    }

    if let Some(field) = field_name {
        obj.insert("field".to_owned(), json!(field));
    }

    if node.is_error() || node.is_missing() {
        obj.insert("error".to_owned(), json!(true));
    }

    if config.include_spans {
        let start = node.start_position();
        let end = node.end_position();
        obj.insert(
            "span".to_owned(),
            json!({
                "start": [start.row, start.column],
                "end": [end.row, end.column],
            }),
        );
    }

    // Collect named children (or all children if include_anonymous is set)
    let child_count = node.child_count();
    let mut children = Vec::new();

    if child_count > 0 {
        let mut cursor = node.walk();
        for i in 0..child_count {
            let idx = i as u32;
            if let Some(child) = node.child(idx) {
                if !config.include_anonymous && !child.is_named() {
                    continue;
                }
                // Get the field name this child occupies, if any
                let child_field = node.field_name_for_child(idx);
                // Use cursor to get the field name (more reliable for named fields)
                let field = child_field.or_else(|| {
                    cursor.reset(child);
                    cursor.field_name()
                });
                children.push(node_to_json(child, source, config, field, language_name));
            }
        }
    }

    if children.is_empty() {
        // Leaf node — include text if configured
        if config.include_text {
            let text = node.utf8_text(source).unwrap_or("<invalid-utf8>");
            obj.insert("text".to_owned(), json!(text));
        }
    } else {
        obj.insert("children".to_owned(), Value::Array(children));
    }

    Value::Object(obj)
}

/// Convert a full tree-sitter tree into a JSON value with metadata.
///
/// Wraps the root node's conversion with source metadata.
pub fn tree_to_json(
    root: Node<'_>,
    source: &[u8],
    config: &SourceConfig,
    language_name: &str,
) -> Value {
    let mut root_json = node_to_json(root, source, config, None, language_name);

    // Add source metadata at the root
    if let Value::Object(ref mut obj) = root_json {
        obj.insert(
            "_source".to_owned(),
            json!({
                "language": language_name,
                "file_size": source.len(),
            }),
        );
    }

    root_json
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{SourceConfig, SourceLanguage};
    use crate::grammar::get_grammar;

    /// Helper: parse source with tree-sitter and return the JSON tree.
    fn parse_to_json(source: &[u8], lang: SourceLanguage, config: &SourceConfig) -> Value {
        let ts_language = match get_grammar(lang) {
            Ok(l) => l,
            Err(_) => return Value::Null,
        };
        let mut parser = tree_sitter::Parser::new();
        if parser.set_language(&ts_language).is_err() {
            return Value::Null;
        }
        let tree = match parser.parse(source, None) {
            Some(t) => t,
            None => return Value::Null,
        };
        tree_to_json(tree.root_node(), source, config, &lang.to_string())
    }

    /// Helper: recursively find all "semantic" values in a JSON tree.
    fn collect_semantic_labels(val: &Value) -> Vec<String> {
        let mut labels = Vec::new();
        if let Value::Object(obj) = val {
            if let Some(Value::String(s)) = obj.get("semantic") {
                labels.push(s.clone());
            }
            if let Some(Value::Array(children)) = obj.get("children") {
                for child in children {
                    labels.extend(collect_semantic_labels(child));
                }
            }
        }
        labels
    }

    // -----------------------------------------------------------------
    // Backward compatibility: semantic field absent when flag is off
    // -----------------------------------------------------------------

    #[test]
    #[cfg(feature = "rust")]
    fn no_semantic_field_without_flag() {
        let config = SourceConfig {
            language: Some(SourceLanguage::Rust),
            semantic_paths: false,
            ..SourceConfig::default()
        };
        let json = parse_to_json(b"fn main() {}", SourceLanguage::Rust, &config);
        let labels = collect_semantic_labels(&json);
        assert!(
            labels.is_empty(),
            "expected no semantic labels when flag is off, got: {labels:?}"
        );
    }

    // -----------------------------------------------------------------
    // Rust: semantic field present when flag is on
    // -----------------------------------------------------------------

    #[test]
    #[cfg(feature = "rust")]
    fn rust_function_gets_semantic_label() {
        let config = SourceConfig {
            language: Some(SourceLanguage::Rust),
            semantic_paths: true,
            ..SourceConfig::default()
        };
        let json = parse_to_json(b"fn hello() {}", SourceLanguage::Rust, &config);
        let labels = collect_semantic_labels(&json);
        assert!(
            labels.contains(&"function".to_owned()),
            "expected 'function' label, got: {labels:?}"
        );
    }

    #[test]
    #[cfg(feature = "rust")]
    fn rust_struct_gets_semantic_label() {
        let config = SourceConfig {
            language: Some(SourceLanguage::Rust),
            semantic_paths: true,
            ..SourceConfig::default()
        };
        let json = parse_to_json(b"struct Point { x: i32 }", SourceLanguage::Rust, &config);
        let labels = collect_semantic_labels(&json);
        assert!(
            labels.contains(&"struct".to_owned()),
            "expected 'struct' label, got: {labels:?}"
        );
    }

    #[test]
    #[cfg(feature = "rust")]
    fn rust_use_gets_import_label() {
        let config = SourceConfig {
            language: Some(SourceLanguage::Rust),
            semantic_paths: true,
            ..SourceConfig::default()
        };
        let json = parse_to_json(b"use std::io;", SourceLanguage::Rust, &config);
        let labels = collect_semantic_labels(&json);
        assert!(
            labels.contains(&"import".to_owned()),
            "expected 'import' label, got: {labels:?}"
        );
    }

    #[test]
    #[cfg(feature = "rust")]
    fn rust_enum_gets_semantic_label() {
        let config = SourceConfig {
            language: Some(SourceLanguage::Rust),
            semantic_paths: true,
            ..SourceConfig::default()
        };
        let json = parse_to_json(b"enum Color { Red, Green }", SourceLanguage::Rust, &config);
        let labels = collect_semantic_labels(&json);
        assert!(
            labels.contains(&"enum".to_owned()),
            "expected 'enum' label, got: {labels:?}"
        );
    }

    #[test]
    #[cfg(feature = "rust")]
    fn rust_impl_gets_semantic_label() {
        let config = SourceConfig {
            language: Some(SourceLanguage::Rust),
            semantic_paths: true,
            ..SourceConfig::default()
        };
        let json = parse_to_json(
            b"struct S; impl S { fn new() -> Self { S } }",
            SourceLanguage::Rust,
            &config,
        );
        let labels = collect_semantic_labels(&json);
        assert!(
            labels.contains(&"impl".to_owned()),
            "expected 'impl' label, got: {labels:?}"
        );
        assert!(
            labels.contains(&"function".to_owned()),
            "expected 'function' label in impl, got: {labels:?}"
        );
    }

    // -----------------------------------------------------------------
    // Python: semantic labels
    // -----------------------------------------------------------------

    #[test]
    #[cfg(feature = "python")]
    fn python_function_gets_semantic_label() {
        let config = SourceConfig {
            language: Some(SourceLanguage::Python),
            semantic_paths: true,
            ..SourceConfig::default()
        };
        let json = parse_to_json(b"def hello():\n    pass\n", SourceLanguage::Python, &config);
        let labels = collect_semantic_labels(&json);
        assert!(
            labels.contains(&"function".to_owned()),
            "expected 'function' label, got: {labels:?}"
        );
    }

    #[test]
    #[cfg(feature = "python")]
    fn python_class_gets_semantic_label() {
        let config = SourceConfig {
            language: Some(SourceLanguage::Python),
            semantic_paths: true,
            ..SourceConfig::default()
        };
        let json = parse_to_json(b"class Foo:\n    pass\n", SourceLanguage::Python, &config);
        let labels = collect_semantic_labels(&json);
        assert!(
            labels.contains(&"class".to_owned()),
            "expected 'class' label, got: {labels:?}"
        );
    }

    #[test]
    #[cfg(feature = "python")]
    fn python_import_gets_semantic_label() {
        let config = SourceConfig {
            language: Some(SourceLanguage::Python),
            semantic_paths: true,
            ..SourceConfig::default()
        };
        let json = parse_to_json(b"import os\n", SourceLanguage::Python, &config);
        let labels = collect_semantic_labels(&json);
        assert!(
            labels.contains(&"import".to_owned()),
            "expected 'import' label, got: {labels:?}"
        );
    }

    #[test]
    #[cfg(feature = "python")]
    fn python_import_from_gets_semantic_label() {
        let config = SourceConfig {
            language: Some(SourceLanguage::Python),
            semantic_paths: true,
            ..SourceConfig::default()
        };
        let json = parse_to_json(b"from os import path\n", SourceLanguage::Python, &config);
        let labels = collect_semantic_labels(&json);
        assert!(
            labels.contains(&"import".to_owned()),
            "expected 'import' label for from-import, got: {labels:?}"
        );
    }

    // -----------------------------------------------------------------
    // Go: semantic labels
    // -----------------------------------------------------------------

    #[test]
    #[cfg(feature = "go")]
    fn go_function_gets_semantic_label() {
        let config = SourceConfig {
            language: Some(SourceLanguage::Go),
            semantic_paths: true,
            ..SourceConfig::default()
        };
        let json = parse_to_json(
            b"package main\nfunc hello() {}\n",
            SourceLanguage::Go,
            &config,
        );
        let labels = collect_semantic_labels(&json);
        assert!(
            labels.contains(&"function".to_owned()),
            "expected 'function' label, got: {labels:?}"
        );
        assert!(
            labels.contains(&"package".to_owned()),
            "expected 'package' label, got: {labels:?}"
        );
    }

    #[test]
    #[cfg(feature = "go")]
    fn go_type_gets_semantic_label() {
        let config = SourceConfig {
            language: Some(SourceLanguage::Go),
            semantic_paths: true,
            ..SourceConfig::default()
        };
        let json = parse_to_json(
            b"package main\ntype Point struct { X int }\n",
            SourceLanguage::Go,
            &config,
        );
        let labels = collect_semantic_labels(&json);
        assert!(
            labels.contains(&"type".to_owned()),
            "expected 'type' label, got: {labels:?}"
        );
    }

    // -----------------------------------------------------------------
    // JavaScript: semantic labels
    // -----------------------------------------------------------------

    #[test]
    #[cfg(feature = "javascript")]
    fn js_function_gets_semantic_label() {
        let config = SourceConfig {
            language: Some(SourceLanguage::JavaScript),
            semantic_paths: true,
            ..SourceConfig::default()
        };
        let json = parse_to_json(
            b"function hello() {}\n",
            SourceLanguage::JavaScript,
            &config,
        );
        let labels = collect_semantic_labels(&json);
        assert!(
            labels.contains(&"function".to_owned()),
            "expected 'function' label, got: {labels:?}"
        );
    }

    #[test]
    #[cfg(feature = "javascript")]
    fn js_class_gets_semantic_label() {
        let config = SourceConfig {
            language: Some(SourceLanguage::JavaScript),
            semantic_paths: true,
            ..SourceConfig::default()
        };
        let json = parse_to_json(b"class Foo {}\n", SourceLanguage::JavaScript, &config);
        let labels = collect_semantic_labels(&json);
        assert!(
            labels.contains(&"class".to_owned()),
            "expected 'class' label, got: {labels:?}"
        );
    }

    // -----------------------------------------------------------------
    // Determinism with semantic paths
    // -----------------------------------------------------------------

    #[test]
    #[cfg(feature = "rust")]
    fn semantic_output_is_deterministic() {
        let source = b"use std::io;\nfn main() { let x = 1; }\nstruct S;";
        let config = SourceConfig {
            language: Some(SourceLanguage::Rust),
            semantic_paths: true,
            ..SourceConfig::default()
        };
        let baseline =
            serde_json::to_string(&parse_to_json(source, SourceLanguage::Rust, &config))
                .unwrap_or_default();
        for _ in 0..10 {
            let run =
                serde_json::to_string(&parse_to_json(source, SourceLanguage::Rust, &config))
                    .unwrap_or_default();
            assert_eq!(baseline, run, "semantic output must be deterministic");
        }
    }

    // -----------------------------------------------------------------
    // Multiple constructs in one file
    // -----------------------------------------------------------------

    #[test]
    #[cfg(feature = "rust")]
    fn rust_multiple_constructs() {
        let source = b"use std::io;\nstruct S;\nenum E { A }\nfn f() {}\n";
        let config = SourceConfig {
            language: Some(SourceLanguage::Rust),
            semantic_paths: true,
            ..SourceConfig::default()
        };
        let json = parse_to_json(source, SourceLanguage::Rust, &config);
        let labels = collect_semantic_labels(&json);
        assert!(labels.contains(&"import".to_owned()));
        assert!(labels.contains(&"struct".to_owned()));
        assert!(labels.contains(&"enum".to_owned()));
        assert!(labels.contains(&"function".to_owned()));
    }
}

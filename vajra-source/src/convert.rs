//! CST-to-JSON conversion.
//!
//! Walks a tree-sitter concrete syntax tree and produces a `serde_json::Value`
//! tree matching the vajra-source schema.

use serde_json::{json, Map, Value};
use tree_sitter::Node;

use crate::config::SourceConfig;

/// Convert a tree-sitter node into a JSON value.
///
/// This is a recursive walk that produces the vajra-source JSON schema:
/// - `kind`: node type string
/// - `named`: whether it's a named grammar node
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
) -> Value {
    let mut obj = Map::new();

    obj.insert("kind".to_owned(), json!(node.kind()));
    obj.insert("named".to_owned(), json!(node.is_named()));

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
                children.push(node_to_json(child, source, config, field));
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
    let mut root_json = node_to_json(root, source, config, None);

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

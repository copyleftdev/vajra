//! Input format detection and conversion.
//!
//! Supports JSON, NDJSON, YAML, CSV, TSV, Markdown, and PDF. All formats are
//! normalized to `serde_json::Value` and then wrapped in a [`Document`] via
//! the existing parse pipeline.

use std::path::Path;

use vajra_types::{Document, VajraError};

use crate::parse::parse_str;

/// Supported input formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputFormat {
    Json,
    Ndjson,
    Yaml,
    Csv,
    Tsv,
    Markdown,
    Pdf,
    Git,
}

/// Auto-detect format from a file extension.
///
/// Falls back to [`InputFormat::Json`] for unknown extensions.
#[must_use]
pub fn detect_format(path: &Path) -> InputFormat {
    match path.extension().and_then(|e| e.to_str()) {
        Some("ndjson" | "jsonl") => InputFormat::Ndjson,
        Some("yaml" | "yml") => InputFormat::Yaml,
        Some("csv") => InputFormat::Csv,
        Some("tsv") => InputFormat::Tsv,
        Some("md" | "markdown") => InputFormat::Markdown,
        Some("pdf") => InputFormat::Pdf,
        // Treat .json and any unknown extension as JSON (the default).
        _ => InputFormat::Json,
    }
}

/// Heuristic content sniffing when no extension is available.
///
/// Looks at the first non-whitespace characters to guess the format.
#[must_use]
pub fn sniff_format(input: &str) -> InputFormat {
    let trimmed = input.trim_start();

    // JSON starts with `{` or `[`
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        // Could be NDJSON -- check if there are multiple lines that each start
        // with `{`.
        let mut brace_lines = 0u32;
        for line in trimmed.lines().take(5) {
            let lt = line.trim();
            if lt.is_empty() {
                continue;
            }
            if lt.starts_with('{') {
                brace_lines += 1;
            }
        }
        if brace_lines > 1 {
            return InputFormat::Ndjson;
        }
        return InputFormat::Json;
    }

    // Markdown indicators: starts with `#` heading or `---` frontmatter
    // followed by a blank line or more YAML-like content. We check for
    // `# ` (heading) specifically to avoid ambiguity with YAML comments.
    if trimmed.starts_with("# ") || trimmed.starts_with("## ") || trimmed.starts_with("### ") {
        return InputFormat::Markdown;
    }

    // YAML indicators
    if trimmed.starts_with("---") || trimmed.starts_with("- ") {
        return InputFormat::Yaml;
    }
    // Check for key: value pattern (YAML)
    for line in trimmed.lines().take(3) {
        let lt = line.trim();
        if !lt.is_empty() && !lt.starts_with('#') {
            if lt.contains(": ") || lt.ends_with(':') {
                return InputFormat::Yaml;
            }
            break;
        }
    }

    InputFormat::Json // ultimate fallback
}

/// Parse input in the detected or explicitly specified format.
///
/// When `format` is `None`, the content is sniffed to guess the format.
///
/// # Errors
///
/// Returns [`VajraError::Parse`] on malformed input.
pub fn parse_auto(input: &str, format: Option<InputFormat>) -> Result<Vec<Document>, VajraError> {
    let fmt = format.unwrap_or_else(|| sniff_format(input));
    match fmt {
        InputFormat::Json => Ok(vec![parse_str(input)?]),
        InputFormat::Ndjson => parse_ndjson(input),
        InputFormat::Yaml => parse_yaml(input),
        InputFormat::Csv => Ok(vec![parse_csv(input, b',')?]),
        InputFormat::Tsv => Ok(vec![parse_csv(input, b'\t')?]),
        InputFormat::Markdown => Ok(vec![crate::markdown::parse_markdown(input)?]),
        InputFormat::Pdf => Err(VajraError::Parse {
            byte_offset: 0,
            message: "PDF input cannot be parsed from a text string; \
                      use parse_pdf_file or parse_pdf_bytes instead"
                .to_string(),
            source_path: None,
        }),
        InputFormat::Git => Err(VajraError::Parse {
            byte_offset: 0,
            message: "Git input cannot be parsed from a text string; \
                      use git::load_git_log instead"
                .to_string(),
            source_path: None,
        }),
    }
}

/// Read a file and parse it with auto-detection.
///
/// The format is detected from the file extension first. If `format` is
/// provided it overrides the extension-based detection.
///
/// # Errors
///
/// Returns [`VajraError::Io`] if the file cannot be read.
/// Returns [`VajraError::Parse`] on malformed content.
pub fn parse_file_auto(
    path: &Path,
    format: Option<InputFormat>,
) -> Result<Vec<Document>, VajraError> {
    let fmt = format.unwrap_or_else(|| detect_format(path));

    // PDF files are binary; handle them separately.
    if fmt == InputFormat::Pdf {
        return Ok(vec![crate::pdf::parse_pdf_file(path)?]);
    }

    // Git format requires the git CLI adapter.
    if fmt == InputFormat::Git {
        let config = crate::git::GitLogConfig::default();
        return Ok(vec![crate::git::load_git_log(path, &config)?]);
    }

    let content = std::fs::read_to_string(path).map_err(|e| VajraError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    parse_auto(&content, Some(fmt))
}

// ---------------------------------------------------------------------------
// NDJSON
// ---------------------------------------------------------------------------

/// Parse newline-delimited JSON.
///
/// Each non-empty line is parsed as an independent JSON document. Empty and
/// whitespace-only lines are silently skipped.
///
/// # Errors
///
/// Returns [`VajraError::Parse`] if any non-empty line is invalid JSON. The
/// error message includes the 1-based line number.
pub fn parse_ndjson(input: &str) -> Result<Vec<Document>, VajraError> {
    let mut docs = Vec::new();
    for (idx, line) in input.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let doc = parse_str(trimmed).map_err(|e| VajraError::Parse {
            byte_offset: idx + 1, // 1-based line number
            message: format!("NDJSON line {}: {e}", idx + 1),
            source_path: None,
        })?;
        docs.push(doc);
    }
    Ok(docs)
}

// ---------------------------------------------------------------------------
// YAML
// ---------------------------------------------------------------------------

/// Parse YAML input.
///
/// Supports both single-document and multi-document YAML (separated by `---`).
/// Each YAML document is converted to `serde_json::Value` and then processed
/// through the standard path-extraction pipeline.
///
/// # Errors
///
/// Returns [`VajraError::Parse`] on invalid YAML.
pub fn parse_yaml(input: &str) -> Result<Vec<Document>, VajraError> {
    let mut docs = Vec::new();

    for (idx, yaml_doc) in serde_yaml::Deserializer::from_str(input).enumerate() {
        // Deserialize to serde_yaml::Value first so that merge keys (<<)
        // and anchors/aliases are fully resolved by the YAML library.
        let yaml_value: serde_yaml::Value =
            serde::Deserialize::deserialize(yaml_doc).map_err(|e| VajraError::Parse {
                byte_offset: 0,
                message: format!("YAML document {}: {e}", idx + 1),
                source_path: None,
            })?;

        // Then round-trip through serde to get serde_json::Value.
        let mut value: serde_json::Value =
            serde_yaml::from_value(yaml_value).map_err(|e| VajraError::Parse {
                byte_offset: 0,
                message: format!("YAML->JSON conversion for document {}: {e}", idx + 1),
                source_path: None,
            })?;

        // Resolve YAML merge keys (<<) that serde_yaml keeps as literal keys.
        resolve_merge_keys(&mut value);

        let json_text = serde_json::to_string(&value).map_err(|e| VajraError::Parse {
            byte_offset: 0,
            message: format!("YAML->JSON serialization for document {}: {e}", idx + 1),
            source_path: None,
        })?;

        let doc = parse_str(&json_text)?;
        docs.push(doc);
    }

    // Filter out documents that deserialized to null from empty / whitespace-only input.
    let non_null: Vec<Document> = docs.into_iter().filter(|d| !d.value().is_null()).collect();

    if non_null.is_empty() {
        return Err(VajraError::Parse {
            byte_offset: 0,
            message: "empty YAML input".to_string(),
            source_path: None,
        });
    }

    Ok(non_null)
}

// ---------------------------------------------------------------------------
// CSV / TSV
// ---------------------------------------------------------------------------

/// Parse CSV (or TSV) input.
///
/// The first row is treated as column headers. Each subsequent row becomes a
/// JSON object whose keys are the header names. Numeric values are
/// automatically inferred: values parseable as `i64` become integers, values
/// parseable as `f64` become floats, empty cells become `null`, and everything
/// else stays a string.
///
/// Returns a single [`Document`] wrapping a JSON array of row objects.
///
/// # Errors
///
/// Returns [`VajraError::Parse`] on CSV parse errors or if the input is empty
/// (no headers).
pub fn parse_csv(input: &str, delimiter: u8) -> Result<Document, VajraError> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(true)
        .flexible(true) // tolerate inconsistent column counts
        .from_reader(input.as_bytes());

    let headers: Vec<String> = reader
        .headers()
        .map_err(|e| VajraError::Parse {
            byte_offset: 0,
            message: format!("CSV header error: {e}"),
            source_path: None,
        })?
        .iter()
        .map(String::from)
        .collect();

    if headers.is_empty() {
        return Err(VajraError::Parse {
            byte_offset: 0,
            message: "CSV has no headers".to_string(),
            source_path: None,
        });
    }

    let mut rows: Vec<serde_json::Value> = Vec::new();

    for (row_idx, result) in reader.records().enumerate() {
        let record = result.map_err(|e| VajraError::Parse {
            byte_offset: row_idx + 2, // 1-based, +1 for header row
            message: format!("CSV row {}: {e}", row_idx + 2),
            source_path: None,
        })?;

        let mut obj = serde_json::Map::with_capacity(headers.len());
        for (col_idx, header) in headers.iter().enumerate() {
            let cell = record.get(col_idx);
            let value = match cell {
                None | Some("") => serde_json::Value::Null,
                Some(s) => infer_scalar(s),
            };
            obj.insert(header.clone(), value);
        }
        rows.push(serde_json::Value::Object(obj));
    }

    let array_value = serde_json::Value::Array(rows);
    let json_text = serde_json::to_string(&array_value).map_err(|e| VajraError::Parse {
        byte_offset: 0,
        message: format!("CSV->JSON conversion: {e}"),
        source_path: None,
    })?;

    parse_str(&json_text)
}

/// Recursively resolve YAML merge keys (`<<`) in a `serde_json::Value`.
///
/// When `serde_yaml` deserializes a mapping that used `<<: *alias`, the result
/// contains a literal `"<<"` key whose value is the merged mapping. This
/// function walks the tree and flattens those keys into their parent objects,
/// matching the standard YAML merge key semantics (explicit keys take
/// precedence over merged ones).
fn resolve_merge_keys(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            // First, recurse into all child values (including the merge source).
            for child in map.values_mut() {
                resolve_merge_keys(child);
            }

            // Then handle the merge key if present.
            if let Some(merge_val) = map.remove("<<") {
                match merge_val {
                    serde_json::Value::Object(merged) => {
                        for (k, v) in merged {
                            // Existing keys take precedence.
                            map.entry(k).or_insert(v);
                        }
                    }
                    serde_json::Value::Array(arr) => {
                        // Multiple merge sources: apply in order.
                        for item in arr {
                            if let serde_json::Value::Object(merged) = item {
                                for (k, v) in merged {
                                    map.entry(k).or_insert(v);
                                }
                            }
                        }
                    }
                    _ => {
                        // Not a mapping or array -- put it back as-is.
                        map.insert("<<".to_string(), merge_val);
                    }
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                resolve_merge_keys(item);
            }
        }
        _ => {}
    }
}

/// Attempt numeric inference on a string value.
///
/// Order: `i64` -> `f64` -> string.
fn infer_scalar(s: &str) -> serde_json::Value {
    // Try integer first
    if let Ok(n) = s.parse::<i64>() {
        return serde_json::Value::Number(n.into());
    }
    // Try float
    if let Ok(f) = s.parse::<f64>() {
        if let Some(num) = serde_json::Number::from_f64(f) {
            return serde_json::Value::Number(num);
        }
    }
    // Fall back to string
    serde_json::Value::String(s.to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // ---- NDJSON tests ----

    #[test]
    fn ndjson_three_lines() -> Result<(), Box<dyn std::error::Error>> {
        let input = r#"{"a":1}
{"b":2}
{"c":3}"#;
        let docs = parse_ndjson(input)?;
        assert_eq!(docs.len(), 3);
        Ok(())
    }

    #[test]
    fn ndjson_empty_lines_skipped() -> Result<(), Box<dyn std::error::Error>> {
        let input = r#"{"a":1}

{"b":2}

"#;
        let docs = parse_ndjson(input)?;
        assert_eq!(docs.len(), 2);
        Ok(())
    }

    #[test]
    fn ndjson_single_line() -> Result<(), Box<dyn std::error::Error>> {
        let input = r#"{"x":"hello"}"#;
        let docs = parse_ndjson(input)?;
        assert_eq!(docs.len(), 1);
        Ok(())
    }

    #[test]
    fn ndjson_trailing_newline() -> Result<(), Box<dyn std::error::Error>> {
        let input = "{\"a\":1}\n{\"b\":2}\n";
        let docs = parse_ndjson(input)?;
        assert_eq!(docs.len(), 2);
        Ok(())
    }

    #[test]
    fn ndjson_error_includes_line_number() -> Result<(), Box<dyn std::error::Error>> {
        let input = "{\"a\":1}\n{bad json}\n{\"c\":3}";
        let err = parse_ndjson(input).err().ok_or("expected error")?;
        let msg = err.to_string();
        assert!(msg.contains("line 2"), "expected 'line 2' in: {msg}");
        Ok(())
    }

    #[test]
    fn ndjson_mixed_schemas() -> Result<(), Box<dyn std::error::Error>> {
        let input = r#"{"name":"Alice","age":30}
{"id":1,"tags":["a","b"]}
[1,2,3]"#;
        let docs = parse_ndjson(input)?;
        assert_eq!(docs.len(), 3);
        Ok(())
    }

    // ---- YAML tests ----

    #[test]
    fn yaml_simple_object() -> Result<(), Box<dyn std::error::Error>> {
        let input = "name: Alice\nage: 30\n";
        let docs = parse_yaml(input)?;
        assert_eq!(docs.len(), 1);
        let val = docs[0].value();
        assert_eq!(val["name"], serde_json::Value::String("Alice".to_string()));
        assert_eq!(val["age"], serde_json::json!(30));
        Ok(())
    }

    #[test]
    fn yaml_nested() -> Result<(), Box<dyn std::error::Error>> {
        let input = "a:\n  b:\n    c: 1\n";
        let docs = parse_yaml(input)?;
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].value()["a"]["b"]["c"], serde_json::json!(1));
        // Trie should have paths: $, $.a, $.a.b, $.a.b.c
        assert_eq!(docs[0].metadata().distinct_paths, 4);
        Ok(())
    }

    #[test]
    fn yaml_array() -> Result<(), Box<dyn std::error::Error>> {
        let input = "- 1\n- 2\n- 3\n";
        let docs = parse_yaml(input)?;
        assert_eq!(docs.len(), 1);
        assert!(docs[0].value().is_array());
        Ok(())
    }

    #[test]
    fn yaml_multi_document() -> Result<(), Box<dyn std::error::Error>> {
        let input = "---\nname: Alice\n---\nname: Bob\n";
        let docs = parse_yaml(input)?;
        assert_eq!(docs.len(), 2);
        assert_eq!(
            docs[0].value()["name"],
            serde_json::Value::String("Alice".to_string())
        );
        assert_eq!(
            docs[1].value()["name"],
            serde_json::Value::String("Bob".to_string())
        );
        Ok(())
    }

    #[test]
    fn yaml_dates_become_strings() -> Result<(), Box<dyn std::error::Error>> {
        // serde_yaml serializes untagged dates as strings when going through
        // serde_json::Value
        let input = "date: 2024-01-15\n";
        let docs = parse_yaml(input)?;
        assert_eq!(docs.len(), 1);
        let date_val = &docs[0].value()["date"];
        assert!(
            date_val.is_string(),
            "expected string for date, got: {date_val:?}"
        );
        Ok(())
    }

    #[test]
    fn yaml_empty_returns_error() {
        let input = "";
        let result = parse_yaml(input);
        assert!(result.is_err(), "expected error for empty YAML");
    }

    #[test]
    fn yaml_anchors_aliases_resolved() -> Result<(), Box<dyn std::error::Error>> {
        let input = "defaults: &defaults\n  adapter: postgres\n  host: localhost\nproduction:\n  <<: *defaults\n  host: prod-server\n";
        let docs = parse_yaml(input)?;
        assert_eq!(docs.len(), 1);
        let prod = &docs[0].value()["production"];
        assert_eq!(
            prod["adapter"],
            serde_json::Value::String("postgres".to_string())
        );
        assert_eq!(
            prod["host"],
            serde_json::Value::String("prod-server".to_string())
        );
        Ok(())
    }

    // ---- CSV tests ----

    #[test]
    fn csv_simple() -> Result<(), Box<dyn std::error::Error>> {
        let input = "name,age,city\nAlice,30,NYC\nBob,25,LA\nCarol,35,SF\n";
        let doc = parse_csv(input, b',')?;
        let arr = doc.value().as_array().ok_or("expected array")?;
        assert_eq!(arr.len(), 3);
        assert_eq!(
            arr[0]["name"],
            serde_json::Value::String("Alice".to_string())
        );
        assert_eq!(arr[0]["age"], serde_json::json!(30));
        Ok(())
    }

    #[test]
    fn csv_numeric_inference() -> Result<(), Box<dyn std::error::Error>> {
        let input = "int,float,string\n42,2.75,hello\n";
        let doc = parse_csv(input, b',')?;
        let arr = doc.value().as_array().ok_or("expected array")?;
        assert_eq!(arr[0]["int"], serde_json::json!(42));
        assert_eq!(arr[0]["float"], serde_json::json!(2.75));
        assert_eq!(
            arr[0]["string"],
            serde_json::Value::String("hello".to_string())
        );
        Ok(())
    }

    #[test]
    fn csv_empty_cells_null() -> Result<(), Box<dyn std::error::Error>> {
        let input = "a,b,c\n1,,3\n";
        let doc = parse_csv(input, b',')?;
        let arr = doc.value().as_array().ok_or("expected array")?;
        assert!(arr[0]["b"].is_null(), "expected null for empty cell");
        Ok(())
    }

    #[test]
    fn csv_quoted_fields() -> Result<(), Box<dyn std::error::Error>> {
        let input = "name,note\n\"Alice\",\"has a comma, here\"\n";
        let doc = parse_csv(input, b',')?;
        let arr = doc.value().as_array().ok_or("expected array")?;
        assert_eq!(
            arr[0]["note"],
            serde_json::Value::String("has a comma, here".to_string())
        );
        Ok(())
    }

    #[test]
    fn csv_headers_only() -> Result<(), Box<dyn std::error::Error>> {
        let input = "a,b,c\n";
        let doc = parse_csv(input, b',')?;
        let arr = doc.value().as_array().ok_or("expected array")?;
        assert!(arr.is_empty());
        Ok(())
    }

    #[test]
    fn csv_single_row() -> Result<(), Box<dyn std::error::Error>> {
        let input = "x,y\n10,20\n";
        let doc = parse_csv(input, b',')?;
        let arr = doc.value().as_array().ok_or("expected array")?;
        assert_eq!(arr.len(), 1);
        Ok(())
    }

    #[test]
    fn tsv_works() -> Result<(), Box<dyn std::error::Error>> {
        let input = "name\tage\nAlice\t30\n";
        let doc = parse_csv(input, b'\t')?;
        let arr = doc.value().as_array().ok_or("expected array")?;
        assert_eq!(arr.len(), 1);
        assert_eq!(
            arr[0]["name"],
            serde_json::Value::String("Alice".to_string())
        );
        assert_eq!(arr[0]["age"], serde_json::json!(30));
        Ok(())
    }

    // ---- Format detection tests ----

    #[test]
    fn detect_json_ext() {
        assert_eq!(detect_format(Path::new("data.json")), InputFormat::Json);
    }

    #[test]
    fn detect_ndjson_ext() {
        assert_eq!(detect_format(Path::new("data.ndjson")), InputFormat::Ndjson);
        assert_eq!(detect_format(Path::new("data.jsonl")), InputFormat::Ndjson);
    }

    #[test]
    fn detect_yaml_ext() {
        assert_eq!(detect_format(Path::new("config.yaml")), InputFormat::Yaml);
        assert_eq!(detect_format(Path::new("config.yml")), InputFormat::Yaml);
    }

    #[test]
    fn detect_csv_tsv_ext() {
        assert_eq!(detect_format(Path::new("data.csv")), InputFormat::Csv);
        assert_eq!(detect_format(Path::new("data.tsv")), InputFormat::Tsv);
    }

    #[test]
    fn sniff_json_content() {
        assert_eq!(sniff_format(r#"{"a":1}"#), InputFormat::Json);
    }

    #[test]
    fn sniff_yaml_content() {
        assert_eq!(sniff_format("---\nname: Alice\n"), InputFormat::Yaml);
    }

    // ---- Determinism tests ----

    #[test]
    fn ndjson_deterministic() -> Result<(), Box<dyn std::error::Error>> {
        let input = r#"{"a":1,"b":2}
{"c":3}
{"d":4,"e":5}"#;
        let reference = parse_ndjson(input)?;
        for _ in 0..10 {
            let docs = parse_ndjson(input)?;
            assert_eq!(docs.len(), reference.len());
            for (a, b) in docs.iter().zip(reference.iter()) {
                assert_eq!(a.value(), b.value());
            }
        }
        Ok(())
    }

    #[test]
    fn yaml_deterministic() -> Result<(), Box<dyn std::error::Error>> {
        let input = "name: Alice\nage: 30\nitems:\n  - one\n  - two\n";
        let reference = parse_yaml(input)?;
        for _ in 0..10 {
            let docs = parse_yaml(input)?;
            assert_eq!(docs.len(), reference.len());
            for (a, b) in docs.iter().zip(reference.iter()) {
                assert_eq!(a.value(), b.value());
            }
        }
        Ok(())
    }

    #[test]
    fn csv_deterministic() -> Result<(), Box<dyn std::error::Error>> {
        let input = "x,y,z\n1,2,3\n4,5,6\n";
        let reference = parse_csv(input, b',')?;
        for _ in 0..10 {
            let doc = parse_csv(input, b',')?;
            assert_eq!(doc.value(), reference.value());
        }
        Ok(())
    }

    // ---- Chaos / stress tests ----

    #[test]
    fn csv_10k_rows() -> Result<(), Box<dyn std::error::Error>> {
        let mut buf = String::from("id,value\n");
        for i in 0..10_000 {
            buf.push_str(&format!("{i},val_{i}\n"));
        }
        let doc = parse_csv(&buf, b',')?;
        let arr = doc.value().as_array().ok_or("expected array")?;
        assert_eq!(arr.len(), 10_000);
        Ok(())
    }

    #[test]
    fn ndjson_10k_lines() -> Result<(), Box<dyn std::error::Error>> {
        let mut buf = String::new();
        for i in 0..10_000 {
            buf.push_str(&format!("{{\"i\":{i}}}\n"));
        }
        let docs = parse_ndjson(&buf)?;
        assert_eq!(docs.len(), 10_000);
        Ok(())
    }

    #[test]
    fn yaml_deeply_nested() -> Result<(), Box<dyn std::error::Error>> {
        // Build 50-level deep YAML
        let mut yaml = String::new();
        for i in 0..50 {
            let indent = "  ".repeat(i);
            yaml.push_str(&format!("{indent}level{i}:\n"));
        }
        let indent = "  ".repeat(50);
        yaml.push_str(&format!("{indent}leaf: true\n"));
        let docs = parse_yaml(&yaml)?;
        assert_eq!(docs.len(), 1);
        Ok(())
    }

    #[test]
    fn csv_empty_file() {
        let result = parse_csv("", b',');
        assert!(result.is_err(), "expected error for empty CSV");
    }

    #[test]
    fn ndjson_all_empty_lines() -> Result<(), Box<dyn std::error::Error>> {
        let input = "\n\n  \n \n\n";
        let docs = parse_ndjson(input)?;
        assert!(docs.is_empty());
        Ok(())
    }

    #[test]
    fn yaml_malformed_returns_error() {
        let input = ":\n  - :\n    bad: [unterminated";
        let result = parse_yaml(input);
        assert!(result.is_err(), "expected error for malformed YAML");
    }

    #[test]
    fn csv_inconsistent_columns() -> Result<(), Box<dyn std::error::Error>> {
        let input = "a,b,c\n1,2\n4,5,6,7\n";
        // flexible(true) means this should not panic
        let doc = parse_csv(input, b',')?;
        let arr = doc.value().as_array().ok_or("expected array")?;
        assert_eq!(arr.len(), 2);
        // First row has fewer columns: missing column becomes null
        assert!(arr[0]["c"].is_null());
        Ok(())
    }

    // ---- parse_auto integration tests ----

    #[test]
    fn parse_auto_json() -> Result<(), Box<dyn std::error::Error>> {
        let input = r#"{"key":"value"}"#;
        let docs = parse_auto(input, Some(InputFormat::Json))?;
        assert_eq!(docs.len(), 1);
        Ok(())
    }

    #[test]
    fn parse_auto_ndjson() -> Result<(), Box<dyn std::error::Error>> {
        let input = "{\"a\":1}\n{\"b\":2}\n";
        let docs = parse_auto(input, Some(InputFormat::Ndjson))?;
        assert_eq!(docs.len(), 2);
        Ok(())
    }

    #[test]
    fn parse_auto_yaml() -> Result<(), Box<dyn std::error::Error>> {
        let input = "name: Alice\nage: 30\n";
        let docs = parse_auto(input, Some(InputFormat::Yaml))?;
        assert_eq!(docs.len(), 1);
        Ok(())
    }

    #[test]
    fn parse_auto_csv() -> Result<(), Box<dyn std::error::Error>> {
        let input = "a,b\n1,2\n";
        let docs = parse_auto(input, Some(InputFormat::Csv))?;
        assert_eq!(docs.len(), 1);
        Ok(())
    }

    #[test]
    fn parse_file_auto_nonexistent() {
        let result = parse_file_auto(Path::new("/nonexistent/file.yaml"), None);
        assert!(result.is_err());
    }

    #[test]
    fn detect_unknown_extension_defaults_json() {
        assert_eq!(detect_format(&PathBuf::from("file.xyz")), InputFormat::Json);
    }

    // ---- Markdown / PDF format detection tests ----

    #[test]
    fn detect_markdown_ext() {
        assert_eq!(detect_format(Path::new("README.md")), InputFormat::Markdown);
        assert_eq!(
            detect_format(Path::new("doc.markdown")),
            InputFormat::Markdown
        );
    }

    #[test]
    fn detect_pdf_ext() {
        assert_eq!(detect_format(Path::new("report.pdf")), InputFormat::Pdf);
    }

    #[test]
    fn sniff_markdown_heading() {
        assert_eq!(
            sniff_format("# Title\n\nSome text.\n"),
            InputFormat::Markdown
        );
        assert_eq!(sniff_format("## Sub heading\n"), InputFormat::Markdown);
    }

    #[test]
    fn parse_auto_markdown() -> Result<(), Box<dyn std::error::Error>> {
        let input = "# Hello\n\nWorld.\n";
        let docs = parse_auto(input, Some(InputFormat::Markdown))?;
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].value()["type"], "document");
        Ok(())
    }

    #[test]
    fn parse_auto_pdf_from_string_errors() {
        // PDF cannot be parsed from text — must use binary API.
        let result = parse_auto("not a pdf", Some(InputFormat::Pdf));
        assert!(result.is_err());
    }

    #[test]
    fn parse_auto_sniff_detects_markdown() -> Result<(), Box<dyn std::error::Error>> {
        let input = "# Heading\n\nParagraph content.\n";
        let docs = parse_auto(input, None)?;
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].value()["type"], "document");
        Ok(())
    }
}

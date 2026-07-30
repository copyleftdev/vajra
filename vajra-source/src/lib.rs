//! Source code parsing for Vajra.
//!
//! Bridges any programming language into Vajra's structured data world
//! by parsing source code via tree-sitter into a concrete syntax tree (CST),
//! then converting that CST into a JSON tree that the existing Vajra pipeline
//! (entropy, anomalies, fingerprinting, drift, motifs, essence) can analyze.
//!
//! # Usage
//!
//! ```no_run
//! use vajra_source::{parse_source_file, SourceConfig};
//! use std::path::Path;
//!
//! let config = SourceConfig::default();
//! let doc = parse_source_file(Path::new("main.rs"), &config).unwrap();
//! // doc is a standard vajra_types::Document — use with any Vajra analyzer
//! ```

pub mod config;
pub mod convert;
pub mod detect;
mod grammar;
pub mod semantic;

use std::path::Path;

use vajra_types::document::Document;
use vajra_types::error::VajraError;

pub use config::{SourceConfig, SourceLanguage};
pub use detect::{available_languages, detect_language, is_source_file};

/// Parse source code bytes into a Vajra Document.
///
/// The language must be specified in `config.language`, or this function
/// returns an error.
///
/// # Errors
/// - `VajraError::Config` if no language is specified or the language is unsupported
/// - `VajraError::Parse` if tree-sitter parsing fails
pub fn parse_source(source: &[u8], config: &SourceConfig) -> Result<Document, VajraError> {
    let lang = config.language.ok_or_else(|| VajraError::Config {
        message: "source language must be specified when parsing from bytes".to_owned(),
    })?;

    parse_with_language(source, lang, config)
}

/// Parse a source file from disk into a Vajra Document.
///
/// Language is auto-detected from the file extension unless overridden
/// in the config.
///
/// # Errors
/// - `VajraError::Io` if the file cannot be read
/// - `VajraError::LimitExceeded` if the file exceeds `config.max_file_size`
/// - `VajraError::Config` if the language cannot be detected
/// - `VajraError::Parse` if tree-sitter parsing fails
pub fn parse_source_file(path: &Path, config: &SourceConfig) -> Result<Document, VajraError> {
    // Detect language
    let lang = config
        .language
        .or_else(|| detect::detect_language(path))
        .ok_or_else(|| VajraError::Config {
            message: format!(
                "cannot detect language for '{}'. Use --lang to specify explicitly.",
                path.display()
            ),
        })?;

    // Check file size
    let metadata = std::fs::metadata(path).map_err(|e| VajraError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    if metadata.len() > config.max_file_size {
        return Err(VajraError::LimitExceeded {
            message: format!(
                "file size {} bytes exceeds max_file_size {} bytes",
                metadata.len(),
                config.max_file_size
            ),
        });
    }

    // Read file
    let source = std::fs::read(path).map_err(|e| VajraError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;

    parse_with_language(&source, lang, config)
}

/// Internal: parse source bytes with a known language.
fn parse_with_language(
    source: &[u8],
    lang: SourceLanguage,
    config: &SourceConfig,
) -> Result<Document, VajraError> {
    // Get the tree-sitter grammar
    let ts_language = grammar::get_grammar(lang)?;

    // Create parser and set language
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&ts_language)
        .map_err(|e| VajraError::Config {
            message: format!("failed to set tree-sitter language for {lang}: {e}"),
        })?;

    // Parse the source
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| VajraError::Parse {
            message: format!("tree-sitter returned no tree for {lang} source"),
            byte_offset: 0,
            source_path: None,
        })?;

    // Convert CST to JSON
    let root = tree.root_node();
    let json_value = convert::tree_to_json(root, source, config, &lang.to_string());

    // Build the Document straight from the value. Round-tripping it through a
    // JSON string cost a full copy of the CST and, when the re-parse hit
    // serde_json's recursion limit, produced an error whose byte offset indexed
    // that string rather than the source file — "byte 202923" for a 50 KB
    // input. `walk` enforces its own depth limit and reports the offending
    // path, so nothing is lost by skipping the round trip. See #90.
    vajra_core::parse::parse_value(json_value, source.len() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Any byte offset a parse failure reports must be a position that exists
    /// in the input. Serialising the CST and re-parsing it meant offsets
    /// indexed that intermediate string: a 50,646-byte file reported a parse
    /// error at byte 202,923, line 1 column 202,923, in a 968-line file whose
    /// longest line was 142 characters. See #90.
    #[test]
    #[cfg(feature = "python")]
    fn a_reported_byte_offset_never_exceeds_the_input() {
        // Deep nesting is what used to trip serde_json's recursion limit
        // during the round trip.
        let mut source = String::new();
        for depth in 0..400 {
            source.push_str(&" ".repeat(depth));
            source.push_str("if x:\n");
        }
        source.push_str(&" ".repeat(400));
        source.push_str("pass\n");

        let config = SourceConfig {
            language: Some(SourceLanguage::Python),
            ..SourceConfig::default()
        };
        match parse_source(source.as_bytes(), &config) {
            Ok(doc) => {
                assert_eq!(
                    doc.metadata().raw_size_bytes,
                    source.len() as u64,
                    "raw_size_bytes must be the input, not an intermediate form"
                );
            }
            Err(VajraError::Parse { byte_offset, .. }) => {
                assert!(
                    byte_offset <= source.len(),
                    "reported byte {byte_offset} is past the end of a {}-byte input",
                    source.len()
                );
            }
            // Depth limits are reported by path, not by offset, which is the
            // honest way to describe a failure with no source position.
            Err(_) => {}
        }
    }

    /// Depth beyond the cap is marked, not silently flattened: a truncated
    /// node must be distinguishable from a genuine leaf.
    #[test]
    #[cfg(feature = "python")]
    fn deep_nesting_is_truncated_visibly_rather_than_overflowing() {
        let mut source = String::new();
        for depth in 0..400 {
            source.push_str(&" ".repeat(depth));
            source.push_str("if x:\n");
        }
        source.push_str(&" ".repeat(400));
        source.push_str("pass\n");

        let config = SourceConfig {
            language: Some(SourceLanguage::Python),
            ..SourceConfig::default()
        };
        let doc = parse_source(source.as_bytes(), &config).expect("deep source still parses");
        let rendered = doc.value().to_string();
        assert!(
            rendered.contains("\"truncated\":true"),
            "the cut must be visible in the document"
        );
    }

    #[test]
    #[cfg(feature = "python")]
    fn raw_size_bytes_is_the_source_length() {
        let source = b"def f(items):\n    return [i for i in items if i]\n";
        let config = SourceConfig {
            language: Some(SourceLanguage::Python),
            ..SourceConfig::default()
        };
        let doc = parse_source(source, &config).expect("python source parses");
        assert_eq!(doc.metadata().raw_size_bytes, source.len() as u64);
    }

    #[test]
    #[cfg(feature = "rust")]
    fn parse_rust_hello_world() -> Result<(), Box<dyn std::error::Error>> {
        let source = b"fn main() { println!(\"hello\"); }";
        let config = SourceConfig {
            language: Some(SourceLanguage::Rust),
            ..SourceConfig::default()
        };
        let doc = parse_source(source, &config)?;
        assert!(doc.metadata().total_nodes > 0);
        assert!(doc.metadata().max_depth > 0);
        Ok(())
    }

    #[test]
    #[cfg(feature = "python")]
    fn parse_python_hello_world() -> Result<(), Box<dyn std::error::Error>> {
        let source = b"def hello():\n    print('hello')\n";
        let config = SourceConfig {
            language: Some(SourceLanguage::Python),
            ..SourceConfig::default()
        };
        let doc = parse_source(source, &config)?;
        assert!(doc.metadata().total_nodes > 0);
        Ok(())
    }

    #[test]
    #[cfg(feature = "go")]
    fn parse_go_hello_world() -> Result<(), Box<dyn std::error::Error>> {
        let source = b"package main\n\nfunc main() {\n\tfmt.Println(\"hello\")\n}\n";
        let config = SourceConfig {
            language: Some(SourceLanguage::Go),
            ..SourceConfig::default()
        };
        let doc = parse_source(source, &config)?;
        assert!(doc.metadata().total_nodes > 0);
        Ok(())
    }

    #[test]
    #[cfg(feature = "javascript")]
    fn parse_javascript_hello_world() -> Result<(), Box<dyn std::error::Error>> {
        let source = b"function hello() { console.log('hello'); }\n";
        let config = SourceConfig {
            language: Some(SourceLanguage::JavaScript),
            ..SourceConfig::default()
        };
        let doc = parse_source(source, &config)?;
        assert!(doc.metadata().total_nodes > 0);
        Ok(())
    }

    #[test]
    #[cfg(feature = "rust")]
    fn determinism_10_runs() -> Result<(), Box<dyn std::error::Error>> {
        let source = br#"
            struct Point { x: f64, y: f64 }
            impl Point {
                fn new(x: f64, y: f64) -> Self { Self { x, y } }
                fn distance(&self, other: &Point) -> f64 {
                    ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
                }
            }
        "#;
        let config = SourceConfig {
            language: Some(SourceLanguage::Rust),
            ..SourceConfig::default()
        };

        let mut outputs = Vec::new();
        for _ in 0..10 {
            let doc = parse_source(source, &config)?;
            // Compare full trie structure, not just metadata
            let paths: Vec<String> = doc
                .trie()
                .all_paths()
                .iter()
                .map(|p| p.to_string())
                .collect();
            let meta = serde_json::to_string(&doc.metadata())?;
            outputs.push(format!("{meta}|{paths:?}"));
        }
        for (i, output) in outputs.iter().enumerate().skip(1) {
            assert_eq!(
                &outputs[0], output,
                "run 0 and run {} produced different output",
                i
            );
        }
        Ok(())
    }

    #[test]
    #[cfg(feature = "rust")]
    fn error_nodes_in_malformed_code() -> Result<(), Box<dyn std::error::Error>> {
        // Malformed Rust — tree-sitter will produce ERROR nodes
        let source = b"fn main( { let x = ; }";
        let config = SourceConfig {
            language: Some(SourceLanguage::Rust),
            ..SourceConfig::default()
        };
        // Should not error — tree-sitter handles partial parses
        let doc = parse_source(source, &config)?;
        assert!(doc.metadata().total_nodes > 0);
        Ok(())
    }

    #[test]
    #[cfg(feature = "rust")]
    fn include_spans() -> Result<(), Box<dyn std::error::Error>> {
        let source = b"fn main() {}";
        let config = SourceConfig {
            language: Some(SourceLanguage::Rust),
            include_spans: true,
            ..SourceConfig::default()
        };
        let doc = parse_source(source, &config)?;
        // Just verify it parses — span data is in the tree
        assert!(doc.metadata().total_nodes > 0);
        Ok(())
    }

    #[test]
    #[cfg(feature = "rust")]
    fn include_anonymous_nodes() -> Result<(), Box<dyn std::error::Error>> {
        let source = b"fn main() {}";
        let config_named = SourceConfig {
            language: Some(SourceLanguage::Rust),
            include_anonymous: false,
            ..SourceConfig::default()
        };
        let config_all = SourceConfig {
            language: Some(SourceLanguage::Rust),
            include_anonymous: true,
            ..SourceConfig::default()
        };
        let doc_named = parse_source(source, &config_named)?;
        let doc_all = parse_source(source, &config_all)?;
        // Including anonymous nodes should produce more nodes
        assert!(doc_all.metadata().total_nodes >= doc_named.metadata().total_nodes);
        Ok(())
    }

    #[test]
    fn no_language_returns_error() {
        let source = b"some code";
        let config = SourceConfig::default(); // no language set
        let result = parse_source(source, &config);
        assert!(result.is_err());
    }

    #[test]
    #[cfg(feature = "rust")]
    fn detect_language_from_extension() {
        assert_eq!(
            detect_language(Path::new("main.rs")),
            Some(SourceLanguage::Rust)
        );
    }

    #[test]
    #[cfg(feature = "python")]
    fn detect_python_from_extension() {
        assert_eq!(
            detect_language(Path::new("app.py")),
            Some(SourceLanguage::Python)
        );
    }

    #[test]
    fn unknown_extension_returns_none() {
        assert_eq!(detect_language(Path::new("data.xyz")), None);
    }

    #[test]
    fn available_languages_not_empty() {
        let langs = available_languages();
        assert!(
            !langs.is_empty(),
            "at least one language should be available"
        );
    }

    #[test]
    #[cfg(feature = "rust")]
    fn semantic_paths_parses_without_error() -> Result<(), Box<dyn std::error::Error>> {
        let source = b"fn main() { let x = 42; }";
        let config = SourceConfig {
            language: Some(SourceLanguage::Rust),
            semantic_paths: true,
            ..SourceConfig::default()
        };
        let doc = parse_source(source, &config)?;
        assert!(doc.metadata().total_nodes > 0);
        Ok(())
    }

    #[test]
    #[cfg(feature = "rust")]
    fn semantic_paths_off_backward_compat() -> Result<(), Box<dyn std::error::Error>> {
        let source = b"fn main() {}";
        let config_off = SourceConfig {
            language: Some(SourceLanguage::Rust),
            semantic_paths: false,
            ..SourceConfig::default()
        };
        let config_default = SourceConfig {
            language: Some(SourceLanguage::Rust),
            ..SourceConfig::default()
        };
        let doc_off = parse_source(source, &config_off)?;
        let doc_default = parse_source(source, &config_default)?;
        // Both should produce identical metadata since semantic_paths defaults to false
        let meta_off = serde_json::to_string(&doc_off.metadata())?;
        let meta_default = serde_json::to_string(&doc_default.metadata())?;
        assert_eq!(meta_off, meta_default);
        Ok(())
    }
}

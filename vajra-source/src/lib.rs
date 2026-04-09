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

    // Serialize to string and parse through vajra-core to build Document
    let json_string = serde_json::to_string(&json_value).map_err(|e| VajraError::Parse {
        message: format!("failed to serialize CST to JSON: {e}"),
        byte_offset: 0,
        source_path: None,
    })?;

    vajra_core::parse::parse_str(&json_string)
}

#[cfg(test)]
mod tests {
    use super::*;

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

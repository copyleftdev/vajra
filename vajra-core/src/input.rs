//! Unified input resolution and loading for Vajra.
//!
//! Handles files (plain or compressed), standard input, and HTTP URLs.
//! This module is the master entry point for loading documents from any
//! supported source, building on top of the [`formats`](crate::formats) and
//! [`decompress`](crate::decompress) modules.

use std::path::{Path, PathBuf};

use vajra_types::{Document, VajraError};

use crate::decompress;
use crate::formats::{self, InputFormat};

/// A resolved input source before loading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputSource {
    /// Local file path.
    File(PathBuf),
    /// Standard input (the "-" convention).
    Stdin,
    /// HTTP or HTTPS URL.
    Url(String),
}

/// Loaded and decoded input content, ready for parsing.
#[derive(Debug, Clone)]
pub struct LoadedInput {
    /// The decoded text content.
    pub content: String,
    /// Detected (or overridden) input format.
    pub format: InputFormat,
    /// Human-readable name of the source (file path, URL, or "stdin").
    pub source_name: String,
}

/// Resolve an input string to its source type.
///
/// - `"-"` maps to [`InputSource::Stdin`]
/// - Strings starting with `http://` or `https://` map to [`InputSource::Url`]
/// - Everything else maps to [`InputSource::File`]
#[must_use]
pub fn resolve_input(input: &str) -> InputSource {
    if input == "-" {
        InputSource::Stdin
    } else if crate::fetch::is_url(input) {
        InputSource::Url(input.to_string())
    } else {
        InputSource::File(PathBuf::from(input))
    }
}

/// Load content from any resolved source, handling decompression.
///
/// # Errors
///
/// Returns [`VajraError::Io`] if the source cannot be read.
/// Returns [`VajraError::Parse`] if decompression or UTF-8 validation fails.
/// Returns [`VajraError::LimitExceeded`] if the content exceeds size limits.
pub fn load_input(source: &InputSource) -> Result<LoadedInput, VajraError> {
    match source {
        InputSource::File(path) => load_from_file(path),
        InputSource::Stdin => load_from_stdin(),
        InputSource::Url(url) => load_from_url(url),
    }
}

/// Full pipeline: resolve input string, load content, detect format, and parse.
///
/// Returns a `Vec<Document>` because NDJSON and multi-document YAML may
/// produce multiple documents from a single source.
///
/// An optional `format_override` forces a specific input format instead of
/// auto-detection.
///
/// # Errors
///
/// Returns errors from loading or parsing stages.
pub fn load_documents(
    input: &str,
    format_override: Option<InputFormat>,
) -> Result<Vec<Document>, VajraError> {
    let source = resolve_input(input);
    let mut loaded = load_input(&source)?;

    if let Some(fmt) = format_override {
        loaded.format = fmt;
    }

    formats::parse_auto(&loaded.content, Some(loaded.format))
}

// ---------------------------------------------------------------------------
// Private loading helpers
// ---------------------------------------------------------------------------

fn load_from_file(path: &Path) -> Result<LoadedInput, VajraError> {
    let content = decompress::decompress_file(path)?;
    let inner_path = decompress::strip_compression_extension(path);
    let format = detect_format_for_path(&inner_path, &content);

    Ok(LoadedInput {
        content,
        format,
        source_name: path.display().to_string(),
    })
}

fn load_from_stdin() -> Result<LoadedInput, VajraError> {
    use std::io::Read;

    let mut buf = Vec::new();
    std::io::stdin()
        .read_to_end(&mut buf)
        .map_err(|e| VajraError::Io {
            path: PathBuf::from("<stdin>"),
            source: e,
        })?;

    if buf.is_empty() {
        return Err(VajraError::Parse {
            byte_offset: 0,
            message: "stdin is empty".to_string(),
            source_path: None,
        });
    }

    // Detect and apply decompression from magic bytes
    let compression = decompress::detect_compression_from_bytes(&buf);
    let content = decompress::decompress_bytes(&buf, compression)?;
    let format = formats::sniff_format(&content);

    Ok(LoadedInput {
        content,
        format,
        source_name: "<stdin>".to_string(),
    })
}

#[cfg(feature = "http")]
fn load_from_url(url: &str) -> Result<LoadedInput, VajraError> {
    let content = crate::fetch::fetch_url(url)?;
    let format = detect_format_for_url(url, &content);

    Ok(LoadedInput {
        content,
        format,
        source_name: url.to_string(),
    })
}

#[cfg(not(feature = "http"))]
fn load_from_url(url: &str) -> Result<LoadedInput, VajraError> {
    Err(VajraError::Config {
        message: format!(
            "HTTP support is not enabled. Rebuild with the 'http' feature to fetch URLs. \
             Requested URL: {url}"
        ),
    })
}

// ---------------------------------------------------------------------------
// Format detection helpers
// ---------------------------------------------------------------------------

/// Detect format from a (possibly compression-stripped) path, falling back
/// to content sniffing.
fn detect_format_for_path(path: &Path, content: &str) -> InputFormat {
    let ext_format = formats::detect_format(path);
    // detect_format defaults to Json for unknown extensions, so we confirm
    // there actually is a known extension before trusting it.
    let has_known_ext = path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        matches!(
            e,
            "json" | "ndjson" | "jsonl" | "yaml" | "yml" | "csv" | "tsv" | "cpuprofile"
        )
    });

    if has_known_ext {
        ext_format
    } else {
        formats::sniff_format(content)
    }
}

/// Detect format from URL path, falling back to content sniffing.
#[cfg(any(feature = "http", test))]
fn detect_format_for_url(url: &str, content: &str) -> InputFormat {
    // Extract path portion (before query string / fragment)
    let path_part = url.split('?').next().unwrap_or(url);
    let path_part = path_part.split('#').next().unwrap_or(path_part);

    let path = Path::new(path_part);
    let inner = decompress::strip_compression_extension(path);
    detect_format_for_path(&inner, content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // -----------------------------------------------------------------------
    // resolve_input tests
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_stdin() {
        assert_eq!(resolve_input("-"), InputSource::Stdin);
    }

    #[test]
    fn resolve_http_url() {
        assert_eq!(
            resolve_input("http://example.com"),
            InputSource::Url("http://example.com".to_string())
        );
    }

    #[test]
    fn resolve_https_url() {
        assert_eq!(
            resolve_input("https://api.test/data"),
            InputSource::Url("https://api.test/data".to_string())
        );
    }

    #[test]
    fn resolve_absolute_path() {
        assert_eq!(
            resolve_input("/path/to/file.json"),
            InputSource::File(PathBuf::from("/path/to/file.json"))
        );
    }

    #[test]
    fn resolve_relative_path() {
        assert_eq!(
            resolve_input("relative.json"),
            InputSource::File(PathBuf::from("relative.json"))
        );
    }

    // -----------------------------------------------------------------------
    // Integration tests: load_documents with temp files
    // -----------------------------------------------------------------------

    #[test]
    fn load_json_file() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.json");
        std::fs::write(&path, r#"{"name":"Alice","age":30}"#)?;

        let docs = load_documents(path.to_str().unwrap_or(""), None);
        assert!(docs.is_ok(), "load_documents failed: {docs:?}");
        let docs = docs.unwrap_or_default();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].metadata().total_nodes, 3); // root + name + age
        Ok(())
    }

    #[test]
    fn load_ndjson_file() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.ndjson");
        std::fs::write(&path, "{\"a\":1}\n{\"b\":2}\n{\"c\":3}\n")?;

        let docs = load_documents(path.to_str().unwrap_or(""), None);
        assert!(docs.is_ok(), "load_documents failed: {docs:?}");
        let docs = docs.unwrap_or_default();
        assert_eq!(docs.len(), 3);
        Ok(())
    }

    #[test]
    fn load_yaml_file() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.yaml");
        std::fs::write(&path, "name: Alice\nage: 30\ncity: NYC\n")?;

        let docs = load_documents(path.to_str().unwrap_or(""), None);
        assert!(docs.is_ok(), "load_documents failed: {docs:?}");
        let docs = docs.unwrap_or_default();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].metadata().total_nodes, 4); // root + 3 fields
        Ok(())
    }

    #[test]
    fn load_csv_file() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.csv");
        std::fs::write(&path, "name,age\nAlice,30\nBob,25\n")?;

        let docs = load_documents(path.to_str().unwrap_or(""), None);
        assert!(docs.is_ok(), "load_documents failed: {docs:?}");
        let docs = docs.unwrap_or_default();
        assert_eq!(docs.len(), 1);
        // CSV becomes array of objects: [{"name":"Alice","age":30},{"name":"Bob","age":25}]
        // root array + 2 objects + 2 fields each = 1 + 2 + 4 = 7
        assert_eq!(docs[0].metadata().total_nodes, 7);
        Ok(())
    }

    #[test]
    fn load_compressed_json_gz() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.json.gz");
        let original = r#"{"compressed":"gzip"}"#;

        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(original.as_bytes())?;
        let compressed = encoder.finish()?;
        std::fs::write(&path, &compressed)?;

        let docs = load_documents(path.to_str().unwrap_or(""), None);
        assert!(docs.is_ok(), "load_documents failed: {docs:?}");
        let docs = docs.unwrap_or_default();
        assert_eq!(docs.len(), 1);
        Ok(())
    }

    #[test]
    fn load_compressed_json_zst() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.json.zst");
        let original = r#"{"compressed":"zstd"}"#;

        let compressed = zstd::encode_all(original.as_bytes(), 3)?;
        std::fs::write(&path, &compressed)?;

        let docs = load_documents(path.to_str().unwrap_or(""), None);
        assert!(docs.is_ok(), "load_documents failed: {docs:?}");
        let docs = docs.unwrap_or_default();
        assert_eq!(docs.len(), 1);
        Ok(())
    }

    #[test]
    fn load_compressed_ndjson_gz() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.ndjson.gz");
        let original = "{\"a\":1}\n{\"b\":2}\n";

        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(original.as_bytes())?;
        let compressed = encoder.finish()?;
        std::fs::write(&path, &compressed)?;

        let docs = load_documents(path.to_str().unwrap_or(""), None);
        assert!(docs.is_ok(), "load_documents failed: {docs:?}");
        let docs = docs.unwrap_or_default();
        assert_eq!(docs.len(), 2);
        Ok(())
    }

    #[test]
    fn load_with_format_override() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        // Write NDJSON content but with .txt extension
        let path = dir.path().join("data.txt");
        std::fs::write(&path, "{\"a\":1}\n{\"b\":2}\n")?;

        let docs = load_documents(path.to_str().unwrap_or(""), Some(InputFormat::Ndjson));
        assert!(docs.is_ok(), "load_documents failed: {docs:?}");
        let docs = docs.unwrap_or_default();
        assert_eq!(docs.len(), 2);
        Ok(())
    }

    #[test]
    fn load_nonexistent_file_returns_error() {
        let result = load_documents("/nonexistent/path/file.json", None);
        assert!(result.is_err());
    }

    #[test]
    fn binary_garbage_as_json_returns_parse_error() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("garbage.json");
        std::fs::write(&path, "this is not { valid json at all !!!")?;

        let result = load_documents(path.to_str().unwrap_or(""), None);
        assert!(result.is_err());
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Determinism tests with compressed input
    // -----------------------------------------------------------------------

    #[test]
    fn deterministic_gzip_parse() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("det.json.gz");
        let original = r#"{"items":[1,2,3],"name":"deterministic"}"#;

        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(original.as_bytes())?;
        let compressed = encoder.finish()?;
        std::fs::write(&path, &compressed)?;

        let path_str = path.to_str().unwrap_or("");
        let mut prev_nodes = None;
        for _ in 0..10 {
            let docs = load_documents(path_str, None)?;
            assert_eq!(docs.len(), 1);
            let nodes = docs[0].metadata().total_nodes;
            if let Some(prev) = prev_nodes {
                assert_eq!(nodes, prev);
            }
            prev_nodes = Some(nodes);
        }
        Ok(())
    }

    #[test]
    fn deterministic_zstd_parse() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("det.json.zst");
        let original = r#"{"items":[4,5,6],"name":"zstd_det"}"#;

        let compressed = zstd::encode_all(original.as_bytes(), 3)?;
        std::fs::write(&path, &compressed)?;

        let path_str = path.to_str().unwrap_or("");
        let mut prev_nodes = None;
        for _ in 0..10 {
            let docs = load_documents(path_str, None)?;
            assert_eq!(docs.len(), 1);
            let nodes = docs[0].metadata().total_nodes;
            if let Some(prev) = prev_nodes {
                assert_eq!(nodes, prev);
            }
            prev_nodes = Some(nodes);
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // URL format detection
    // -----------------------------------------------------------------------

    #[test]
    fn detect_format_from_url_json() {
        assert_eq!(
            detect_format_for_url("https://example.com/data.json", ""),
            InputFormat::Json
        );
    }

    #[test]
    fn detect_format_from_url_ndjson() {
        assert_eq!(
            detect_format_for_url("https://example.com/data.ndjson", ""),
            InputFormat::Ndjson
        );
    }

    #[test]
    fn detect_format_from_url_json_gz() {
        assert_eq!(
            detect_format_for_url("https://example.com/data.json.gz", ""),
            InputFormat::Json
        );
    }

    #[test]
    fn detect_format_from_url_with_query() {
        assert_eq!(
            detect_format_for_url("https://example.com/data.yaml?key=val", ""),
            InputFormat::Yaml
        );
    }

    #[test]
    fn detect_format_from_url_no_extension_sniffs() {
        assert_eq!(
            detect_format_for_url("https://api.example.com/v1/data", r#"{"sniffed":"json"}"#),
            InputFormat::Json
        );
    }
}

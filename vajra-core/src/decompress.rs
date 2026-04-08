//! Compression detection and decompression for Vajra input files.
//!
//! Supports gzip and zstd compression formats. Detection is based on file
//! extension or magic bytes. Decompressed output is validated as UTF-8 and
//! subject to a configurable size limit (default 10 GB).

use std::io::Read;
use std::path::Path;

use vajra_types::VajraError;

/// Default maximum decompressed size: 10 GB.
const DEFAULT_MAX_DECOMPRESSED_SIZE: u64 = 10 * 1024 * 1024 * 1024;

/// Supported compression formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    /// No compression.
    None,
    /// Gzip compression (RFC 1952).
    Gzip,
    /// Zstandard compression (RFC 8878).
    Zstd,
}

/// Detect compression format from a file path's extension.
///
/// Recognises double extensions such as `.json.gz` and `.ndjson.zst`.
#[must_use]
pub fn detect_compression(path: &Path) -> Compression {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    if name.ends_with(".gz") || name.ends_with(".gzip") {
        return Compression::Gzip;
    }
    if name.ends_with(".zst") || name.ends_with(".zstd") {
        return Compression::Zstd;
    }

    Compression::None
}

/// Detect compression from the first few magic bytes of a buffer.
///
/// - Gzip magic: `1f 8b`
/// - Zstd magic: `28 b5 2f fd`
#[must_use]
pub fn detect_compression_from_bytes(data: &[u8]) -> Compression {
    if data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b {
        return Compression::Gzip;
    }
    if data.len() >= 4 && data[0] == 0x28 && data[1] == 0xb5 && data[2] == 0x2f && data[3] == 0xfd {
        return Compression::Zstd;
    }
    Compression::None
}

/// Strip the compression extension from a file name, returning the
/// "inner" path that can be used for format detection.
///
/// For example `data.json.gz` becomes `data.json`.
#[must_use]
pub fn strip_compression_extension(path: &Path) -> std::path::PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let lower = name.to_lowercase();
    let stripped = if lower.ends_with(".gzip") {
        &name[..name.len() - 5]
    } else if lower.ends_with(".gz") {
        &name[..name.len() - 3]
    } else if lower.ends_with(".zstd") {
        &name[..name.len() - 5]
    } else if lower.ends_with(".zst") {
        &name[..name.len() - 4]
    } else {
        return path.to_path_buf();
    };

    if let Some(parent) = path.parent() {
        parent.join(stripped)
    } else {
        std::path::PathBuf::from(stripped)
    }
}

/// Read and decompress a file to a UTF-8 string.
///
/// The compression format is auto-detected from the file extension, falling
/// back to magic-byte sniffing.
///
/// # Errors
///
/// Returns [`VajraError::Io`] if the file cannot be read.
/// Returns [`VajraError::Parse`] if decompressed output is not valid UTF-8.
/// Returns [`VajraError::LimitExceeded`] if decompressed size exceeds the limit.
pub fn decompress_file(path: &Path) -> Result<String, VajraError> {
    decompress_file_with_limit(path, DEFAULT_MAX_DECOMPRESSED_SIZE)
}

/// Read and decompress a file to a UTF-8 string with a custom size limit.
///
/// # Errors
///
/// Returns [`VajraError::Io`] if the file cannot be read.
/// Returns [`VajraError::Parse`] if decompressed output is not valid UTF-8.
/// Returns [`VajraError::LimitExceeded`] if decompressed size exceeds `max_bytes`.
pub fn decompress_file_with_limit(path: &Path, max_bytes: u64) -> Result<String, VajraError> {
    let raw = std::fs::read(path).map_err(|e| VajraError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;

    let mut compression = detect_compression(path);
    if compression == Compression::None {
        compression = detect_compression_from_bytes(&raw);
    }

    decompress_bytes_with_limit(&raw, compression, max_bytes)
}

/// Decompress a byte slice based on the specified compression format.
///
/// Uses the default 10 GB size limit.
///
/// # Errors
///
/// Returns [`VajraError::Parse`] if decompressed output is not valid UTF-8
/// or the compressed data is invalid.
/// Returns [`VajraError::LimitExceeded`] if decompressed size exceeds the limit.
pub fn decompress_bytes(data: &[u8], compression: Compression) -> Result<String, VajraError> {
    decompress_bytes_with_limit(data, compression, DEFAULT_MAX_DECOMPRESSED_SIZE)
}

/// Decompress a byte slice with a custom size limit.
///
/// # Errors
///
/// Returns [`VajraError::Parse`] on invalid compressed data or non-UTF-8 output.
/// Returns [`VajraError::LimitExceeded`] if decompressed size exceeds `max_bytes`.
pub fn decompress_bytes_with_limit(
    data: &[u8],
    compression: Compression,
    max_bytes: u64,
) -> Result<String, VajraError> {
    if data.is_empty() {
        return Err(VajraError::Parse {
            byte_offset: 0,
            message: "input data is empty".to_string(),
            source_path: None,
        });
    }

    match compression {
        Compression::None => String::from_utf8(data.to_vec()).map_err(|e| VajraError::Parse {
            byte_offset: e.utf8_error().valid_up_to(),
            message: format!("invalid UTF-8: {e}"),
            source_path: None,
        }),
        Compression::Gzip => decompress_gzip(data, max_bytes),
        Compression::Zstd => decompress_zstd(data, max_bytes),
    }
}

/// Decompress gzip data with a size limit.
fn decompress_gzip(data: &[u8], max_bytes: u64) -> Result<String, VajraError> {
    let decoder = flate2::read::GzDecoder::new(data);
    read_limited(decoder, max_bytes, "gzip")
}

/// Decompress zstd data with a size limit.
fn decompress_zstd(data: &[u8], max_bytes: u64) -> Result<String, VajraError> {
    let decoder = zstd::stream::read::Decoder::new(data).map_err(|e| VajraError::Parse {
        byte_offset: 0,
        message: format!("failed to initialize zstd decoder: {e}"),
        source_path: None,
    })?;
    read_limited(decoder, max_bytes, "zstd")
}

/// Read from a decoder into a String, enforcing a byte-count limit.
fn read_limited<R: Read>(
    mut reader: R,
    max_bytes: u64,
    format_name: &str,
) -> Result<String, VajraError> {
    let mut buf = Vec::new();
    let chunk_size: usize = 64 * 1024; // 64 KB chunks
    let mut tmp = vec![0u8; chunk_size];

    loop {
        let n = reader.read(&mut tmp).map_err(|e| VajraError::Parse {
            byte_offset: buf.len(),
            message: format!("failed to decompress {format_name} data: {e}"),
            source_path: None,
        })?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.len() as u64 > max_bytes {
            return Err(VajraError::LimitExceeded {
                message: format!(
                    "decompressed {format_name} data exceeds maximum size of {max_bytes} bytes"
                ),
            });
        }
    }

    String::from_utf8(buf).map_err(|e| VajraError::Parse {
        byte_offset: e.utf8_error().valid_up_to(),
        message: format!("decompressed {format_name} output is not valid UTF-8: {e}"),
        source_path: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Helper: compress a string with gzip.
    fn gzip_compress(input: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(input.as_bytes())?;
        Ok(encoder.finish()?)
    }

    /// Helper: compress a string with zstd.
    fn zstd_compress(input: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        Ok(zstd::encode_all(input.as_bytes(), 3)?)
    }

    // -----------------------------------------------------------------------
    // Detection tests
    // -----------------------------------------------------------------------

    #[test]
    fn detect_gz_extension() {
        assert_eq!(detect_compression(Path::new("data.gz")), Compression::Gzip);
    }

    #[test]
    fn detect_gzip_extension() {
        assert_eq!(
            detect_compression(Path::new("data.gzip")),
            Compression::Gzip
        );
    }

    #[test]
    fn detect_zst_extension() {
        assert_eq!(detect_compression(Path::new("data.zst")), Compression::Zstd);
    }

    #[test]
    fn detect_zstd_extension() {
        assert_eq!(
            detect_compression(Path::new("data.zstd")),
            Compression::Zstd
        );
    }

    #[test]
    fn detect_json_no_compression() {
        assert_eq!(
            detect_compression(Path::new("data.json")),
            Compression::None
        );
    }

    #[test]
    fn detect_double_extension_json_gz() {
        assert_eq!(
            detect_compression(Path::new("data.json.gz")),
            Compression::Gzip
        );
    }

    #[test]
    fn detect_double_extension_ndjson_gz() {
        assert_eq!(
            detect_compression(Path::new("data.ndjson.gz")),
            Compression::Gzip
        );
    }

    #[test]
    fn detect_double_extension_csv_gz() {
        assert_eq!(
            detect_compression(Path::new("data.csv.gz")),
            Compression::Gzip
        );
    }

    #[test]
    fn detect_double_extension_json_zst() {
        assert_eq!(
            detect_compression(Path::new("data.json.zst")),
            Compression::Zstd
        );
    }

    #[test]
    fn detect_double_extension_ndjson_zst() {
        assert_eq!(
            detect_compression(Path::new("data.ndjson.zst")),
            Compression::Zstd
        );
    }

    #[test]
    fn detect_magic_bytes_gzip() -> Result<(), Box<dyn std::error::Error>> {
        let data = gzip_compress(r#"{"hello":"world"}"#)?;
        assert_eq!(detect_compression_from_bytes(&data), Compression::Gzip);
        Ok(())
    }

    #[test]
    fn detect_magic_bytes_zstd() -> Result<(), Box<dyn std::error::Error>> {
        let data = zstd_compress(r#"{"hello":"world"}"#)?;
        assert_eq!(detect_compression_from_bytes(&data), Compression::Zstd);
        Ok(())
    }

    #[test]
    fn detect_magic_bytes_plain() {
        let data = b"{\"hello\":\"world\"}";
        assert_eq!(detect_compression_from_bytes(data), Compression::None);
    }

    // -----------------------------------------------------------------------
    // Decompression tests
    // -----------------------------------------------------------------------

    #[test]
    fn decompress_gzip_json() -> Result<(), Box<dyn std::error::Error>> {
        let original = r#"{"name":"Alice","age":30}"#;
        let compressed = gzip_compress(original)?;
        let result = decompress_bytes(&compressed, Compression::Gzip);
        assert!(result.is_ok(), "decompress failed: {result:?}");
        assert_eq!(result.unwrap_or_default(), original);
        Ok(())
    }

    #[test]
    fn decompress_zstd_json() -> Result<(), Box<dyn std::error::Error>> {
        let original = r#"{"name":"Bob","scores":[1,2,3]}"#;
        let compressed = zstd_compress(original)?;
        let result = decompress_bytes(&compressed, Compression::Zstd);
        assert!(result.is_ok(), "decompress failed: {result:?}");
        assert_eq!(result.unwrap_or_default(), original);
        Ok(())
    }

    #[test]
    fn decompress_none_passthrough() {
        let original = r#"{"key":"value"}"#;
        let result = decompress_bytes(original.as_bytes(), Compression::None);
        assert!(result.is_ok(), "passthrough failed: {result:?}");
        assert_eq!(result.unwrap_or_default(), original);
    }

    #[test]
    fn decompress_invalid_gzip_returns_error() {
        let garbage = b"not gzip data at all";
        let result = decompress_bytes(garbage, Compression::Gzip);
        assert!(result.is_err());
    }

    #[test]
    fn decompress_invalid_zstd_returns_error() {
        let garbage = b"not zstd data at all";
        let result = decompress_bytes(garbage, Compression::Zstd);
        assert!(result.is_err());
    }

    #[test]
    fn decompress_empty_data_returns_error() {
        let result = decompress_bytes(&[], Compression::Gzip);
        assert!(result.is_err());
    }

    #[test]
    fn decompress_empty_data_none_returns_error() {
        let result = decompress_bytes(&[], Compression::None);
        assert!(result.is_err());
    }

    #[test]
    fn decompress_empty_data_zstd_returns_error() {
        let result = decompress_bytes(&[], Compression::Zstd);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Size-limit tests
    // -----------------------------------------------------------------------

    #[test]
    fn gzip_bomb_hits_size_limit() -> Result<(), Box<dyn std::error::Error>> {
        // Create a string that compresses well but decompresses large.
        let big = "A".repeat(1_000_000); // 1 MB of 'A's
        let compressed = gzip_compress(&big)?;
        // Set a small limit
        let result = decompress_bytes_with_limit(&compressed, Compression::Gzip, 1024);
        assert!(result.is_err());
        match result {
            Err(VajraError::LimitExceeded { .. }) => {} // expected
            other => return Err(format!("expected LimitExceeded, got {other:?}").into()),
        }
        Ok(())
    }

    #[test]
    fn zstd_bomb_hits_size_limit() -> Result<(), Box<dyn std::error::Error>> {
        let big = "B".repeat(1_000_000);
        let compressed = zstd_compress(&big)?;
        let result = decompress_bytes_with_limit(&compressed, Compression::Zstd, 1024);
        assert!(result.is_err());
        match result {
            Err(VajraError::LimitExceeded { .. }) => {} // expected
            other => return Err(format!("expected LimitExceeded, got {other:?}").into()),
        }
        Ok(())
    }

    #[test]
    fn truncated_gzip_returns_error() -> Result<(), Box<dyn std::error::Error>> {
        let full = gzip_compress(r#"{"hello":"world"}"#)?;
        // Truncate: take only first half
        let truncated = &full[..full.len() / 2];
        let result = decompress_bytes(truncated, Compression::Gzip);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn binary_garbage_as_none_returns_parse_error() {
        let garbage: Vec<u8> = vec![0x00, 0x01, 0x80, 0xFF, 0xFE, 0xFD];
        let result = decompress_bytes(&garbage, Compression::None);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Strip-extension tests
    // -----------------------------------------------------------------------

    #[test]
    fn strip_gz_extension() {
        let result = strip_compression_extension(Path::new("data.json.gz"));
        assert_eq!(result, Path::new("data.json"));
    }

    #[test]
    fn strip_zst_extension() {
        let result = strip_compression_extension(Path::new("data.ndjson.zst"));
        assert_eq!(result, Path::new("data.ndjson"));
    }

    #[test]
    fn strip_no_compression_extension() {
        let result = strip_compression_extension(Path::new("data.json"));
        assert_eq!(result, Path::new("data.json"));
    }

    // -----------------------------------------------------------------------
    // Determinism tests
    // -----------------------------------------------------------------------

    #[test]
    fn decompress_gzip_deterministic() -> Result<(), Box<dyn std::error::Error>> {
        let original = r#"{"items":[1,2,3],"name":"test"}"#;
        let compressed = gzip_compress(original)?;
        for _ in 0..10 {
            let result = decompress_bytes(&compressed, Compression::Gzip)?;
            assert_eq!(result, original);
        }
        Ok(())
    }

    #[test]
    fn decompress_zstd_deterministic() -> Result<(), Box<dyn std::error::Error>> {
        let original = r#"{"items":[4,5,6],"name":"zstd_test"}"#;
        let compressed = zstd_compress(original)?;
        for _ in 0..10 {
            let result = decompress_bytes(&compressed, Compression::Zstd)?;
            assert_eq!(result, original);
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // File-based tests
    // -----------------------------------------------------------------------

    #[test]
    fn decompress_file_gzip() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.json.gz");
        let original = r#"{"file":"gzip_test"}"#;
        let compressed = gzip_compress(original)?;
        std::fs::write(&path, &compressed)?;

        let result = decompress_file(&path);
        assert!(result.is_ok(), "decompress_file failed: {result:?}");
        assert_eq!(result.unwrap_or_default(), original);
        Ok(())
    }

    #[test]
    fn decompress_file_zstd() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.json.zst");
        let original = r#"{"file":"zstd_test"}"#;
        let compressed = zstd_compress(original)?;
        std::fs::write(&path, &compressed)?;

        let result = decompress_file(&path);
        assert!(result.is_ok(), "decompress_file failed: {result:?}");
        assert_eq!(result.unwrap_or_default(), original);
        Ok(())
    }

    #[test]
    fn decompress_file_plain() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.json");
        let original = r#"{"file":"plain"}"#;
        std::fs::write(&path, original)?;

        let result = decompress_file(&path);
        assert!(result.is_ok(), "decompress_file failed: {result:?}");
        assert_eq!(result.unwrap_or_default(), original);
        Ok(())
    }

    #[test]
    fn decompress_file_nonexistent() -> Result<(), Box<dyn std::error::Error>> {
        let result = decompress_file(Path::new("/nonexistent/file.json.gz"));
        assert!(result.is_err());
        match result {
            Err(VajraError::Io { .. }) => {} // expected
            other => return Err(format!("expected Io error, got {other:?}").into()),
        }
        Ok(())
    }

    #[test]
    fn zero_length_file_returns_error() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("empty.json.gz");
        std::fs::write(&path, [])?;

        let result = decompress_file(&path);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn large_decompressed_output_limit_enforcement() -> Result<(), Box<dyn std::error::Error>> {
        // 500 KB of data, limit to 100 KB
        let big = "X".repeat(500_000);
        let compressed = gzip_compress(&big)?;
        let result = decompress_bytes_with_limit(&compressed, Compression::Gzip, 100_000);
        assert!(result.is_err());
        match result {
            Err(VajraError::LimitExceeded { .. }) => {} // expected
            other => return Err(format!("expected LimitExceeded, got {other:?}").into()),
        }
        Ok(())
    }
}

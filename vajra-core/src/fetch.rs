//! HTTP URL fetching for Vajra inputs.
//!
//! Uses `ureq` for synchronous HTTP GET requests. Gated behind the `http`
//! feature flag (enabled by default).

#[cfg(feature = "http")]
use vajra_types::VajraError;

#[cfg(feature = "http")]
use crate::decompress::{self, Compression};

/// Default timeout for HTTP requests (connect + read): 30 seconds.
#[cfg(feature = "http")]
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Default maximum response size: 100 MB.
#[cfg(feature = "http")]
const DEFAULT_MAX_RESPONSE_SIZE: u64 = 100 * 1024 * 1024;

/// Check whether a string looks like an HTTP or HTTPS URL.
#[must_use]
pub fn is_url(input: &str) -> bool {
    input.starts_with("http://") || input.starts_with("https://")
}

/// Fetch content from an HTTP or HTTPS URL.
///
/// - User-Agent: `vajra/{version}`
/// - Timeout: 30 seconds (connect + read)
/// - Max response body: 100 MB
/// - Automatically decompresses gzip/zstd responses (Content-Encoding header).
///
/// # Errors
///
/// Returns [`VajraError::Io`] on network errors, timeouts, and HTTP errors.
/// Returns [`VajraError::LimitExceeded`] if the response body exceeds the limit.
/// Returns [`VajraError::Parse`] if the body is not valid UTF-8.
#[cfg(feature = "http")]
pub fn fetch_url(url: &str) -> Result<String, VajraError> {
    fetch_url_with_limits(url, DEFAULT_TIMEOUT_SECS, DEFAULT_MAX_RESPONSE_SIZE)
}

/// Fetch content from a URL with custom timeout and size limit.
///
/// # Errors
///
/// Returns [`VajraError::Io`] on network errors, timeouts, and HTTP errors.
/// Returns [`VajraError::LimitExceeded`] if the response body exceeds `max_bytes`.
/// Returns [`VajraError::Parse`] if the body is not valid UTF-8.
#[cfg(feature = "http")]
pub fn fetch_url_with_limits(
    url: &str,
    timeout_secs: u64,
    max_bytes: u64,
) -> Result<String, VajraError> {
    use std::io::Read;
    use std::path::PathBuf;
    use std::time::Duration;

    let version = env!("CARGO_PKG_VERSION");
    let user_agent = format!("vajra/{version}");
    let timeout = Duration::from_secs(timeout_secs);

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(timeout)
        .timeout_read(timeout)
        .user_agent(&user_agent)
        .build();

    let response = agent.get(url).call().map_err(|e| {
        let message = match &e {
            ureq::Error::Status(code, resp) => {
                let status_text = resp.status_text().to_string();
                match code {
                    404 => format!("URL not found (404): {url}"),
                    500..=599 => format!("server error ({code} {status_text}): {url}"),
                    _ => format!("HTTP {code} {status_text}: {url}"),
                }
            }
            ureq::Error::Transport(transport) => {
                let kind = transport.kind();
                match kind {
                    ureq::ErrorKind::Io => format!("connection failed: {url}: {transport}"),
                    ureq::ErrorKind::ConnectionFailed => {
                        format!("connection failed: {url}: {transport}")
                    }
                    ureq::ErrorKind::Dns => {
                        format!("DNS resolution failed: {url}: {transport}")
                    }
                    _ => format!("network error fetching {url}: {transport}"),
                }
            }
        };
        VajraError::Io {
            path: PathBuf::from(url),
            source: std::io::Error::new(std::io::ErrorKind::Other, message),
        }
    })?;

    // Detect content-encoding for transparent decompression
    let content_encoding = response
        .header("content-encoding")
        .unwrap_or("")
        .to_lowercase();

    // Read the raw body bytes with size limit
    let mut body = Vec::new();
    let mut reader = response.into_reader();
    let chunk_size: usize = 64 * 1024;
    let mut tmp = vec![0u8; chunk_size];

    loop {
        let n = reader.read(&mut tmp).map_err(|e| VajraError::Io {
            path: PathBuf::from(url),
            source: e,
        })?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
        if body.len() as u64 > max_bytes {
            return Err(VajraError::LimitExceeded {
                message: format!("response from {url} exceeds maximum size of {max_bytes} bytes"),
            });
        }
    }

    if body.is_empty() {
        return Err(VajraError::Parse {
            byte_offset: 0,
            message: format!("empty response from {url}"),
            source_path: None,
        });
    }

    // Apply decompression based on Content-Encoding header
    let compression = if content_encoding.contains("gzip") {
        Compression::Gzip
    } else if content_encoding.contains("zstd") || content_encoding.contains("zstandard") {
        Compression::Zstd
    } else {
        // Fall back to magic-byte detection
        decompress::detect_compression_from_bytes(&body)
    };

    if compression == Compression::None {
        String::from_utf8(body).map_err(|e| VajraError::Parse {
            byte_offset: e.utf8_error().valid_up_to(),
            message: format!("response from {url} is not valid UTF-8: {e}"),
            source_path: None,
        })
    } else {
        decompress::decompress_bytes_with_limit(&body, compression, max_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // is_url tests
    // -----------------------------------------------------------------------

    #[test]
    fn is_url_http() {
        assert!(is_url("http://example.com"));
    }

    #[test]
    fn is_url_https() {
        assert!(is_url("https://example.com/data.json"));
    }

    #[test]
    fn is_url_absolute_path() {
        assert!(!is_url("/local/file.json"));
    }

    #[test]
    fn is_url_relative_path() {
        assert!(!is_url("relative.json"));
    }

    #[test]
    fn is_url_stdin() {
        assert!(!is_url("-"));
    }

    #[test]
    fn is_url_empty() {
        assert!(!is_url(""));
    }

    #[test]
    fn is_url_ftp_not_supported() {
        assert!(!is_url("ftp://files.example.com/data.json"));
    }

    // -----------------------------------------------------------------------
    // Network tests are skipped to avoid hitting the network in CI.
    // The fetch_url function is tested through integration tests that
    // use a local HTTP server if needed.
    // -----------------------------------------------------------------------
}

//! Encoding layer peeling.
//!
//! Recursively detects nested encodings in a string value,
//! bounded by a maximum depth to prevent adversarial depth bombs.

use vajra_types::traits::InferenceConfidence;

use crate::decode;
use crate::entropy::byte_entropy;

/// Maximum allowed peeling depth, regardless of caller request.
const HARD_MAX_DEPTH: u8 = 5;

/// A single detected encoding layer.
#[derive(Debug, Clone)]
pub struct EncodingLayer {
    /// Encoding type name (matches TypeRecognizer::type_name()).
    pub encoding: &'static str,
    /// Confidence level.
    pub confidence: InferenceConfidence,
    /// Depth (0 = outermost).
    pub depth: u8,
    /// Preview of decoded content (first bytes, bounded).
    pub decoded_preview: Option<String>,
}

/// Detect encoding layers in a string value.
///
/// Peels encodings greedily from the outside in, up to `max_depth` layers
/// (capped at 5). Returns an empty vec if no encoding is detected.
pub fn detect_encoding_layers(value: &str, max_depth: u8) -> Vec<EncodingLayer> {
    let effective_depth = max_depth.min(HARD_MAX_DEPTH);
    let mut layers = Vec::new();
    peel_recursive(value, 0, effective_depth, &mut layers);
    layers
}

fn peel_recursive(value: &str, depth: u8, max_depth: u8, layers: &mut Vec<EncodingLayer>) {
    if depth >= max_depth || value.is_empty() {
        return;
    }

    // Try each encoding in priority order
    if let Some((encoding, decoded)) = try_detect_and_decode(value) {
        let preview = make_preview(&decoded);
        layers.push(EncodingLayer {
            encoding,
            confidence: confidence_for(encoding),
            depth,
            decoded_preview: Some(preview.clone()),
        });
        // Recurse on decoded content
        peel_recursive(&preview, depth + 1, max_depth, layers);
    }
}

/// Try to detect the encoding of a value and decode it.
/// Returns (encoding_name, decoded_string) if successful.
fn try_detect_and_decode(value: &str) -> Option<(&'static str, Vec<u8>)> {
    // URL encoding (most common layering vector)
    if value.matches('%').count() >= 2 {
        if let Some(decoded) = decode::try_percent_decode(value) {
            if decoded != value.as_bytes() {
                return Some(("url_encoded", decoded));
            }
        }
    }

    // Base64 (standard)
    if value.len() >= 24 && value.len() % 4 == 0 {
        let looks_b64 = value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'=');
        if looks_b64 {
            if let Some(decoded) = decode::try_base64_decode(value) {
                let enc_entropy = byte_entropy(value.as_bytes());
                let dec_entropy = byte_entropy(&decoded);
                if enc_entropy >= 4.5 || dec_entropy < 4.0 {
                    return Some(("base64", decoded));
                }
            }
        }
    }

    // Hex encoding
    if value.len() >= 32 && value.len() % 2 == 0 && value.bytes().all(|b| b.is_ascii_hexdigit()) {
        if let Some(decoded) = decode::try_hex_decode(value) {
            return Some(("hex_encoded", decoded));
        }
    }

    None
}

fn confidence_for(encoding: &str) -> InferenceConfidence {
    match encoding {
        "url_encoded" => InferenceConfidence::Dominant,
        "base64" => InferenceConfidence::Heuristic,
        "hex_encoded" => InferenceConfidence::Heuristic,
        _ => InferenceConfidence::Heuristic,
    }
}

fn make_preview(decoded: &[u8]) -> String {
    let preview_len = decoded.len().min(256);
    String::from_utf8_lossy(&decoded[..preview_len]).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_encoding_returns_empty() {
        let layers = detect_encoding_layers("hello world", 5);
        assert!(layers.is_empty());
    }

    #[test]
    fn single_url_encoding() {
        let layers = detect_encoding_layers("hello%20world%21%20test%20value", 5);
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].encoding, "url_encoded");
        assert_eq!(layers[0].depth, 0);
    }

    #[test]
    fn double_url_encoding() {
        // %2520 → decode to %20 → decode to space
        let layers = detect_encoding_layers("hello%2520world%2521%2520test%2520value", 5);
        assert!(
            layers.len() >= 2,
            "expected at least 2 layers, got {}",
            layers.len()
        );
        assert_eq!(layers[0].encoding, "url_encoded");
        assert_eq!(layers[0].depth, 0);
    }

    #[test]
    fn max_depth_respected() {
        let layers = detect_encoding_layers("hello%2520world%2521", 1);
        assert!(layers.len() <= 1, "depth cap should limit to 1 layer");
    }

    #[test]
    fn hard_max_depth_cap() {
        // Even with max_depth=255, hard cap at 5
        let layers = detect_encoding_layers("hello%20world%21%20test%20value", 255);
        assert!(layers.len() <= 5);
    }

    #[test]
    fn empty_input() {
        let layers = detect_encoding_layers("", 5);
        assert!(layers.is_empty());
    }

    #[test]
    fn determinism_10_runs() {
        let input = "hello%2520world%2521%2520test%2520value";
        let mut results: Vec<Vec<String>> = Vec::new();
        for _ in 0..10 {
            let layers = detect_encoding_layers(input, 5);
            let names: Vec<String> = layers.iter().map(|l| l.encoding.to_string()).collect();
            results.push(names);
        }
        for (i, run) in results.iter().enumerate().skip(1) {
            assert_eq!(&results[0], run, "run 0 and run {} differ", i);
        }
    }
}

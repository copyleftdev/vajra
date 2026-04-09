//! Encoding detection domain plugin for Vajra.
//!
//! Detects data encodings in JSON string values: Base64, hex, URL encoding,
//! HTML entities, PEM blocks, data URIs, and more. Includes adversarial
//! detection for double encoding, mixed encoding, and layered obfuscation.

mod decode;
pub mod entropy;
pub mod hints;
pub mod layers;
pub mod recognizers;

use vajra_types::traits::{RelationshipHint, TypeRecognizer, VajraPlugin};

use crate::hints::encoding_hints;
use crate::recognizers::{
    Base64Recognizer, Base64UrlRecognizer, DataUriRecognizer, DoubleEncodedRecognizer,
    HexEncodedRecognizer, HtmlEntityRecognizer, MimeEncodedWordRecognizer, MixedEncodingRecognizer,
    PemBlockRecognizer, PunycodeRecognizer, QuotedPrintableRecognizer, UnicodeEscapeRecognizer,
    UrlEncodedRecognizer,
};

pub use layers::{detect_encoding_layers, EncodingLayer};

/// The encoding detection domain plugin.
///
/// Registers 13 type recognizers across 3 confidence tiers and 3
/// relationship hints for encoding-related field patterns.
pub struct EncodingPlugin;

impl VajraPlugin for EncodingPlugin {
    fn name(&self) -> &str {
        "encoding"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn type_recognizers(&self) -> Vec<Box<dyn TypeRecognizer>> {
        vec![
            // Tier 1: Definite
            Box::new(PemBlockRecognizer),
            Box::new(DataUriRecognizer),
            Box::new(MimeEncodedWordRecognizer),
            Box::new(PunycodeRecognizer),
            // Tier 2: Dominant
            Box::new(UrlEncodedRecognizer),
            Box::new(QuotedPrintableRecognizer),
            Box::new(HtmlEntityRecognizer),
            Box::new(UnicodeEscapeRecognizer),
            Box::new(Base64UrlRecognizer),
            // Tier 3: Heuristic
            Box::new(Base64Recognizer),
            Box::new(HexEncodedRecognizer),
            Box::new(DoubleEncodedRecognizer),
            Box::new(MixedEncodingRecognizer),
        ]
    }

    fn relationship_hints(&self) -> Vec<RelationshipHint> {
        encoding_hints()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vajra_types::traits::VajraPlugin;

    #[test]
    fn plugin_name_and_version() {
        let p = EncodingPlugin;
        assert_eq!(p.name(), "encoding");
        assert_eq!(p.version(), "0.1.0");
    }

    #[test]
    fn plugin_returns_correct_recognizer_count() {
        assert_eq!(EncodingPlugin.type_recognizers().len(), 13);
    }

    #[test]
    fn plugin_returns_correct_hint_count() {
        assert_eq!(EncodingPlugin.relationship_hints().len(), 3);
    }

    #[test]
    fn recognizer_names_are_unique() {
        let recognizers = EncodingPlugin.type_recognizers();
        let mut names: Vec<&str> = recognizers.iter().map(|r| r.type_name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), recognizers.len());
    }

    #[test]
    fn recognizers_have_positive_priority() {
        for r in &EncodingPlugin.type_recognizers() {
            assert!(r.priority() > 0, "{} priority must be > 0", r.type_name());
        }
    }

    #[test]
    fn plugin_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<EncodingPlugin>();
    }

    #[test]
    fn hint_names_match_expected() {
        let hints = EncodingPlugin.relationship_hints();
        let names: Vec<&str> = hints.iter().map(|h| h.name.as_str()).collect();
        assert!(names.contains(&"content_with_encoding"));
        assert!(names.contains(&"transfer_encoding"));
        assert!(names.contains(&"encoded_decoded_pair"));
    }

    #[test]
    fn determinism_recognizers_10_runs() {
        let test_values = [
            "-----BEGIN CERTIFICATE-----\ndata\n-----END CERTIFICATE-----",
            "data:image/png;base64,iVBORw0KGgo=",
            "=?UTF-8?B?SGVsbG8=?=",
            "xn--nxasmq6b",
            "hello%20world%21",
            "&lt;script&gt;",
            "\\u0048\\u0065",
            "SGVsbG8gV29ybGQhIFRoaXMgaXMgYSB0ZXN0IHN0cmluZyBmb3IgYmFzZTY0Lg==",
            "just a normal string",
            "",
        ];
        let mut results: Vec<Vec<Vec<bool>>> = Vec::new();
        for _ in 0..10 {
            let recognizers = EncodingPlugin.type_recognizers();
            let run: Vec<Vec<bool>> = test_values
                .iter()
                .map(|v| recognizers.iter().map(|r| r.matches(v)).collect())
                .collect();
            results.push(run);
        }
        for (i, run) in results.iter().enumerate().skip(1) {
            assert_eq!(&results[0], run, "run 0 and run {} differ", i);
        }
    }

    #[test]
    fn determinism_hints_10_runs() {
        let mut results: Vec<Vec<String>> = Vec::new();
        for _ in 0..10 {
            let hints = EncodingPlugin.relationship_hints();
            let names: Vec<String> = hints.iter().map(|h| h.name.clone()).collect();
            results.push(names);
        }
        for (i, run) in results.iter().enumerate().skip(1) {
            assert_eq!(&results[0], run, "run 0 and run {} differ", i);
        }
    }
}

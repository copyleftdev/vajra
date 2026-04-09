//! Encoding type recognizers.
//!
//! 13 recognizers in 3 tiers organized by confidence level.
//! Each uses OnceLock-compiled regex with trial decode and entropy gating
//! where needed to control false positives.

use std::sync::OnceLock;

use regex::Regex;
use vajra_types::traits::{InferenceConfidence, TypeRecognizer};

use crate::decode;
use crate::entropy::byte_entropy;

fn compile_once<'a>(lock: &'a OnceLock<Option<Regex>>, pattern: &str) -> Option<&'a Regex> {
    lock.get_or_init(|| Regex::new(pattern).ok()).as_ref()
}

// ===========================================================================
// Tier 1: Definite confidence
// ===========================================================================

/// PEM-encoded blocks (certificates, keys, CSRs).
pub struct PemBlockRecognizer;

impl TypeRecognizer for PemBlockRecognizer {
    fn type_name(&self) -> &str {
        "pem_block"
    }
    fn matches(&self, value: &str) -> bool {
        value.starts_with("-----BEGIN ") && value.contains("-----END ")
    }
    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Definite
    }
    fn priority(&self) -> u32 {
        200
    }
}

/// Data URIs (data:mime;base64,...).
pub struct DataUriRecognizer;

static DATA_URI_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for DataUriRecognizer {
    fn type_name(&self) -> &str {
        "data_uri"
    }
    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(
            &DATA_URI_RE,
            r"^data:[a-zA-Z0-9][a-zA-Z0-9!#$&\-^_+.]+/[a-zA-Z0-9][a-zA-Z0-9!#$&\-^_+.]*(?:;[a-zA-Z0-9]+=[\w-]+)*(?:;base64)?,.+$",
        ) else {
            return false;
        };
        re.is_match(value)
    }
    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Definite
    }
    fn priority(&self) -> u32 {
        195
    }
}

/// MIME encoded words (=?charset?B/Q?...?=).
pub struct MimeEncodedWordRecognizer;

static MIME_EW_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for MimeEncodedWordRecognizer {
    fn type_name(&self) -> &str {
        "mime_encoded_word"
    }
    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(&MIME_EW_RE, r"^=\?[^?]+\?[BbQq]\?[^?]+\?=$") else {
            return false;
        };
        re.is_match(value)
    }
    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Definite
    }
    fn priority(&self) -> u32 {
        190
    }
}

/// Punycode internationalized domain labels (xn--...).
pub struct PunycodeRecognizer;

static PUNYCODE_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for PunycodeRecognizer {
    fn type_name(&self) -> &str {
        "punycode"
    }
    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(&PUNYCODE_RE, r"^xn--[a-z0-9-]{1,59}$") else {
            return false;
        };
        re.is_match(value)
    }
    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Definite
    }
    fn priority(&self) -> u32 {
        185
    }
}

// ===========================================================================
// Tier 2: Dominant confidence
// ===========================================================================

/// URL/percent-encoded strings with 2+ %XX sequences.
pub struct UrlEncodedRecognizer;

static URL_ENC_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for UrlEncodedRecognizer {
    fn type_name(&self) -> &str {
        "url_encoded"
    }
    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(&URL_ENC_RE, r"%[0-9A-Fa-f]{2}") else {
            return false;
        };
        // Need at least 2 percent-encoded sequences
        let count = re.find_iter(value).take(2).count();
        if count < 2 {
            return false;
        }
        // Trial decode must succeed
        decode::try_percent_decode(value).is_some()
    }
    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Dominant
    }
    fn priority(&self) -> u32 {
        170
    }
}

/// Quoted-printable encoding (3+ =XX sequences).
pub struct QuotedPrintableRecognizer;

static QP_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for QuotedPrintableRecognizer {
    fn type_name(&self) -> &str {
        "quoted_printable"
    }
    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(&QP_RE, r"=[0-9A-F]{2}") else {
            return false;
        };
        re.find_iter(value).take(3).count() >= 3
    }
    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Dominant
    }
    fn priority(&self) -> u32 {
        165
    }
}

/// HTML entity references (2+ entities: &amp; &#xNN; &#NNN;).
pub struct HtmlEntityRecognizer;

static HTML_ENT_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for HtmlEntityRecognizer {
    fn type_name(&self) -> &str {
        "html_entity"
    }
    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(&HTML_ENT_RE, r"&(?:#x?[0-9a-fA-F]+|[a-zA-Z]+);") else {
            return false;
        };
        re.find_iter(value).take(2).count() >= 2
    }
    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Dominant
    }
    fn priority(&self) -> u32 {
        160
    }
}

/// Unicode escape sequences (2+ \uXXXX or \xNN).
pub struct UnicodeEscapeRecognizer;

static UNICODE_ESC_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for UnicodeEscapeRecognizer {
    fn type_name(&self) -> &str {
        "unicode_escape"
    }
    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(&UNICODE_ESC_RE, r"\\u[0-9a-fA-F]{4}|\\x[0-9a-fA-F]{2}") else {
            return false;
        };
        re.find_iter(value).take(2).count() >= 2
    }
    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Dominant
    }
    fn priority(&self) -> u32 {
        155
    }
}

/// URL-safe Base64 (16+ chars, -, _ alphabet).
pub struct Base64UrlRecognizer;

static B64URL_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for Base64UrlRecognizer {
    fn type_name(&self) -> &str {
        "base64url"
    }
    fn matches(&self, value: &str) -> bool {
        // Exclude JWTs (three dot-separated eyJ segments)
        if value.starts_with("eyJ") && value.matches('.').count() == 2 {
            return false;
        }
        // Exclude UUIDs (8-4-4-4-12 hex with hyphens)
        if value.len() == 36 && value.chars().filter(|&c| c == '-').count() == 4 {
            return false;
        }
        let Some(re) = compile_once(&B64URL_RE, r"^[A-Za-z0-9_-]{16,}={0,2}$") else {
            return false;
        };
        if !re.is_match(value) {
            return false;
        }
        // Must contain at least one URL-safe character to distinguish from standard base64
        if !value.contains('-') && !value.contains('_') {
            return false;
        }
        decode::try_base64_decode(value).is_some()
    }
    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Dominant
    }
    fn priority(&self) -> u32 {
        150
    }
}

// ===========================================================================
// Tier 3: Heuristic confidence
// ===========================================================================

/// Standard Base64 (24+ chars, heavy gating to control false positives).
pub struct Base64Recognizer;

static B64_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for Base64Recognizer {
    fn type_name(&self) -> &str {
        "base64"
    }
    fn matches(&self, value: &str) -> bool {
        // Gate 1: length >= 24
        if value.len() < 24 {
            return false;
        }
        // Gate 2: length divisible by 4
        if value.len() % 4 != 0 {
            return false;
        }
        // Gate 3: regex (standard alphabet only, no URL-safe chars)
        let Some(re) = compile_once(&B64_RE, r"^[A-Za-z0-9+/]+=*$") else {
            return false;
        };
        if !re.is_match(value) {
            return false;
        }
        // Exclude JWTs
        if value.starts_with("eyJ") {
            return false;
        }
        // Exclude PEM body content (typically appears with -----BEGIN)
        // Can't check context here, but very long pure base64 is fine to flag
        // Gate 4: trial decode
        let Some(decoded) = decode::try_base64_decode(value) else {
            return false;
        };
        // Gate 5: entropy gating
        let encoded_entropy = byte_entropy(value.as_bytes());
        let decoded_entropy = byte_entropy(&decoded);
        // Accept if: encoded looks like real base64 (high entropy)
        // OR decoded is readable text (low entropy)
        encoded_entropy >= 4.5 || decoded_entropy < 4.0
    }
    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Heuristic
    }
    fn priority(&self) -> u32 {
        140
    }
}

/// Hex-encoded data (32+ chars, excludes known hash lengths).
pub struct HexEncodedRecognizer;

static HEX_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for HexEncodedRecognizer {
    fn type_name(&self) -> &str {
        "hex_encoded"
    }
    fn matches(&self, value: &str) -> bool {
        let len = value.len();
        // Gate 1: >= 32 chars, even length
        if len < 32 || len % 2 != 0 {
            return false;
        }
        // Gate 2: exclude known hash lengths (handled by vajra-domain-sec)
        if matches!(len, 32 | 40 | 64 | 128) {
            return false;
        }
        // Gate 3: regex
        let Some(re) = compile_once(&HEX_RE, r"^[0-9a-fA-F]+$") else {
            return false;
        };
        if !re.is_match(value) {
            return false;
        }
        // Gate 4: entropy > 3.0 (not all zeros or repetitive)
        byte_entropy(value.as_bytes()) > 3.0
    }
    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Heuristic
    }
    fn priority(&self) -> u32 {
        135
    }
}

/// Double-encoded values (decode reveals another encoded value).
pub struct DoubleEncodedRecognizer;

impl TypeRecognizer for DoubleEncodedRecognizer {
    fn type_name(&self) -> &str {
        "double_encoded"
    }
    fn matches(&self, value: &str) -> bool {
        // Try URL decoding first (most common double-encoding vector)
        if let Some(decoded) = decode::try_percent_decode(value) {
            let decoded_str = String::from_utf8_lossy(&decoded);
            // Check if decoded result contains more percent-encoding
            if decoded_str.contains('%') && decoded_str.matches('%').count() >= 2 {
                return true;
            }
        }
        // Try base64 decoding
        if value.len() >= 24 && value.len() % 4 == 0 {
            if let Some(decoded) = decode::try_base64_decode(value) {
                let decoded_str = String::from_utf8_lossy(&decoded);
                // Check if decoded result looks base64-encoded itself
                if decoded_str.len() >= 24
                    && decoded_str.len() % 4 == 0
                    && decode::try_base64_decode(&decoded_str).is_some()
                {
                    return true;
                }
            }
        }
        false
    }
    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Heuristic
    }
    fn priority(&self) -> u32 {
        130
    }
}

/// Mixed encoding (2+ distinct encoding patterns in one value).
pub struct MixedEncodingRecognizer;

static MIX_URL_RE: OnceLock<Option<Regex>> = OnceLock::new();
static MIX_HTML_RE: OnceLock<Option<Regex>> = OnceLock::new();
static MIX_UNICODE_RE: OnceLock<Option<Regex>> = OnceLock::new();
static MIX_B64_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for MixedEncodingRecognizer {
    fn type_name(&self) -> &str {
        "mixed_encoding"
    }
    fn matches(&self, value: &str) -> bool {
        let mut types_found = 0u32;

        if let Some(re) = compile_once(&MIX_URL_RE, r"%[0-9A-Fa-f]{2}") {
            if re.is_match(value) {
                types_found += 1;
            }
        }
        if let Some(re) = compile_once(&MIX_HTML_RE, r"&(?:#x?[0-9a-fA-F]+|[a-zA-Z]+);") {
            if re.is_match(value) {
                types_found += 1;
            }
        }
        if let Some(re) = compile_once(&MIX_UNICODE_RE, r"\\u[0-9a-fA-F]{4}|\\x[0-9a-fA-F]{2}") {
            if re.is_match(value) {
                types_found += 1;
            }
        }
        if let Some(re) = compile_once(&MIX_B64_RE, r"(?:;base64,|[A-Za-z0-9+/]{20,}={1,2})") {
            if re.is_match(value) {
                types_found += 1;
            }
        }

        types_found >= 2
    }
    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Heuristic
    }
    fn priority(&self) -> u32 {
        125
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Tier 1: Definite ----

    #[test]
    fn pem_valid() {
        let r = PemBlockRecognizer;
        assert!(r.matches("-----BEGIN CERTIFICATE-----\nMIIBxTCCAW...\n-----END CERTIFICATE-----"));
        assert!(r.matches("-----BEGIN RSA PRIVATE KEY-----\ndata\n-----END RSA PRIVATE KEY-----"));
    }

    #[test]
    fn pem_invalid() {
        let r = PemBlockRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("-----BEGIN CERTIFICATE-----")); // no END
        assert!(!r.matches("just a string"));
    }

    #[test]
    fn data_uri_valid() {
        let r = DataUriRecognizer;
        assert!(r.matches("data:image/png;base64,iVBORw0KGgo="));
        assert!(r.matches("data:text/plain,hello%20world"));
        assert!(r.matches("data:application/json;charset=utf-8,{\"a\":1}"));
    }

    #[test]
    fn data_uri_invalid() {
        let r = DataUriRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("data:"));
        assert!(!r.matches("http://example.com"));
    }

    #[test]
    fn mime_encoded_word_valid() {
        let r = MimeEncodedWordRecognizer;
        assert!(r.matches("=?UTF-8?B?SGVsbG8=?="));
        assert!(r.matches("=?ISO-8859-1?Q?=E9t=E9?="));
    }

    #[test]
    fn mime_encoded_word_invalid() {
        let r = MimeEncodedWordRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("=?UTF-8?X?bad?=")); // invalid encoding type
    }

    #[test]
    fn punycode_valid() {
        let r = PunycodeRecognizer;
        assert!(r.matches("xn--nxasmq6b")); // example punycode
        assert!(r.matches("xn--80akhbyknj4f")); // Russian domain
    }

    #[test]
    fn punycode_invalid() {
        let r = PunycodeRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("xn-")); // too short prefix
        assert!(!r.matches("example.com"));
    }

    // ---- Tier 2: Dominant ----

    #[test]
    fn url_encoded_valid() {
        let r = UrlEncodedRecognizer;
        assert!(r.matches("hello%20world%21"));
        assert!(r.matches("%3Cscript%3Ealert%281%29%3C%2Fscript%3E"));
    }

    #[test]
    fn url_encoded_invalid() {
        let r = UrlEncodedRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("hello world")); // no percent
        assert!(!r.matches("hello%20world")); // only 1 sequence
    }

    #[test]
    fn html_entity_valid() {
        let r = HtmlEntityRecognizer;
        assert!(r.matches("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(r.matches("&#x3C;b&#x3E;bold&#x3C;/b&#x3E;"));
    }

    #[test]
    fn html_entity_invalid() {
        let r = HtmlEntityRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("no entities here"));
        assert!(!r.matches("one &amp; only")); // only 1 entity
    }

    #[test]
    fn unicode_escape_valid() {
        let r = UnicodeEscapeRecognizer;
        assert!(r.matches("\\u0048\\u0065\\u006C\\u006C\\u006F"));
        assert!(r.matches("\\x48\\x65\\x6C\\x6C\\x6F"));
    }

    #[test]
    fn unicode_escape_invalid() {
        let r = UnicodeEscapeRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("hello"));
        assert!(!r.matches("\\u0048")); // only 1 escape
    }

    #[test]
    fn base64url_valid() {
        let r = Base64UrlRecognizer;
        assert!(r.matches("SGVsbG8tV29ybGQtdGVzdA_-")); // has - and _
    }

    #[test]
    fn base64url_rejects_jwt() {
        let r = Base64UrlRecognizer;
        assert!(!r.matches("eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U"));
    }

    // ---- Tier 3: Heuristic ----

    #[test]
    fn base64_valid() {
        let r = Base64Recognizer;
        // "Hello World! This is a test string for base64." base64-encoded
        assert!(r.matches("SGVsbG8gV29ybGQhIFRoaXMgaXMgYSB0ZXN0IHN0cmluZyBmb3IgYmFzZTY0Lg=="));
    }

    #[test]
    fn base64_rejects_short() {
        let r = Base64Recognizer;
        assert!(!r.matches("SGVsbG8=")); // too short (<24)
    }

    #[test]
    fn base64_rejects_jwt() {
        let r = Base64Recognizer;
        assert!(!r.matches("eyJhbGciOiJIUzI1NiJ9")); // starts with eyJ
    }

    #[test]
    fn hex_encoded_valid() {
        let r = HexEncodedRecognizer;
        // 48 hex chars (24 bytes) — not a known hash length
        // 48 hex chars with high entropy (diverse byte values)
        assert!(r.matches("48656c6c6f20576f726c6421a3b4c5d6e7f80912233445a6"));
    }

    #[test]
    fn hex_rejects_known_hash_lengths() {
        let r = HexEncodedRecognizer;
        // 64 chars = SHA-256 length
        assert!(!r.matches("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"));
        // 40 chars = SHA-1 length
        assert!(!r.matches("da39a3ee5e6b4b0d3255bfef95601890afd80709"));
        // 32 chars = MD5 length
        assert!(!r.matches("d41d8cd98f00b204e9800998ecf8427e"));
    }

    #[test]
    fn double_encoded_url() {
        let r = DoubleEncodedRecognizer;
        // %253C = double-encoded < (%25 decodes to %, then %3C decodes to <)
        assert!(r.matches("%253Cscript%253Ealert%25281%2529%253C%252Fscript%253E"));
    }

    #[test]
    fn mixed_encoding_detected() {
        let r = MixedEncodingRecognizer;
        // URL encoding + HTML entities in same string
        assert!(r.matches("hello%20&amp;%20world&lt;"));
    }

    // ---- Cross-recognizer ----

    #[test]
    fn all_recognizers_reject_empty() {
        let recognizers: Vec<Box<dyn TypeRecognizer>> = vec![
            Box::new(PemBlockRecognizer),
            Box::new(DataUriRecognizer),
            Box::new(MimeEncodedWordRecognizer),
            Box::new(PunycodeRecognizer),
            Box::new(UrlEncodedRecognizer),
            Box::new(QuotedPrintableRecognizer),
            Box::new(HtmlEntityRecognizer),
            Box::new(UnicodeEscapeRecognizer),
            Box::new(Base64UrlRecognizer),
            Box::new(Base64Recognizer),
            Box::new(HexEncodedRecognizer),
            Box::new(DoubleEncodedRecognizer),
            Box::new(MixedEncodingRecognizer),
        ];
        for r in &recognizers {
            assert!(!r.matches(""), "{} should not match empty", r.type_name());
        }
    }

    #[test]
    fn all_recognizers_reject_whitespace() {
        let recognizers: Vec<Box<dyn TypeRecognizer>> = vec![
            Box::new(PemBlockRecognizer),
            Box::new(DataUriRecognizer),
            Box::new(MimeEncodedWordRecognizer),
            Box::new(PunycodeRecognizer),
            Box::new(UrlEncodedRecognizer),
            Box::new(QuotedPrintableRecognizer),
            Box::new(HtmlEntityRecognizer),
            Box::new(UnicodeEscapeRecognizer),
            Box::new(Base64UrlRecognizer),
            Box::new(Base64Recognizer),
            Box::new(HexEncodedRecognizer),
            Box::new(DoubleEncodedRecognizer),
            Box::new(MixedEncodingRecognizer),
        ];
        for r in &recognizers {
            assert!(
                !r.matches("   "),
                "{} should not match spaces",
                r.type_name()
            );
            assert!(!r.matches("\t"), "{} should not match tab", r.type_name());
            assert!(
                !r.matches("\n"),
                "{} should not match newline",
                r.type_name()
            );
        }
    }

    #[test]
    fn false_positive_gauntlet() {
        // Common values that should NOT trigger encoding recognizers
        let normals = [
            "hello world",
            "John Smith",
            "2025-04-07T14:30:00Z",
            "192.168.1.1",
            "user@example.com",
            "/api/v1/users/12345",
            "application/json",
            "true",
            "false",
            "null",
            "1234567890",
            "USD",
            "enabled",
            "active",
            "pending",
            "The quick brown fox jumps over the lazy dog",
            "https://example.com/page?q=search&lang=en",
            "550e8400-e29b-41d4-a716-446655440000", // UUID
            "2025-04-07",
            "Mon, 07 Apr 2025 14:30:00 GMT",
        ];

        let recognizers: Vec<Box<dyn TypeRecognizer>> = vec![
            Box::new(Base64Recognizer),
            Box::new(HexEncodedRecognizer),
            Box::new(DoubleEncodedRecognizer),
        ];

        for value in &normals {
            for r in &recognizers {
                assert!(
                    !r.matches(value),
                    "false positive: {} matched '{}'",
                    r.type_name(),
                    value
                );
            }
        }
    }
}

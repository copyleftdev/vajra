//! Security domain type recognizers.
//!
//! Each recognizer identifies a specific security-relevant data type
//! (CVE IDs, IP addresses, hashes, tokens, MITRE ATT&CK IDs) by matching
//! string values against compiled regular expressions. Regexes are compiled
//! once using `OnceLock`.

use std::sync::OnceLock;

use regex::Regex;
use vajra_types::traits::{InferenceConfidence, TypeRecognizer};

/// Helper to compile a regex once and return a reference.
/// Returns `None` if the regex is invalid (should not happen with known patterns).
fn compile_once<'a>(lock: &'a OnceLock<Option<Regex>>, pattern: &str) -> Option<&'a Regex> {
    lock.get_or_init(|| Regex::new(pattern).ok()).as_ref()
}

// ---------------------------------------------------------------------------
// CVE Recognizer
// ---------------------------------------------------------------------------

/// Recognizes CVE identifiers: CVE-YYYY-NNNNN where year is 4 digits
/// and ID is 4 or more digits (e.g., CVE-2024-12345, CVE-2023-0001).
pub struct CveRecognizer;

static CVE_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for CveRecognizer {
    fn type_name(&self) -> &str {
        "cve_id"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(&CVE_RE, r"^CVE-\d{4}-\d{4,}$") else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Definite
    }

    fn priority(&self) -> u32 {
        120
    }
}

// ---------------------------------------------------------------------------
// IPv4 Recognizer
// ---------------------------------------------------------------------------

/// Recognizes IPv4 addresses in dotted-quad notation (e.g., 192.168.1.1).
/// Validates each octet is in range 0-255.
pub struct Ipv4Recognizer;

static IPV4_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for Ipv4Recognizer {
    fn type_name(&self) -> &str {
        "ipv4"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(
            &IPV4_RE,
            r"^(?:(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.){3}(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)$",
        ) else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Dominant
    }

    fn priority(&self) -> u32 {
        100
    }
}

// ---------------------------------------------------------------------------
// IPv6 Recognizer
// ---------------------------------------------------------------------------

/// Recognizes IPv6 addresses in full, compressed, or mixed notation.
/// Uses a simplified pattern that covers common formats.
pub struct Ipv6Recognizer;

static IPV6_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for Ipv6Recognizer {
    fn type_name(&self) -> &str {
        "ipv6"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(
            &IPV6_RE,
            r"^(?:[0-9a-fA-F]{1,4}:){7}[0-9a-fA-F]{1,4}$|^(?:[0-9a-fA-F]{1,4}:){1,7}:$|^(?:[0-9a-fA-F]{1,4}:){1,6}:[0-9a-fA-F]{1,4}$|^(?:[0-9a-fA-F]{1,4}:){1,5}(?::[0-9a-fA-F]{1,4}){1,2}$|^(?:[0-9a-fA-F]{1,4}:){1,4}(?::[0-9a-fA-F]{1,4}){1,3}$|^(?:[0-9a-fA-F]{1,4}:){1,3}(?::[0-9a-fA-F]{1,4}){1,4}$|^(?:[0-9a-fA-F]{1,4}:){1,2}(?::[0-9a-fA-F]{1,4}){1,5}$|^[0-9a-fA-F]{1,4}:(?::[0-9a-fA-F]{1,4}){1,6}$|^::(?:[0-9a-fA-F]{1,4}:){0,5}[0-9a-fA-F]{1,4}$|^::$",
        ) else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Dominant
    }

    fn priority(&self) -> u32 {
        95
    }
}

// ---------------------------------------------------------------------------
// CIDR Recognizer
// ---------------------------------------------------------------------------

/// Recognizes CIDR notation: an IPv4 address followed by /prefix (0-32).
pub struct CidrRecognizer;

static CIDR_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for CidrRecognizer {
    fn type_name(&self) -> &str {
        "cidr"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(
            &CIDR_RE,
            r"^(?:(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.){3}(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)/(?:[0-9]|[12][0-9]|3[0-2])$",
        ) else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Definite
    }

    fn priority(&self) -> u32 {
        110
    }
}

// ---------------------------------------------------------------------------
// MAC Address Recognizer
// ---------------------------------------------------------------------------

/// Recognizes MAC addresses in colon-separated or hyphen-separated format
/// (e.g., AA:BB:CC:DD:EE:FF or AA-BB-CC-DD-EE-FF).
pub struct MacAddressRecognizer;

static MAC_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for MacAddressRecognizer {
    fn type_name(&self) -> &str {
        "mac_address"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(
            &MAC_RE,
            r"^(?i)[0-9a-f]{2}(?::[0-9a-f]{2}){5}$|^(?i)[0-9a-f]{2}(?:-[0-9a-f]{2}){5}$",
        ) else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Dominant
    }

    fn priority(&self) -> u32 {
        90
    }
}

// ---------------------------------------------------------------------------
// SHA-256 Hash Recognizer
// ---------------------------------------------------------------------------

/// Recognizes SHA-256 hashes: exactly 64 lowercase hex characters.
pub struct Sha256Recognizer;

static SHA256_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for Sha256Recognizer {
    fn type_name(&self) -> &str {
        "sha256_hash"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(&SHA256_RE, r"^[0-9a-f]{64}$") else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Dominant
    }

    fn priority(&self) -> u32 {
        85
    }
}

// ---------------------------------------------------------------------------
// SHA-1 Hash Recognizer
// ---------------------------------------------------------------------------

/// Recognizes SHA-1 hashes: exactly 40 lowercase hex characters.
pub struct Sha1Recognizer;

static SHA1_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for Sha1Recognizer {
    fn type_name(&self) -> &str {
        "sha1_hash"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(&SHA1_RE, r"^[0-9a-f]{40}$") else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Dominant
    }

    fn priority(&self) -> u32 {
        80
    }
}

// ---------------------------------------------------------------------------
// MD5 Hash Recognizer
// ---------------------------------------------------------------------------

/// Recognizes MD5 hashes: exactly 32 lowercase hex characters.
/// Heuristic confidence since 32 hex chars can be other identifiers.
pub struct Md5Recognizer;

static MD5_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for Md5Recognizer {
    fn type_name(&self) -> &str {
        "md5_hash"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(&MD5_RE, r"^[0-9a-f]{32}$") else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Heuristic
    }

    fn priority(&self) -> u32 {
        75
    }
}

// ---------------------------------------------------------------------------
// JWT Recognizer
// ---------------------------------------------------------------------------

/// Recognizes JSON Web Tokens: three base64url-encoded segments separated by dots,
/// where the first two segments start with "eyJ" (base64url for '{"').
pub struct JwtRecognizer;

static JWT_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for JwtRecognizer {
    fn type_name(&self) -> &str {
        "jwt_token"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(
            &JWT_RE,
            r"^eyJ[A-Za-z0-9_-]*\.eyJ[A-Za-z0-9_-]*\.[A-Za-z0-9_-]+$",
        ) else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Definite
    }

    fn priority(&self) -> u32 {
        115
    }
}

// ---------------------------------------------------------------------------
// MITRE ATT&CK Technique Recognizer
// ---------------------------------------------------------------------------

/// Recognizes MITRE ATT&CK technique IDs: T followed by 4 digits,
/// optionally followed by a subtechnique (.NNN).
/// Examples: T1059, T1059.001, T1566.002.
pub struct MitreAttackRecognizer;

static MITRE_ATTACK_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for MitreAttackRecognizer {
    fn type_name(&self) -> &str {
        "mitre_attack_id"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(&MITRE_ATTACK_RE, r"^T\d{4}(\.\d{3})?$") else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Definite
    }

    fn priority(&self) -> u32 {
        120
    }
}

// ---------------------------------------------------------------------------
// MITRE ATT&CK Tactic Recognizer
// ---------------------------------------------------------------------------

/// Recognizes MITRE ATT&CK tactic IDs: TA followed by 4 digits.
/// Examples: TA0001 (Initial Access), TA0040 (Impact).
pub struct MitreTacticRecognizer;

static MITRE_TACTIC_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for MitreTacticRecognizer {
    fn type_name(&self) -> &str {
        "mitre_tactic"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(&MITRE_TACTIC_RE, r"^TA\d{4}$") else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Definite
    }

    fn priority(&self) -> u32 {
        118
    }
}

// ---------------------------------------------------------------------------
// CVSS Vector String Recognizer
// ---------------------------------------------------------------------------

/// Recognizes CVSS v3.x vector strings.
/// Example: CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H
pub struct CvssRecognizer;

static CVSS_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for CvssRecognizer {
    fn type_name(&self) -> &str {
        "cvss_vector"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(
            &CVSS_RE,
            r"^CVSS:3\.[01]/AV:[NALP]/AC:[LH]/PR:[NLH]/UI:[NR]/S:[UC]/C:[NLH]/I:[NLH]/A:[NLH]$",
        ) else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Definite
    }

    fn priority(&self) -> u32 {
        115
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // CVE tests
    // -----------------------------------------------------------------------

    #[test]
    fn cve_valid_codes() {
        let r = CveRecognizer;
        assert!(r.matches("CVE-2024-12345"));
        assert!(r.matches("CVE-2023-0001"));
        assert!(r.matches("CVE-1999-99999")); // 5-digit ID
        assert!(r.matches("CVE-2024-1234")); // 4-digit ID
    }

    #[test]
    fn cve_invalid_codes() {
        let r = CveRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("CVE-2024-123")); // 3-digit ID (too short)
        assert!(!r.matches("cve-2024-12345")); // lowercase
        assert!(!r.matches("CVE2024-12345")); // missing hyphen
        assert!(!r.matches("CVE-24-12345")); // 2-digit year
        assert!(!r.matches("CVE-2024-")); // no ID
        assert!(!r.matches("CVE-2024")); // no ID at all
    }

    #[test]
    fn cve_metadata() {
        let r = CveRecognizer;
        assert_eq!(r.type_name(), "cve_id");
        assert_eq!(r.confidence(), InferenceConfidence::Definite);
        assert_eq!(r.priority(), 120);
    }

    // -----------------------------------------------------------------------
    // IPv4 tests
    // -----------------------------------------------------------------------

    #[test]
    fn ipv4_valid() {
        let r = Ipv4Recognizer;
        assert!(r.matches("192.168.1.1"));
        assert!(r.matches("10.0.0.0"));
        assert!(r.matches("255.255.255.255"));
        assert!(r.matches("0.0.0.0"));
        assert!(r.matches("127.0.0.1"));
        assert!(r.matches("1.2.3.4"));
    }

    #[test]
    fn ipv4_invalid() {
        let r = Ipv4Recognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("256.0.0.1")); // octet > 255
        assert!(!r.matches("192.168.1")); // only 3 octets
        assert!(!r.matches("192.168.1.1.1")); // 5 octets
        assert!(!r.matches("abc.def.ghi.jkl")); // non-numeric
        assert!(!r.matches("192.168.1.1/24")); // CIDR, not plain IP
        assert!(!r.matches(" 192.168.1.1")); // leading space
    }

    #[test]
    fn ipv4_metadata() {
        let r = Ipv4Recognizer;
        assert_eq!(r.type_name(), "ipv4");
        assert_eq!(r.confidence(), InferenceConfidence::Dominant);
        assert_eq!(r.priority(), 100);
    }

    // -----------------------------------------------------------------------
    // IPv6 tests
    // -----------------------------------------------------------------------

    #[test]
    fn ipv6_valid() {
        let r = Ipv6Recognizer;
        assert!(r.matches("2001:0db8:85a3:0000:0000:8a2e:0370:7334")); // full
        assert!(r.matches("::1")); // loopback
        assert!(r.matches("::")); // all zeros
        assert!(r.matches("fe80::")); // link-local prefix
        assert!(r.matches("2001:db8::1")); // compressed
    }

    #[test]
    fn ipv6_invalid() {
        let r = Ipv6Recognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("192.168.1.1")); // IPv4
        assert!(!r.matches("not-an-ip"));
        assert!(!r.matches("2001:db8::1::2")); // double ::
    }

    #[test]
    fn ipv6_metadata() {
        let r = Ipv6Recognizer;
        assert_eq!(r.type_name(), "ipv6");
        assert_eq!(r.confidence(), InferenceConfidence::Dominant);
        assert_eq!(r.priority(), 95);
    }

    // -----------------------------------------------------------------------
    // CIDR tests
    // -----------------------------------------------------------------------

    #[test]
    fn cidr_valid() {
        let r = CidrRecognizer;
        assert!(r.matches("10.0.0.0/8"));
        assert!(r.matches("192.168.1.0/24"));
        assert!(r.matches("0.0.0.0/0"));
        assert!(r.matches("255.255.255.255/32"));
        assert!(r.matches("172.16.0.0/12"));
    }

    #[test]
    fn cidr_invalid() {
        let r = CidrRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("192.168.1.1")); // no prefix
        assert!(!r.matches("192.168.1.0/33")); // prefix > 32
        assert!(!r.matches("256.0.0.0/8")); // invalid octet
        assert!(!r.matches("10.0.0.0/")); // no prefix number
    }

    #[test]
    fn cidr_metadata() {
        let r = CidrRecognizer;
        assert_eq!(r.type_name(), "cidr");
        assert_eq!(r.confidence(), InferenceConfidence::Definite);
        assert_eq!(r.priority(), 110);
    }

    // -----------------------------------------------------------------------
    // MAC Address tests
    // -----------------------------------------------------------------------

    #[test]
    fn mac_valid() {
        let r = MacAddressRecognizer;
        assert!(r.matches("aa:bb:cc:dd:ee:ff")); // lowercase colon
        assert!(r.matches("AA:BB:CC:DD:EE:FF")); // uppercase colon
        assert!(r.matches("00:11:22:33:44:55"));
        assert!(r.matches("aa-bb-cc-dd-ee-ff")); // hyphen-separated
        assert!(r.matches("AA-BB-CC-DD-EE-FF")); // uppercase hyphen
    }

    #[test]
    fn mac_invalid() {
        let r = MacAddressRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("aa:bb:cc:dd:ee")); // only 5 octets
        assert!(!r.matches("aa:bb:cc:dd:ee:ff:00")); // 7 octets
        assert!(!r.matches("aabbccddeeff")); // no separator
        assert!(!r.matches("gg:hh:ii:jj:kk:ll")); // non-hex
        assert!(!r.matches("aa:bb:cc-dd:ee:ff")); // mixed separators
    }

    #[test]
    fn mac_metadata() {
        let r = MacAddressRecognizer;
        assert_eq!(r.type_name(), "mac_address");
        assert_eq!(r.confidence(), InferenceConfidence::Dominant);
        assert_eq!(r.priority(), 90);
    }

    // -----------------------------------------------------------------------
    // SHA-256 tests
    // -----------------------------------------------------------------------

    #[test]
    fn sha256_valid() {
        let r = Sha256Recognizer;
        assert!(r.matches("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")); // sha256 of ""
        assert!(r.matches("a665a45920422f9d417e4867efdc4fb8a04a1f3fff1fa07e998e86f7f7a27ae3"));
    }

    #[test]
    fn sha256_invalid() {
        let r = Sha256Recognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("e3b0c44298fc1c149afbf4c8996fb924")); // too short (32 chars = MD5)
        assert!(!r.matches("E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855")); // uppercase
        assert!(!r.matches("g3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"));
        // non-hex char
    }

    #[test]
    fn sha256_metadata() {
        let r = Sha256Recognizer;
        assert_eq!(r.type_name(), "sha256_hash");
        assert_eq!(r.confidence(), InferenceConfidence::Dominant);
        assert_eq!(r.priority(), 85);
    }

    // -----------------------------------------------------------------------
    // SHA-1 tests
    // -----------------------------------------------------------------------

    #[test]
    fn sha1_valid() {
        let r = Sha1Recognizer;
        assert!(r.matches("da39a3ee5e6b4b0d3255bfef95601890afd80709")); // sha1 of ""
        assert!(r.matches("a94a8fe5ccb19ba61c4c0873d391e987982fbbd3"));
    }

    #[test]
    fn sha1_invalid() {
        let r = Sha1Recognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("da39a3ee5e6b4b0d3255bfef9560189")); // 31 chars
        assert!(!r.matches("da39a3ee5e6b4b0d3255bfef95601890afd807091")); // 41 chars
        assert!(!r.matches("DA39A3EE5E6B4B0D3255BFEF95601890AFD80709")); // uppercase
    }

    #[test]
    fn sha1_metadata() {
        let r = Sha1Recognizer;
        assert_eq!(r.type_name(), "sha1_hash");
        assert_eq!(r.confidence(), InferenceConfidence::Dominant);
        assert_eq!(r.priority(), 80);
    }

    // -----------------------------------------------------------------------
    // MD5 tests
    // -----------------------------------------------------------------------

    #[test]
    fn md5_valid() {
        let r = Md5Recognizer;
        assert!(r.matches("d41d8cd98f00b204e9800998ecf8427e")); // md5 of ""
        assert!(r.matches("098f6bcd4621d373cade4e832627b4f6"));
    }

    #[test]
    fn md5_invalid() {
        let r = Md5Recognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("d41d8cd98f00b204e9800998ecf8427")); // 31 chars
        assert!(!r.matches("d41d8cd98f00b204e9800998ecf8427e0")); // 33 chars
        assert!(!r.matches("D41D8CD98F00B204E9800998ECF8427E")); // uppercase
    }

    #[test]
    fn md5_metadata() {
        let r = Md5Recognizer;
        assert_eq!(r.type_name(), "md5_hash");
        assert_eq!(r.confidence(), InferenceConfidence::Heuristic);
        assert_eq!(r.priority(), 75);
    }

    // -----------------------------------------------------------------------
    // JWT tests
    // -----------------------------------------------------------------------

    #[test]
    fn jwt_valid() {
        let r = JwtRecognizer;
        assert!(r.matches(
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U"
        ));
        assert!(r.matches(
            "eyJhbGciOiJSUzI1NiJ9.eyJpc3MiOiJqb2UiLA0KICJleHAiOjEzMDA4MTkzODAsDQogImh0dHA6Ly9leGFtcGxlLmNvbS9pc19yb290Ijp0cnVlfQ.cC4hiUPoj9Eetdgtv3hF80EGrhuB__dzERat0XF9g2VtQgr9PJbu3XOiZj5RZmh7AAuHIm4Bh-0Qc_lF5YKt_O8W2Fp5jujGbds9uJdbF9CUAr7t1dnZcAcQjbKBYNX4BAynRFdiuB--f_nZLgrnbyTyWzO75vRK5h6xBArLIARNPvkSjtQBMHlb1L07Qe7K0GarZRmB_eSN9383LcOLn6_dO--xi12jzDwusC-eOkHWEsqtFZESc6BfI7noOPqvhJ1phCnvWh6IeYI2w9QOYEUipUTI8np6LbgGY9Fs98rqVt5AXLIhWkWywlVmtVrBp0igcN_IoypGlUPQGe77Rw"
        ));
    }

    #[test]
    fn jwt_invalid() {
        let r = JwtRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("eyJ.eyJ")); // only 2 segments
        assert!(!r.matches("not.a.jwt")); // doesn't start with eyJ
        assert!(!r.matches("eyJhbGciOiJIUzI1NiJ9")); // only 1 segment
        assert!(!r.matches("eyJhbGciOiJIUzI1NiJ9.notbase64.sig")); // 2nd segment not eyJ
    }

    #[test]
    fn jwt_metadata() {
        let r = JwtRecognizer;
        assert_eq!(r.type_name(), "jwt_token");
        assert_eq!(r.confidence(), InferenceConfidence::Definite);
        assert_eq!(r.priority(), 115);
    }

    // -----------------------------------------------------------------------
    // MITRE ATT&CK Technique tests
    // -----------------------------------------------------------------------

    #[test]
    fn mitre_attack_valid() {
        let r = MitreAttackRecognizer;
        assert!(r.matches("T1059")); // technique
        assert!(r.matches("T1059.001")); // subtechnique
        assert!(r.matches("T1566.002"));
        assert!(r.matches("T1190"));
        assert!(r.matches("T1078.003"));
    }

    #[test]
    fn mitre_attack_invalid() {
        let r = MitreAttackRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("T105")); // too few digits
        assert!(!r.matches("T10590")); // too many digits (5)
        assert!(!r.matches("TA0001")); // that's a tactic
        assert!(!r.matches("t1059")); // lowercase
        assert!(!r.matches("T1059.01")); // subtechnique only 2 digits
        assert!(!r.matches("T1059.0001")); // subtechnique 4 digits
    }

    #[test]
    fn mitre_attack_metadata() {
        let r = MitreAttackRecognizer;
        assert_eq!(r.type_name(), "mitre_attack_id");
        assert_eq!(r.confidence(), InferenceConfidence::Definite);
        assert_eq!(r.priority(), 120);
    }

    // -----------------------------------------------------------------------
    // MITRE ATT&CK Tactic tests
    // -----------------------------------------------------------------------

    #[test]
    fn mitre_tactic_valid() {
        let r = MitreTacticRecognizer;
        assert!(r.matches("TA0001")); // Initial Access
        assert!(r.matches("TA0040")); // Impact
        assert!(r.matches("TA0009")); // Collection
    }

    #[test]
    fn mitre_tactic_invalid() {
        let r = MitreTacticRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("T1059")); // that's a technique
        assert!(!r.matches("ta0001")); // lowercase
        assert!(!r.matches("TA001")); // 3 digits
        assert!(!r.matches("TA00010")); // 5 digits
    }

    #[test]
    fn mitre_tactic_metadata() {
        let r = MitreTacticRecognizer;
        assert_eq!(r.type_name(), "mitre_tactic");
        assert_eq!(r.confidence(), InferenceConfidence::Definite);
        assert_eq!(r.priority(), 118);
    }

    // -----------------------------------------------------------------------
    // CVSS tests
    // -----------------------------------------------------------------------

    #[test]
    fn cvss_valid() {
        let r = CvssRecognizer;
        // Critical: CVSS 10.0
        assert!(r.matches("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H"));
        // Low
        assert!(r.matches("CVSS:3.0/AV:L/AC:H/PR:H/UI:R/S:C/C:L/I:L/A:N"));
    }

    #[test]
    fn cvss_invalid() {
        let r = CvssRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("CVSS:2.0/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H")); // v2
        assert!(!r.matches("cvss:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H")); // lowercase
        assert!(!r.matches("CVSS:3.1/AV:X/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H")); // invalid AV
        assert!(!r.matches("10.0")); // just a score
    }

    #[test]
    fn cvss_metadata() {
        let r = CvssRecognizer;
        assert_eq!(r.type_name(), "cvss_vector");
        assert_eq!(r.confidence(), InferenceConfidence::Definite);
        assert_eq!(r.priority(), 115);
    }

    // -----------------------------------------------------------------------
    // Edge cases across all recognizers
    // -----------------------------------------------------------------------

    #[test]
    fn all_recognizers_reject_empty_string() {
        let recognizers: Vec<Box<dyn TypeRecognizer>> = vec![
            Box::new(CveRecognizer),
            Box::new(Ipv4Recognizer),
            Box::new(Ipv6Recognizer),
            Box::new(CidrRecognizer),
            Box::new(MacAddressRecognizer),
            Box::new(Sha256Recognizer),
            Box::new(Sha1Recognizer),
            Box::new(Md5Recognizer),
            Box::new(JwtRecognizer),
            Box::new(MitreAttackRecognizer),
            Box::new(MitreTacticRecognizer),
            Box::new(CvssRecognizer),
        ];
        for r in &recognizers {
            assert!(
                !r.matches(""),
                "{} should not match empty string",
                r.type_name()
            );
        }
    }

    #[test]
    fn all_recognizers_reject_whitespace() {
        let recognizers: Vec<Box<dyn TypeRecognizer>> = vec![
            Box::new(CveRecognizer),
            Box::new(Ipv4Recognizer),
            Box::new(Ipv6Recognizer),
            Box::new(CidrRecognizer),
            Box::new(MacAddressRecognizer),
            Box::new(Sha256Recognizer),
            Box::new(Sha1Recognizer),
            Box::new(Md5Recognizer),
            Box::new(JwtRecognizer),
            Box::new(MitreAttackRecognizer),
            Box::new(MitreTacticRecognizer),
            Box::new(CvssRecognizer),
        ];
        for r in &recognizers {
            assert!(
                !r.matches("   "),
                "{} should not match whitespace",
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
    fn near_miss_boundary_cases() {
        // CVE: 3-digit ID
        assert!(!CveRecognizer.matches("CVE-2024-123"));

        // IPv4: single octet over 255
        assert!(!Ipv4Recognizer.matches("256.1.1.1"));

        // CIDR: prefix 33
        assert!(!CidrRecognizer.matches("10.0.0.0/33"));

        // SHA-256: 63 chars (one short)
        assert!(!Sha256Recognizer
            .matches("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b85"));

        // SHA-1: 39 chars (one short)
        assert!(!Sha1Recognizer.matches("da39a3ee5e6b4b0d3255bfef95601890afd8070"));

        // MD5: 31 chars (one short)
        assert!(!Md5Recognizer.matches("d41d8cd98f00b204e9800998ecf8427"));

        // MITRE: technique with 2-digit subtechnique
        assert!(!MitreAttackRecognizer.matches("T1059.01"));

        // MAC: 5 octets
        assert!(!MacAddressRecognizer.matches("aa:bb:cc:dd:ee"));
    }
}

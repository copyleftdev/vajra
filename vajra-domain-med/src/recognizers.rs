//! Medical code type recognizers.
//!
//! Each recognizer identifies a specific medical coding system (ICD-10, CPT,
//! HCPCS, NDC, NPI) by matching string values against compiled regular
//! expressions. Regexes are compiled once using `OnceLock`.

use std::sync::OnceLock;

use regex::Regex;
use vajra_types::traits::{InferenceConfidence, TypeRecognizer};

/// Helper to compile a regex once and return a reference.
/// Returns `None` if the regex is invalid (should not happen with known patterns).
fn compile_once<'a>(lock: &'a OnceLock<Option<Regex>>, pattern: &str) -> Option<&'a Regex> {
    lock.get_or_init(|| Regex::new(pattern).ok()).as_ref()
}

// ---------------------------------------------------------------------------
// ICD-10 Recognizer
// ---------------------------------------------------------------------------

/// Recognizes ICD-10 codes: a letter followed by 2 digits, optionally a dot
/// and 1-4 more alphanumeric characters (e.g., E11.9, J18.9, M54.5, Z00.00).
pub struct Icd10Recognizer;

static ICD10_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for Icd10Recognizer {
    fn type_name(&self) -> &str {
        "icd10"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(&ICD10_RE, r"^[A-Za-z]\d{2}(\.\w{1,4})?$") else {
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
// CPT Recognizer
// ---------------------------------------------------------------------------

/// Recognizes CPT codes: exactly 5 digits (e.g., 99213, 99214, 99215).
pub struct CptRecognizer;

static CPT_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for CptRecognizer {
    fn type_name(&self) -> &str {
        "cpt"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(&CPT_RE, r"^\d{5}$") else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Heuristic
    }

    fn priority(&self) -> u32 {
        80
    }
}

// ---------------------------------------------------------------------------
// HCPCS Recognizer
// ---------------------------------------------------------------------------

/// Recognizes HCPCS Level II codes: a letter followed by 4 digits (e.g., J0120, G0101).
pub struct HcpcsRecognizer;

static HCPCS_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for HcpcsRecognizer {
    fn type_name(&self) -> &str {
        "hcpcs"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(&HCPCS_RE, r"^[A-Za-z]\d{4}$") else {
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
// NDC Recognizer
// ---------------------------------------------------------------------------

/// Recognizes NDC (National Drug Code) codes. Supports:
/// - Hyphenated format: XXXXX-XXXX-XX (5-4-2)
/// - Hyphenated format: XXXXX-XXX-XX (5-3-2)
/// - Hyphenated format: XXXXX-XXXX-X (5-4-1)
/// - 11-digit numeric (no hyphens)
pub struct NdcRecognizer;

static NDC_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for NdcRecognizer {
    fn type_name(&self) -> &str {
        "ndc"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(
            &NDC_RE,
            r"^(\d{5}-\d{3,4}-\d{1,2}|\d{11})$",
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
// NPI Recognizer
// ---------------------------------------------------------------------------

/// Recognizes NPI (National Provider Identifier) numbers: exactly 10 digits
/// starting with 1 or 2.
pub struct NpiRecognizer;

static NPI_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for NpiRecognizer {
    fn type_name(&self) -> &str {
        "npi"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(&NPI_RE, r"^[12]\d{9}$") else {
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
// Diagnosis Code Recognizer (broader)
// ---------------------------------------------------------------------------

/// Broader recognizer that matches both ICD-10-CM and ICD-10-PCS patterns.
///
/// ICD-10-CM: letter + 2 digits + optional (dot + 1-4 alphanumeric chars)
/// ICD-10-PCS: 7 alphanumeric characters (no dots)
pub struct DiagnosisCodeRecognizer;

static DIAG_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for DiagnosisCodeRecognizer {
    fn type_name(&self) -> &str {
        "diagnosis_code"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(
            &DIAG_RE,
            r"^([A-Za-z]\d{2}(\.\w{1,4})?|[A-Za-z0-9]{7})$",
        ) else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Heuristic
    }

    fn priority(&self) -> u32 {
        70
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // ICD-10 tests
    // -----------------------------------------------------------------------

    #[test]
    fn icd10_valid_codes() {
        let r = Icd10Recognizer;
        // Standard ICD-10-CM codes
        assert!(r.matches("E11.9"));
        assert!(r.matches("J18.9"));
        assert!(r.matches("M54.5"));
        assert!(r.matches("Z00.00"));
        // Without decimal
        assert!(r.matches("E11"));
        assert!(r.matches("A00"));
        // Lowercase
        assert!(r.matches("e11.9"));
        // 4-char suffix
        assert!(r.matches("S72.001A"));
    }

    #[test]
    fn icd10_invalid_codes() {
        let r = Icd10Recognizer;
        // Empty
        assert!(!r.matches(""));
        // Number start
        assert!(!r.matches("111.9"));
        // Too short
        assert!(!r.matches("E1"));
        // Dot but no suffix
        assert!(!r.matches("E11."));
        // Suffix too long (5 chars after dot)
        assert!(!r.matches("E11.12345"));
        // Just a letter
        assert!(!r.matches("E"));
        // Spaces
        assert!(!r.matches("E11 .9"));
        // Special chars
        assert!(!r.matches("E11-9"));
    }

    #[test]
    fn icd10_metadata() {
        let r = Icd10Recognizer;
        assert_eq!(r.type_name(), "icd10");
        assert_eq!(r.confidence(), InferenceConfidence::Dominant);
        assert_eq!(r.priority(), 100);
    }

    // -----------------------------------------------------------------------
    // CPT tests
    // -----------------------------------------------------------------------

    #[test]
    fn cpt_valid_codes() {
        let r = CptRecognizer;
        assert!(r.matches("99213"));
        assert!(r.matches("99214"));
        assert!(r.matches("99215"));
        assert!(r.matches("00100"));
        assert!(r.matches("12345"));
    }

    #[test]
    fn cpt_invalid_codes() {
        let r = CptRecognizer;
        assert!(!r.matches(""));
        // 4 digits
        assert!(!r.matches("9921"));
        // 6 digits
        assert!(!r.matches("992134"));
        // Letters
        assert!(!r.matches("9921A"));
        assert!(!r.matches("A9213"));
        // Spaces
        assert!(!r.matches("99 13"));
        // Dashes
        assert!(!r.matches("99-13"));
    }

    #[test]
    fn cpt_metadata() {
        let r = CptRecognizer;
        assert_eq!(r.type_name(), "cpt");
        assert_eq!(r.confidence(), InferenceConfidence::Heuristic);
        assert_eq!(r.priority(), 80);
    }

    // -----------------------------------------------------------------------
    // HCPCS tests
    // -----------------------------------------------------------------------

    #[test]
    fn hcpcs_valid_codes() {
        let r = HcpcsRecognizer;
        assert!(r.matches("J0120"));
        assert!(r.matches("G0101"));
        assert!(r.matches("A0021"));
        assert!(r.matches("j0120")); // lowercase
    }

    #[test]
    fn hcpcs_invalid_codes() {
        let r = HcpcsRecognizer;
        assert!(!r.matches(""));
        // 5 digits only
        assert!(!r.matches("10120"));
        // Too short
        assert!(!r.matches("J012"));
        // Too long
        assert!(!r.matches("J01200"));
        // Two letters
        assert!(!r.matches("JJ012"));
        // Spaces
        assert!(!r.matches("J 120"));
    }

    #[test]
    fn hcpcs_metadata() {
        let r = HcpcsRecognizer;
        assert_eq!(r.type_name(), "hcpcs");
        assert_eq!(r.confidence(), InferenceConfidence::Dominant);
        assert_eq!(r.priority(), 90);
    }

    // -----------------------------------------------------------------------
    // NDC tests
    // -----------------------------------------------------------------------

    #[test]
    fn ndc_valid_codes() {
        let r = NdcRecognizer;
        // 5-4-2 format
        assert!(r.matches("12345-6789-01"));
        // 5-3-2 format
        assert!(r.matches("12345-678-01"));
        // 5-4-1 format
        assert!(r.matches("12345-6789-0"));
        // 11-digit numeric
        assert!(r.matches("12345678901"));
    }

    #[test]
    fn ndc_invalid_codes() {
        let r = NdcRecognizer;
        assert!(!r.matches(""));
        // Too few digits
        assert!(!r.matches("1234567890"));
        // Too many digits
        assert!(!r.matches("123456789012"));
        // Wrong hyphen format
        assert!(!r.matches("1234-5678-01"));
        // Letters
        assert!(!r.matches("1234A-6789-01"));
        // Spaces
        assert!(!r.matches("12345 6789 01"));
    }

    #[test]
    fn ndc_metadata() {
        let r = NdcRecognizer;
        assert_eq!(r.type_name(), "ndc");
        assert_eq!(r.confidence(), InferenceConfidence::Dominant);
        assert_eq!(r.priority(), 95);
    }

    // -----------------------------------------------------------------------
    // NPI tests
    // -----------------------------------------------------------------------

    #[test]
    fn npi_valid_codes() {
        let r = NpiRecognizer;
        assert!(r.matches("1234567890"));
        assert!(r.matches("2345678901"));
        assert!(r.matches("1000000000"));
        assert!(r.matches("2999999999"));
    }

    #[test]
    fn npi_invalid_codes() {
        let r = NpiRecognizer;
        assert!(!r.matches(""));
        // Starts with 0
        assert!(!r.matches("0234567890"));
        // Starts with 3
        assert!(!r.matches("3234567890"));
        // 9 digits
        assert!(!r.matches("123456789"));
        // 11 digits
        assert!(!r.matches("12345678901"));
        // Letters
        assert!(!r.matches("1234A67890"));
        // Spaces
        assert!(!r.matches("123 567890"));
    }

    #[test]
    fn npi_metadata() {
        let r = NpiRecognizer;
        assert_eq!(r.type_name(), "npi");
        assert_eq!(r.confidence(), InferenceConfidence::Dominant);
        assert_eq!(r.priority(), 85);
    }

    // -----------------------------------------------------------------------
    // Diagnosis code (broader) tests
    // -----------------------------------------------------------------------

    #[test]
    fn diagnosis_code_valid_icd10cm() {
        let r = DiagnosisCodeRecognizer;
        // Standard ICD-10-CM
        assert!(r.matches("E11.9"));
        assert!(r.matches("J18.9"));
        assert!(r.matches("M54.5"));
        assert!(r.matches("Z00.00"));
        assert!(r.matches("E11"));
    }

    #[test]
    fn diagnosis_code_valid_icd10pcs() {
        let r = DiagnosisCodeRecognizer;
        // 7-character PCS codes
        assert!(r.matches("0BJ08ZZ"));
        assert!(r.matches("0DT60ZZ"));
        assert!(r.matches("0W3P0ZZ"));
    }

    #[test]
    fn diagnosis_code_invalid() {
        let r = DiagnosisCodeRecognizer;
        assert!(!r.matches(""));
        // 8-character string (too long for PCS, invalid CM)
        assert!(!r.matches("0BJ08ZZX"));
        // 6-character string (wrong length for PCS, missing dot for CM)
        assert!(!r.matches("0BJ08Z"));
        // Only digits, not 7 long or 11 long
        assert!(!r.matches("12345"));
        // Special chars
        assert!(!r.matches("E11-9"));
    }

    #[test]
    fn diagnosis_code_metadata() {
        let r = DiagnosisCodeRecognizer;
        assert_eq!(r.type_name(), "diagnosis_code");
        assert_eq!(r.confidence(), InferenceConfidence::Heuristic);
        assert_eq!(r.priority(), 70);
    }

    // -----------------------------------------------------------------------
    // Edge cases across all recognizers
    // -----------------------------------------------------------------------

    #[test]
    fn all_recognizers_reject_empty_string() {
        let recognizers: Vec<Box<dyn TypeRecognizer>> = vec![
            Box::new(Icd10Recognizer),
            Box::new(CptRecognizer),
            Box::new(HcpcsRecognizer),
            Box::new(NdcRecognizer),
            Box::new(NpiRecognizer),
            Box::new(DiagnosisCodeRecognizer),
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
            Box::new(Icd10Recognizer),
            Box::new(CptRecognizer),
            Box::new(HcpcsRecognizer),
            Box::new(NdcRecognizer),
            Box::new(NpiRecognizer),
            Box::new(DiagnosisCodeRecognizer),
        ];
        for r in &recognizers {
            assert!(
                !r.matches("   "),
                "{} should not match whitespace",
                r.type_name()
            );
            assert!(
                !r.matches("\t"),
                "{} should not match tab",
                r.type_name()
            );
            assert!(
                !r.matches("\n"),
                "{} should not match newline",
                r.type_name()
            );
        }
    }

    #[test]
    fn near_miss_boundary_cases() {
        // ICD-10: letter + 1 digit (too short)
        assert!(!Icd10Recognizer.matches("E1"));
        // ICD-10: letter + 2 digits + dot + 5 chars (suffix too long)
        assert!(!Icd10Recognizer.matches("E11.12345"));

        // CPT: 4 digits (too few)
        assert!(!CptRecognizer.matches("9921"));
        // CPT: 6 digits (too many)
        assert!(!CptRecognizer.matches("992134"));

        // HCPCS: letter + 3 digits (too short)
        assert!(!HcpcsRecognizer.matches("J012"));
        // HCPCS: letter + 5 digits (too long)
        assert!(!HcpcsRecognizer.matches("J01200"));

        // NPI: 9 digits starting with 1 (too short)
        assert!(!NpiRecognizer.matches("123456789"));
        // NPI: 10 digits starting with 3 (wrong start)
        assert!(!NpiRecognizer.matches("3234567890"));

        // NDC: 10-digit numeric (too short for 11-digit pattern)
        assert!(!NdcRecognizer.matches("1234567890"));
    }
}

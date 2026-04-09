//! Source code domain type recognizers.
//!
//! Recognizes patterns in the JSON trees produced by vajra-source:
//! tree-sitter node kinds, field roles, and identifier naming conventions.

use std::sync::OnceLock;

use regex::Regex;
use vajra_types::traits::{InferenceConfidence, TypeRecognizer};

/// Helper to compile a regex once and return a reference.
fn compile_once<'a>(lock: &'a OnceLock<Option<Regex>>, pattern: &str) -> Option<&'a Regex> {
    lock.get_or_init(|| Regex::new(pattern).ok()).as_ref()
}

// ---------------------------------------------------------------------------
// Snake Case Identifier Recognizer
// ---------------------------------------------------------------------------

/// Recognizes snake_case identifiers (e.g., my_function, some_variable).
pub struct SnakeCaseRecognizer;

static SNAKE_CASE_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for SnakeCaseRecognizer {
    fn type_name(&self) -> &str {
        "snake_case_identifier"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(&SNAKE_CASE_RE, r"^[a-z][a-z0-9]*(?:_[a-z0-9]+)+$") else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Heuristic
    }

    fn priority(&self) -> u32 {
        40
    }
}

// ---------------------------------------------------------------------------
// Camel Case Identifier Recognizer
// ---------------------------------------------------------------------------

/// Recognizes camelCase identifiers (e.g., myFunction, someVariable).
pub struct CamelCaseRecognizer;

static CAMEL_CASE_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for CamelCaseRecognizer {
    fn type_name(&self) -> &str {
        "camel_case_identifier"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(&CAMEL_CASE_RE, r"^[a-z][a-zA-Z0-9]*[A-Z][a-zA-Z0-9]*$") else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Heuristic
    }

    fn priority(&self) -> u32 {
        40
    }
}

// ---------------------------------------------------------------------------
// Pascal Case Identifier Recognizer
// ---------------------------------------------------------------------------

/// Recognizes PascalCase identifiers (e.g., MyClass, SomeType).
pub struct PascalCaseRecognizer;

static PASCAL_CASE_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for PascalCaseRecognizer {
    fn type_name(&self) -> &str {
        "pascal_case_identifier"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(&PASCAL_CASE_RE, r"^[A-Z][a-zA-Z0-9]+$") else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Heuristic
    }

    fn priority(&self) -> u32 {
        40
    }
}

// ---------------------------------------------------------------------------
// SCREAMING_SNAKE_CASE Recognizer
// ---------------------------------------------------------------------------

/// Recognizes SCREAMING_SNAKE_CASE identifiers (e.g., MAX_SIZE, HTTP_STATUS).
pub struct ScreamingSnakeCaseRecognizer;

static SCREAMING_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for ScreamingSnakeCaseRecognizer {
    fn type_name(&self) -> &str {
        "screaming_snake_case"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(&SCREAMING_RE, r"^[A-Z][A-Z0-9]*(?:_[A-Z0-9]+)+$") else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Heuristic
    }

    fn priority(&self) -> u32 {
        45
    }
}

// ---------------------------------------------------------------------------
// Import Path Recognizer
// ---------------------------------------------------------------------------

/// Recognizes module/import paths (e.g., std::collections::HashMap,
/// com.example.MyClass, @angular/core).
pub struct ImportPathRecognizer;

static IMPORT_PATH_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for ImportPathRecognizer {
    fn type_name(&self) -> &str {
        "import_path"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(
            &IMPORT_PATH_RE,
            r"^(?:[a-zA-Z_][\w]*(?:::|\.|\/))+[a-zA-Z_][\w]*$|^@[\w-]+/[\w-]+$",
        ) else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Heuristic
    }

    fn priority(&self) -> u32 {
        50
    }
}

// ---------------------------------------------------------------------------
// File Path Recognizer
// ---------------------------------------------------------------------------

/// Recognizes file paths with common source extensions.
pub struct SourceFilePathRecognizer;

static FILE_PATH_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for SourceFilePathRecognizer {
    fn type_name(&self) -> &str {
        "source_file_path"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(
            &FILE_PATH_RE,
            r"^(?:\.{0,2}/)?[\w./-]+\.(?:rs|py|js|ts|go|java|c|cpp|h|hpp|rb)$",
        ) else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Dominant
    }

    fn priority(&self) -> u32 {
        55
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Snake case ---

    #[test]
    fn snake_case_valid() {
        let r = SnakeCaseRecognizer;
        assert!(r.matches("my_function"));
        assert!(r.matches("some_variable_name"));
        assert!(r.matches("get_value_2"));
    }

    #[test]
    fn snake_case_invalid() {
        let r = SnakeCaseRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("myFunction")); // camelCase
        assert!(!r.matches("MyClass")); // PascalCase
        assert!(!r.matches("x")); // single word, no underscore
        assert!(!r.matches("_private")); // starts with underscore
    }

    // --- Camel case ---

    #[test]
    fn camel_case_valid() {
        let r = CamelCaseRecognizer;
        assert!(r.matches("myFunction"));
        assert!(r.matches("getValueFrom"));
        assert!(r.matches("parseJSON"));
    }

    #[test]
    fn camel_case_invalid() {
        let r = CamelCaseRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("my_function")); // snake_case
        assert!(!r.matches("MyClass")); // PascalCase
        assert!(!r.matches("lowercase")); // no uppercase
    }

    // --- Pascal case ---

    #[test]
    fn pascal_case_valid() {
        let r = PascalCaseRecognizer;
        assert!(r.matches("MyClass"));
        assert!(r.matches("SomeType"));
        assert!(r.matches("HttpClient"));
    }

    #[test]
    fn pascal_case_invalid() {
        let r = PascalCaseRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("myFunction")); // camelCase
        assert!(!r.matches("my_function")); // snake_case
        assert!(!r.matches("X")); // too short
    }

    // --- Screaming snake case ---

    #[test]
    fn screaming_snake_valid() {
        let r = ScreamingSnakeCaseRecognizer;
        assert!(r.matches("MAX_SIZE"));
        assert!(r.matches("HTTP_STATUS_CODE"));
        assert!(r.matches("API_KEY_V2"));
    }

    #[test]
    fn screaming_snake_invalid() {
        let r = ScreamingSnakeCaseRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("max_size")); // lowercase
        assert!(!r.matches("MAX")); // no underscore
        assert!(!r.matches("My_Mixed")); // mixed case
    }

    // --- Import path ---

    #[test]
    fn import_path_valid() {
        let r = ImportPathRecognizer;
        assert!(r.matches("std::collections::HashMap"));
        assert!(r.matches("com.example.MyClass"));
        assert!(r.matches("@angular/core"));
        assert!(r.matches("os/path"));
    }

    #[test]
    fn import_path_invalid() {
        let r = ImportPathRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("simple"));
        assert!(!r.matches("123::bad"));
    }

    // --- File path ---

    #[test]
    fn file_path_valid() {
        let r = SourceFilePathRecognizer;
        assert!(r.matches("src/main.rs"));
        assert!(r.matches("./lib/utils.py"));
        assert!(r.matches("../pkg/server.go"));
        assert!(r.matches("index.js"));
    }

    #[test]
    fn file_path_invalid() {
        let r = SourceFilePathRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("data.json")); // not a source extension
        assert!(!r.matches("no_extension"));
    }

    // --- Cross-recognizer ---

    #[test]
    fn all_recognizers_reject_empty() {
        let recognizers: Vec<Box<dyn TypeRecognizer>> = vec![
            Box::new(SnakeCaseRecognizer),
            Box::new(CamelCaseRecognizer),
            Box::new(PascalCaseRecognizer),
            Box::new(ScreamingSnakeCaseRecognizer),
            Box::new(ImportPathRecognizer),
            Box::new(SourceFilePathRecognizer),
        ];
        for r in &recognizers {
            assert!(!r.matches(""), "{} should not match empty", r.type_name());
        }
    }

    #[test]
    fn all_recognizers_reject_whitespace() {
        let recognizers: Vec<Box<dyn TypeRecognizer>> = vec![
            Box::new(SnakeCaseRecognizer),
            Box::new(CamelCaseRecognizer),
            Box::new(PascalCaseRecognizer),
            Box::new(ScreamingSnakeCaseRecognizer),
            Box::new(ImportPathRecognizer),
            Box::new(SourceFilePathRecognizer),
        ];
        for r in &recognizers {
            assert!(
                !r.matches("   "),
                "{} should not match whitespace",
                r.type_name()
            );
        }
    }
}

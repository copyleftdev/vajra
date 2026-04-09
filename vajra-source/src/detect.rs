//! Language detection from file extensions.

use std::path::Path;

use crate::config::SourceLanguage;

/// Detect language from a file extension.
///
/// Returns `None` for unrecognized extensions or if the detected language's
/// feature flag is not enabled.
pub fn detect_language(path: &Path) -> Option<SourceLanguage> {
    let ext = path.extension()?.to_str()?;
    match ext {
        #[cfg(feature = "rust")]
        "rs" => Some(SourceLanguage::Rust),
        #[cfg(feature = "python")]
        "py" | "pyi" => Some(SourceLanguage::Python),
        #[cfg(feature = "javascript")]
        "js" | "mjs" | "cjs" | "jsx" => Some(SourceLanguage::JavaScript),
        #[cfg(feature = "typescript")]
        "ts" | "tsx" | "mts" | "cts" => Some(SourceLanguage::TypeScript),
        #[cfg(feature = "go")]
        "go" => Some(SourceLanguage::Go),
        #[cfg(feature = "java")]
        "java" => Some(SourceLanguage::Java),
        #[cfg(feature = "c")]
        "c" | "h" => Some(SourceLanguage::C),
        #[cfg(feature = "cpp")]
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "hh" => Some(SourceLanguage::Cpp),
        #[cfg(feature = "ruby")]
        "rb" => Some(SourceLanguage::Ruby),
        _ => None,
    }
}

/// List all languages available in this build.
#[allow(clippy::vec_init_then_push)] // cfg-gated pushes can't use vec![]
pub fn available_languages() -> Vec<SourceLanguage> {
    let mut langs = Vec::new();
    #[cfg(feature = "rust")]
    langs.push(SourceLanguage::Rust);
    #[cfg(feature = "python")]
    langs.push(SourceLanguage::Python);
    #[cfg(feature = "javascript")]
    langs.push(SourceLanguage::JavaScript);
    #[cfg(feature = "typescript")]
    langs.push(SourceLanguage::TypeScript);
    #[cfg(feature = "go")]
    langs.push(SourceLanguage::Go);
    #[cfg(feature = "java")]
    langs.push(SourceLanguage::Java);
    #[cfg(feature = "c")]
    langs.push(SourceLanguage::C);
    #[cfg(feature = "cpp")]
    langs.push(SourceLanguage::Cpp);
    #[cfg(feature = "ruby")]
    langs.push(SourceLanguage::Ruby);
    langs
}

/// Returns true if the file extension is a recognized source code format.
pub fn is_source_file(path: &Path) -> bool {
    detect_language(path).is_some()
}

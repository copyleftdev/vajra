//! Language grammar dispatch.
//!
//! Maps `SourceLanguage` to a `tree_sitter::Language` for parsing.
//! Each grammar is behind a feature flag.

use tree_sitter::Language;
use vajra_types::error::VajraError;

use crate::config::SourceLanguage;

/// Get the tree-sitter `Language` for a `SourceLanguage`.
///
/// # Errors
/// Returns `VajraError::Config` if the language feature is not enabled.
pub fn get_grammar(lang: SourceLanguage) -> Result<Language, VajraError> {
    match lang {
        #[cfg(feature = "rust")]
        SourceLanguage::Rust => Ok(Language::from(tree_sitter_rust::LANGUAGE)),

        #[cfg(feature = "python")]
        SourceLanguage::Python => Ok(Language::from(tree_sitter_python::LANGUAGE)),

        #[cfg(feature = "javascript")]
        SourceLanguage::JavaScript => Ok(Language::from(tree_sitter_javascript::LANGUAGE)),

        #[cfg(feature = "typescript")]
        SourceLanguage::TypeScript => {
            Ok(Language::from(tree_sitter_typescript::LANGUAGE_TYPESCRIPT))
        }

        #[cfg(feature = "go")]
        SourceLanguage::Go => Ok(Language::from(tree_sitter_go::LANGUAGE)),

        #[cfg(feature = "java")]
        SourceLanguage::Java => Ok(Language::from(tree_sitter_java::LANGUAGE)),

        #[cfg(feature = "c")]
        SourceLanguage::C => Ok(Language::from(tree_sitter_c::LANGUAGE)),

        #[cfg(feature = "cpp")]
        SourceLanguage::Cpp => Ok(Language::from(tree_sitter_cpp::LANGUAGE)),

        #[cfg(feature = "ruby")]
        SourceLanguage::Ruby => Ok(Language::from(tree_sitter_ruby::LANGUAGE)),
    }
}

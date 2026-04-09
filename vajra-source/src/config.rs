//! Configuration types for source code parsing.

use std::fmt;

/// Configuration for source code parsing.
#[derive(Debug, Clone)]
pub struct SourceConfig {
    /// Language to parse as. None = auto-detect from file extension.
    pub language: Option<SourceLanguage>,
    /// Include anonymous (unnamed) tree-sitter nodes (punctuation, keywords).
    /// Default: false.
    pub include_anonymous: bool,
    /// Include span (line/column) information on each node. Default: false.
    pub include_spans: bool,
    /// Include source text on leaf nodes. Default: true.
    pub include_text: bool,
    /// Maximum file size in bytes before rejecting. Default: 10MB.
    pub max_file_size: u64,
    /// Include semantic labels on nodes whose tree-sitter kind maps to a
    /// known construct (function, class, import, etc.). Default: false.
    pub semantic_paths: bool,
}

impl Default for SourceConfig {
    fn default() -> Self {
        Self {
            language: None,
            include_anonymous: false,
            include_spans: false,
            include_text: true,
            max_file_size: 10 * 1024 * 1024,
            semantic_paths: false,
        }
    }
}

/// A supported programming language.
///
/// Each variant corresponds to a tree-sitter grammar crate, gated by
/// a Cargo feature flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SourceLanguage {
    #[cfg(feature = "rust")]
    Rust,
    #[cfg(feature = "python")]
    Python,
    #[cfg(feature = "javascript")]
    JavaScript,
    #[cfg(feature = "typescript")]
    TypeScript,
    #[cfg(feature = "go")]
    Go,
    #[cfg(feature = "java")]
    Java,
    #[cfg(feature = "c")]
    C,
    #[cfg(feature = "cpp")]
    Cpp,
    #[cfg(feature = "ruby")]
    Ruby,
}

impl fmt::Display for SourceLanguage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            #[cfg(feature = "rust")]
            Self::Rust => "rust",
            #[cfg(feature = "python")]
            Self::Python => "python",
            #[cfg(feature = "javascript")]
            Self::JavaScript => "javascript",
            #[cfg(feature = "typescript")]
            Self::TypeScript => "typescript",
            #[cfg(feature = "go")]
            Self::Go => "go",
            #[cfg(feature = "java")]
            Self::Java => "java",
            #[cfg(feature = "c")]
            Self::C => "c",
            #[cfg(feature = "cpp")]
            Self::Cpp => "cpp",
            #[cfg(feature = "ruby")]
            Self::Ruby => "ruby",
        };
        write!(f, "{name}")
    }
}

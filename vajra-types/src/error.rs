//! Error types for the Vajra pipeline.

use std::path::PathBuf;

/// Top-level error type for Vajra operations.
#[derive(Debug, thiserror::Error)]
pub enum VajraError {
    /// JSON parsing failed.
    #[error("parse error at byte {byte_offset}: {message}")]
    Parse {
        byte_offset: usize,
        message: String,
        source_path: Option<PathBuf>,
    },

    /// Input file could not be read.
    #[error("failed to read {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    /// Document exceeds configured limits.
    #[error("limit exceeded: {message}")]
    LimitExceeded { message: String },

    /// Analysis failed for a specific path.
    #[error("analysis error at {path}: {message}")]
    Analysis { path: String, message: String },

    /// Configuration is invalid.
    #[error("configuration error: {message}")]
    Config { message: String },

    /// A plugin failed (core analysis continues).
    #[error("plugin '{plugin_name}' failed: {message}")]
    Plugin {
        plugin_name: String,
        message: String,
    },
}

/// Result type alias using `VajraError`.
pub type Result<T> = std::result::Result<T, VajraError>;

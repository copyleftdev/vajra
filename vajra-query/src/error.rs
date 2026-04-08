//! Error types for the Vajra query engine.

/// Errors that can occur during query parsing or evaluation.
#[derive(Debug, thiserror::Error)]
pub enum QueryError {
    /// A parse error at a specific position in the input.
    #[error("parse error at position {position}: {message}")]
    Parse {
        /// Byte offset in the input where the error occurred.
        position: usize,
        /// Human-readable description of the error.
        message: String,
    },

    /// An unknown function was referenced.
    #[error("unknown function: {name}")]
    UnknownFunction {
        /// The unrecognized function name.
        name: String,
    },

    /// A path was not found in the document or stats.
    #[error("path not found: {path}")]
    PathNotFound {
        /// The path expression that could not be resolved.
        path: String,
    },

    /// A stats-dependent function was called without a stats context.
    #[error("stats not available (run with stats context)")]
    StatsRequired,

    /// A type mismatch during evaluation.
    #[error("type error: {message}")]
    TypeError {
        /// Human-readable description of the type mismatch.
        message: String,
    },
}

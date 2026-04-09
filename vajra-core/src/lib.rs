//! Core parsing, path extraction, and canonicalization for the Vajra pipeline.
//!
//! This crate provides two primary capabilities:
//! - **Parsing**: JSON parsing with structural path trie construction (`parse` module)
//! - **Canonicalization**: RFC 8785 (JCS) deterministic JSON serialization (`canonical` module)
//! - **Input loading**: Unified loading from files (plain/compressed), stdin, and URLs (`input` module)

pub mod canonical;
pub mod cpuprofile;
pub mod decompress;
pub mod event;
pub mod fetch;
pub mod formats;
pub mod git;
pub mod input;
pub mod markdown;
pub mod parse;
pub mod pdf;
pub mod pipeline;
pub mod redact;
pub mod strace;
pub mod stream;

pub use canonical::canonicalize;
pub use decompress::{decompress_bytes, decompress_file, Compression};
pub use event::{JsonEvent, ScalarValue};
pub use fetch::is_url;
pub use formats::{
    detect_format, parse_auto, parse_csv, parse_file_auto, parse_ndjson, parse_yaml, InputFormat,
};
pub use git::{is_git_repo, load_git_log, GitLogConfig};
pub use input::{load_documents, load_input, resolve_input, InputSource, LoadedInput};
pub use markdown::parse_markdown;
pub use parse::{parse_file, parse_str};
pub use pdf::{parse_pdf_bytes, parse_pdf_file};
pub use pipeline::{emit_and_index, metadata_from_events, trie_from_events};
pub use stream::{emit_events, load_adaptive, AnalysisMode};

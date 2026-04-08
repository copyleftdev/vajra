//! Query interface for Vajra analysis results.
//!
//! Provides a simple expression language for path-based filtering and
//! analysis functions over parsed JSON documents.
//!
//! # Expression syntax
//!
//! - **Paths**: `$.claims[*].amount` — select matching wildcard paths
//! - **Functions**: `entropy($.claims[*].status)` — compute a metric
//! - **Comparisons**: `entropy($.claims[*].status) > 0.5` — filter by threshold
//! - **Literals**: `3.14` — numeric constants

pub mod error;
pub mod eval;
pub mod expr;
pub mod parser;

pub use error::QueryError;
pub use eval::{evaluate, QueryContext, QueryResult};
pub use expr::{CompareOp, Expr, FunctionCall, PathPattern, PatternSegment};
pub use parser::parse_expr;

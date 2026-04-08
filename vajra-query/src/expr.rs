//! Expression types for the Vajra query language.

/// A parsed query expression.
#[derive(Debug, Clone)]
pub enum Expr {
    /// A path selector: `$.claims[*].amount`
    Path(PathPattern),
    /// A function call: `entropy($.claims[*].status)`
    Function(FunctionCall),
    /// A comparison: `entropy($.claims[*].status) > 0.5`
    Compare(Box<Expr>, CompareOp, Box<Expr>),
    /// A literal numeric value.
    Literal(f64),
}

/// A pattern that matches one or more wildcard paths.
#[derive(Debug, Clone)]
pub struct PathPattern {
    /// The segments that make up this pattern.
    pub segments: Vec<PatternSegment>,
}

impl PathPattern {
    /// Returns the canonical string representation of this pattern.
    #[must_use]
    pub fn as_str(&self) -> String {
        let mut out = String::new();
        for seg in &self.segments {
            match seg {
                PatternSegment::Root => out.push('$'),
                PatternSegment::Key(k) => {
                    out.push('.');
                    out.push_str(k);
                }
                PatternSegment::ArrayWildcard => out.push_str("[*]"),
                PatternSegment::Glob => out.push_str(".**"),
            }
        }
        out
    }
}

/// A single segment in a path pattern.
#[derive(Debug, Clone, PartialEq)]
pub enum PatternSegment {
    /// The root `$` segment.
    Root,
    /// A named object key.
    Key(String),
    /// An array wildcard `[*]`.
    ArrayWildcard,
    /// A recursive glob `**` that matches any depth.
    Glob,
}

/// A function call expression.
#[derive(Debug, Clone)]
pub struct FunctionCall {
    /// The function name.
    pub name: String,
    /// The arguments to the function.
    pub args: Vec<Expr>,
}

/// A comparison operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    /// Greater than (`>`)
    Gt,
    /// Less than (`<`)
    Lt,
    /// Greater than or equal (`>=`)
    Gte,
    /// Less than or equal (`<=`)
    Lte,
    /// Equal (`==`)
    Eq,
    /// Not equal (`!=`)
    Ne,
}

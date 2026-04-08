//! Recursive-descent parser for Vajra query expressions.
//!
//! Grammar (informal):
//! ```text
//! expr     = compare | funcall | path | literal
//! compare  = (funcall | path) op (literal | funcall | path)
//! funcall  = IDENT "(" expr ("," expr)* ")"
//! path     = "$" ("." IDENT | "[*]" | ".**")*
//! literal  = NUMBER
//! op       = ">" | "<" | ">=" | "<=" | "==" | "!="
//! IDENT    = [a-zA-Z_][a-zA-Z0-9_]*
//! NUMBER   = [0-9]+ ("." [0-9]+)?
//! ```

use crate::error::QueryError;
use crate::expr::{CompareOp, Expr, FunctionCall, PathPattern, PatternSegment};

/// Parse a query expression string into an [`Expr`].
///
/// # Errors
///
/// Returns [`QueryError::Parse`] if the input is not a valid expression.
pub fn parse_expr(input: &str) -> Result<Expr, QueryError> {
    let mut parser = Parser::new(input);
    let expr = parser.parse_top()?;
    parser.skip_whitespace();
    if parser.pos < parser.input.len() {
        return Err(parser.error("unexpected trailing characters"));
    }
    Ok(expr)
}

struct Parser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn error(&self, message: &str) -> QueryError {
        QueryError::Parse {
            position: self.pos,
            message: message.to_owned(),
        }
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() && self.input.as_bytes()[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.as_bytes().get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.input.as_bytes().get(self.pos + offset).copied()
    }

    fn expect(&mut self, ch: u8) -> Result<(), QueryError> {
        if self.peek() == Some(ch) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.error(&format!("expected '{}'", ch as char)))
        }
    }

    /// Top-level: parse a primary expression, then optionally a comparison.
    fn parse_top(&mut self) -> Result<Expr, QueryError> {
        self.skip_whitespace();
        let lhs = self.parse_primary()?;
        self.skip_whitespace();

        if let Some(op) = self.try_parse_op() {
            self.skip_whitespace();
            let rhs = self.parse_primary()?;
            Ok(Expr::Compare(Box::new(lhs), op, Box::new(rhs)))
        } else {
            Ok(lhs)
        }
    }

    /// Parse a primary expression: function call, path, or literal.
    fn parse_primary(&mut self) -> Result<Expr, QueryError> {
        self.skip_whitespace();
        match self.peek() {
            Some(b'$') => self.parse_path(),
            Some(b) if b.is_ascii_digit() => self.parse_literal(),
            Some(b) if b.is_ascii_alphabetic() || b == b'_' => self.parse_funcall(),
            Some(_) => Err(self.error("unexpected character")),
            None => Err(self.error("unexpected end of input")),
        }
    }

    fn parse_path(&mut self) -> Result<Expr, QueryError> {
        self.expect(b'$')?;
        let mut segments = vec![PatternSegment::Root];

        loop {
            if self.pos < self.input.len() && self.input.as_bytes()[self.pos] == b'.' {
                // Check for ".**"
                if self.peek_at(1) == Some(b'*') && self.peek_at(2) == Some(b'*') {
                    self.pos += 3; // consume ".**"
                    segments.push(PatternSegment::Glob);
                } else {
                    // Regular ".key"
                    self.pos += 1; // consume '.'
                    let key = self.parse_ident()?;
                    segments.push(PatternSegment::Key(key));
                }
            } else if self.pos + 2 < self.input.len()
                && &self.input[self.pos..self.pos + 3] == "[*]"
            {
                self.pos += 3;
                segments.push(PatternSegment::ArrayWildcard);
            } else {
                break;
            }
        }

        Ok(Expr::Path(PathPattern { segments }))
    }

    fn parse_ident(&mut self) -> Result<String, QueryError> {
        let start = self.pos;
        while self.pos < self.input.len() {
            let b = self.input.as_bytes()[self.pos];
            if b.is_ascii_alphanumeric() || b == b'_' {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos == start {
            return Err(self.error("expected identifier"));
        }
        Ok(self.input[start..self.pos].to_owned())
    }

    fn parse_literal(&mut self) -> Result<Expr, QueryError> {
        let start = self.pos;
        while self.pos < self.input.len() && self.input.as_bytes()[self.pos].is_ascii_digit() {
            self.pos += 1;
        }
        if self.pos < self.input.len() && self.input.as_bytes()[self.pos] == b'.' {
            self.pos += 1;
            while self.pos < self.input.len() && self.input.as_bytes()[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
        }
        let s = &self.input[start..self.pos];
        let val: f64 = s.parse().map_err(|_| self.error("invalid number"))?;
        Ok(Expr::Literal(val))
    }

    fn parse_funcall(&mut self) -> Result<Expr, QueryError> {
        let name = self.parse_ident()?;
        self.skip_whitespace();
        self.expect(b'(')?;
        self.skip_whitespace();

        let mut args = Vec::new();
        if self.peek() != Some(b')') {
            args.push(self.parse_primary()?);
            self.skip_whitespace();
            while self.peek() == Some(b',') {
                self.pos += 1; // consume ','
                self.skip_whitespace();
                args.push(self.parse_primary()?);
                self.skip_whitespace();
            }
        }

        self.skip_whitespace();
        self.expect(b')')?;

        Ok(Expr::Function(FunctionCall { name, args }))
    }

    fn try_parse_op(&mut self) -> Option<CompareOp> {
        // Two-character operators first.
        if self.pos + 1 < self.input.len() {
            let two = &self.input[self.pos..self.pos + 2];
            let op = match two {
                ">=" => Some(CompareOp::Gte),
                "<=" => Some(CompareOp::Lte),
                "==" => Some(CompareOp::Eq),
                "!=" => Some(CompareOp::Ne),
                _ => None,
            };
            if op.is_some() {
                self.pos += 2;
                return op;
            }
        }
        // Single-character operators.
        match self.peek() {
            Some(b'>') => {
                self.pos += 1;
                Some(CompareOp::Gt)
            }
            Some(b'<') => {
                self.pos += 1;
                Some(CompareOp::Lt)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::{CompareOp, Expr, PatternSegment};

    #[test]
    fn parse_simple_path() {
        let expr = parse_expr("$.a.b").unwrap();
        match expr {
            Expr::Path(p) => {
                assert_eq!(p.segments.len(), 3);
                assert_eq!(p.segments[0], PatternSegment::Root);
                assert_eq!(p.segments[1], PatternSegment::Key("a".to_owned()));
                assert_eq!(p.segments[2], PatternSegment::Key("b".to_owned()));
            }
            other => panic!("expected Path, got {other:?}"),
        }
    }

    #[test]
    fn parse_array_wildcard() {
        let expr = parse_expr("$.items[*].name").unwrap();
        match expr {
            Expr::Path(p) => {
                assert_eq!(p.segments.len(), 4);
                assert_eq!(p.segments[0], PatternSegment::Root);
                assert_eq!(p.segments[1], PatternSegment::Key("items".to_owned()));
                assert_eq!(p.segments[2], PatternSegment::ArrayWildcard);
                assert_eq!(p.segments[3], PatternSegment::Key("name".to_owned()));
            }
            other => panic!("expected Path, got {other:?}"),
        }
    }

    #[test]
    fn parse_function_call() {
        let expr = parse_expr("entropy($.status)").unwrap();
        match expr {
            Expr::Function(fc) => {
                assert_eq!(fc.name, "entropy");
                assert_eq!(fc.args.len(), 1);
                match &fc.args[0] {
                    Expr::Path(p) => {
                        assert_eq!(p.segments.len(), 2);
                        assert_eq!(p.segments[1], PatternSegment::Key("status".to_owned()));
                    }
                    other => panic!("expected Path arg, got {other:?}"),
                }
            }
            other => panic!("expected Function, got {other:?}"),
        }
    }

    #[test]
    fn parse_comparison() {
        let expr = parse_expr("entropy($.x) > 0.5").unwrap();
        match expr {
            Expr::Compare(lhs, op, rhs) => {
                assert_eq!(op, CompareOp::Gt);
                match lhs.as_ref() {
                    Expr::Function(fc) => assert_eq!(fc.name, "entropy"),
                    other => panic!("expected Function on LHS, got {other:?}"),
                }
                match rhs.as_ref() {
                    Expr::Literal(v) => assert!((v - 0.5).abs() < f64::EPSILON),
                    other => panic!("expected Literal on RHS, got {other:?}"),
                }
            }
            other => panic!("expected Compare, got {other:?}"),
        }
    }

    #[test]
    fn parse_literal() {
        let expr = parse_expr("3.14").unwrap();
        match expr {
            Expr::Literal(v) => assert!((v - 3.14).abs() < f64::EPSILON),
            other => panic!("expected Literal, got {other:?}"),
        }
    }

    #[test]
    fn parse_error_invalid_path() {
        let result = parse_expr("$..invalid");
        assert!(result.is_err());
        match result {
            Err(QueryError::Parse { position, .. }) => {
                // Position should be at the second dot (after "$.")
                assert!(position > 0);
            }
            other => panic!("expected Parse error, got {other:?}"),
        }
    }

    #[test]
    fn parse_error_no_args_ident_not_followed_by_paren() {
        // An identifier not followed by '(' is an error at top level
        // because it looks like a function call but has no parens.
        let result = parse_expr("unknownfunc");
        assert!(result.is_err());
    }

    #[test]
    fn parse_all_comparison_ops() {
        for (input, expected_op) in &[
            ("$.x >= 1", CompareOp::Gte),
            ("$.x <= 1", CompareOp::Lte),
            ("$.x == 1", CompareOp::Eq),
            ("$.x != 1", CompareOp::Ne),
            ("$.x > 1", CompareOp::Gt),
            ("$.x < 1", CompareOp::Lt),
        ] {
            let expr = parse_expr(input).unwrap();
            match expr {
                Expr::Compare(_, op, _) => assert_eq!(op, *expected_op, "failed for {input}"),
                other => panic!("expected Compare for {input}, got {other:?}"),
            }
        }
    }

    #[test]
    fn parse_glob_pattern() {
        let expr = parse_expr("$.**").unwrap();
        match expr {
            Expr::Path(p) => {
                assert_eq!(p.segments.len(), 2);
                assert_eq!(p.segments[1], PatternSegment::Glob);
            }
            other => panic!("expected Path with Glob, got {other:?}"),
        }
    }

    #[test]
    fn parse_function_with_two_args() {
        let expr = parse_expr("rarity($.code, 3.0)").unwrap();
        match expr {
            Expr::Function(fc) => {
                assert_eq!(fc.name, "rarity");
                assert_eq!(fc.args.len(), 2);
            }
            other => panic!("expected Function, got {other:?}"),
        }
    }

    #[test]
    fn parse_integer_literal() {
        let expr = parse_expr("42").unwrap();
        match expr {
            Expr::Literal(v) => assert!((v - 42.0).abs() < f64::EPSILON),
            other => panic!("expected Literal, got {other:?}"),
        }
    }

    #[test]
    fn parse_whitespace_tolerance() {
        let expr = parse_expr("  entropy( $.x )  >  0.5  ").unwrap();
        match expr {
            Expr::Compare(_, CompareOp::Gt, _) => {}
            other => panic!("expected Compare with Gt, got {other:?}"),
        }
    }

    #[test]
    fn parse_nested_path_with_multiple_arrays() {
        let expr = parse_expr("$.a[*].b[*].c").unwrap();
        match expr {
            Expr::Path(p) => {
                assert_eq!(p.segments.len(), 6);
                assert_eq!(p.segments[0], PatternSegment::Root);
                assert_eq!(p.segments[1], PatternSegment::Key("a".to_owned()));
                assert_eq!(p.segments[2], PatternSegment::ArrayWildcard);
                assert_eq!(p.segments[3], PatternSegment::Key("b".to_owned()));
                assert_eq!(p.segments[4], PatternSegment::ArrayWildcard);
                assert_eq!(p.segments[5], PatternSegment::Key("c".to_owned()));
            }
            other => panic!("expected Path, got {other:?}"),
        }
    }

    #[test]
    fn parse_error_empty_input() {
        let result = parse_expr("");
        assert!(result.is_err());
    }

    #[test]
    fn parse_root_path_only() {
        let expr = parse_expr("$").unwrap();
        match expr {
            Expr::Path(p) => {
                assert_eq!(p.segments.len(), 1);
                assert_eq!(p.segments[0], PatternSegment::Root);
            }
            other => panic!("expected Path, got {other:?}"),
        }
    }
}

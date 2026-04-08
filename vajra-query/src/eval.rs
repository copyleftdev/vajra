//! Expression evaluator for the Vajra query language.
//!
//! Evaluates parsed [`Expr`] trees against a [`Document`] and optional
//! [`StatsResult`], producing [`QueryResult`] values.

use vajra_stats::StatsResult;
use vajra_types::document::Document;
use vajra_types::path::{PathSegment, WildcardPath};

use crate::error::QueryError;
use crate::expr::{CompareOp, Expr, FunctionCall, PathPattern, PatternSegment};

/// Context for evaluating a query expression.
pub struct QueryContext<'a> {
    /// The document to query against.
    pub doc: &'a Document,
    /// Optional pre-computed statistics for function evaluation.
    pub stats: Option<&'a StatsResult>,
}

/// The result of evaluating a query expression.
#[derive(Debug, Clone)]
pub enum QueryResult {
    /// A set of matching paths with their extracted values.
    PathSet(Vec<(WildcardPath, Vec<serde_json::Value>)>),
    /// A single numeric value.
    Scalar(f64),
    /// A set of paths that matched a filter condition.
    FilteredPaths(Vec<WildcardPath>),
}

/// Evaluate an expression against a query context.
///
/// # Errors
///
/// Returns a [`QueryError`] if:
/// - A referenced path is not found
/// - A function requires stats but none are provided
/// - An unknown function is referenced
/// - A type error occurs during evaluation
pub fn evaluate(expr: &Expr, ctx: &QueryContext<'_>) -> Result<QueryResult, QueryError> {
    match expr {
        Expr::Path(pattern) => eval_path(pattern, ctx),
        Expr::Literal(v) => Ok(QueryResult::Scalar(*v)),
        Expr::Function(fc) => eval_function(fc, ctx),
        Expr::Compare(lhs, op, rhs) => eval_compare(lhs, *op, rhs, ctx),
    }
}

/// Match a `PathPattern` against the trie's wildcard paths, returning
/// all matching paths and their values extracted from the document.
fn eval_path(pattern: &PathPattern, ctx: &QueryContext<'_>) -> Result<QueryResult, QueryError> {
    let all_paths = ctx.doc.trie().all_paths();
    let matched: Vec<WildcardPath> = all_paths
        .into_iter()
        .filter(|wp| pattern_matches(pattern, wp))
        .collect();

    if matched.is_empty() {
        return Ok(QueryResult::PathSet(Vec::new()));
    }

    let mut results = Vec::new();
    for wp in matched {
        let values = extract_values(ctx.doc.value(), wp.segments(), 0);
        results.push((wp, values));
    }

    Ok(QueryResult::PathSet(results))
}

/// Check whether a `PathPattern` matches a concrete `WildcardPath`.
fn pattern_matches(pattern: &PathPattern, path: &WildcardPath) -> bool {
    let pat_segs = &pattern.segments;
    let path_segs = path.segments();

    match_segments(pat_segs, 0, path_segs, 0)
}

fn match_segments(pat: &[PatternSegment], pi: usize, path: &[PathSegment], si: usize) -> bool {
    // Both exhausted: match.
    if pi == pat.len() && si == path.len() {
        return true;
    }
    // Pattern exhausted but path remains: no match.
    if pi == pat.len() {
        return false;
    }

    match &pat[pi] {
        PatternSegment::Glob => {
            // Glob matches zero or more segments.
            // Try matching remaining pattern at every position.
            for i in si..=path.len() {
                if match_segments(pat, pi + 1, path, i) {
                    return true;
                }
            }
            false
        }
        PatternSegment::Root => {
            if si < path.len() && path[si] == PathSegment::Root {
                match_segments(pat, pi + 1, path, si + 1)
            } else {
                false
            }
        }
        PatternSegment::Key(k) => {
            if si < path.len() && path[si] == PathSegment::Key(k.clone()) {
                match_segments(pat, pi + 1, path, si + 1)
            } else {
                false
            }
        }
        PatternSegment::ArrayWildcard => {
            if si < path.len() && path[si] == PathSegment::ArrayWildcard {
                match_segments(pat, pi + 1, path, si + 1)
            } else {
                false
            }
        }
    }
}

/// Extract JSON values from a document at the given wildcard path segments.
fn extract_values(
    value: &serde_json::Value,
    segments: &[PathSegment],
    idx: usize,
) -> Vec<serde_json::Value> {
    if idx >= segments.len() {
        return vec![value.clone()];
    }

    match &segments[idx] {
        PathSegment::Root => extract_values(value, segments, idx + 1),
        PathSegment::Key(k) => {
            if let Some(child) = value.get(k.as_str()) {
                extract_values(child, segments, idx + 1)
            } else {
                Vec::new()
            }
        }
        PathSegment::ArrayWildcard => {
            if let Some(arr) = value.as_array() {
                let mut results = Vec::new();
                for element in arr {
                    results.extend(extract_values(element, segments, idx + 1));
                }
                results
            } else {
                Vec::new()
            }
        }
    }
}

/// Evaluate a function call.
fn eval_function(fc: &FunctionCall, ctx: &QueryContext<'_>) -> Result<QueryResult, QueryError> {
    match fc.name.as_str() {
        "entropy" => eval_stats_fn(fc, ctx, |ps| ps.entropy),
        "cardinality" => eval_stats_fn(fc, ctx, |ps| ps.cardinality as f64),
        "rarity" => eval_stats_fn(fc, ctx, |ps| ps.max_rarity),
        "null_rate" => eval_trie_fn(fc, ctx, |meta| meta.null_rate()),
        "instability" => eval_trie_fn(fc, ctx, |meta| meta.type_instability()),
        "count" => eval_trie_fn(fc, ctx, |meta| meta.count as f64),
        "depth" => eval_depth_fn(fc, ctx),
        _ => Err(QueryError::UnknownFunction {
            name: fc.name.clone(),
        }),
    }
}

/// Evaluate a stats-based function (entropy, cardinality, rarity).
fn eval_stats_fn(
    fc: &FunctionCall,
    ctx: &QueryContext<'_>,
    extractor: fn(&vajra_stats::PathStats) -> f64,
) -> Result<QueryResult, QueryError> {
    let path = require_path_arg(fc)?;
    let stats = ctx.stats.ok_or(QueryError::StatsRequired)?;

    // Find the matching wildcard path in the stats result.
    let wp = resolve_path_pattern(&path, ctx)?;
    let path_stats = stats
        .paths
        .get(&wp)
        .ok_or_else(|| QueryError::PathNotFound {
            path: path.as_str(),
        })?;

    Ok(QueryResult::Scalar(extractor(path_stats)))
}

/// Evaluate a trie-based function (null_rate, instability, count).
fn eval_trie_fn(
    fc: &FunctionCall,
    ctx: &QueryContext<'_>,
    extractor: fn(&vajra_types::trie::PathMetadata) -> f64,
) -> Result<QueryResult, QueryError> {
    let path = require_path_arg(fc)?;
    let wp = resolve_path_pattern(&path, ctx)?;

    let node = ctx
        .doc
        .trie()
        .get(&wp)
        .ok_or_else(|| QueryError::PathNotFound {
            path: path.as_str(),
        })?;

    Ok(QueryResult::Scalar(extractor(&node.metadata)))
}

/// Evaluate the depth() function.
fn eval_depth_fn(fc: &FunctionCall, ctx: &QueryContext<'_>) -> Result<QueryResult, QueryError> {
    let path = require_path_arg(fc)?;
    let wp = resolve_path_pattern(&path, ctx)?;
    Ok(QueryResult::Scalar(f64::from(wp.depth())))
}

/// Extract the first argument as a path pattern (most built-in functions
/// take a single path argument).
fn require_path_arg(fc: &FunctionCall) -> Result<PathPattern, QueryError> {
    if fc.args.is_empty() {
        return Err(QueryError::TypeError {
            message: format!("{}() requires at least one argument", fc.name),
        });
    }
    match &fc.args[0] {
        Expr::Path(p) => Ok(p.clone()),
        _ => Err(QueryError::TypeError {
            message: format!("{}() first argument must be a path", fc.name),
        }),
    }
}

/// Resolve a `PathPattern` to the first matching `WildcardPath` in the trie.
fn resolve_path_pattern(
    pattern: &PathPattern,
    ctx: &QueryContext<'_>,
) -> Result<WildcardPath, QueryError> {
    let all_paths = ctx.doc.trie().all_paths();
    all_paths
        .into_iter()
        .find(|wp| pattern_matches(pattern, wp))
        .ok_or_else(|| QueryError::PathNotFound {
            path: pattern.as_str(),
        })
}

/// Evaluate a comparison expression.
fn eval_compare(
    lhs: &Expr,
    op: CompareOp,
    rhs: &Expr,
    ctx: &QueryContext<'_>,
) -> Result<QueryResult, QueryError> {
    let lhs_result = evaluate(lhs, ctx)?;
    let rhs_result = evaluate(rhs, ctx)?;

    match (&lhs_result, &rhs_result) {
        (QueryResult::Scalar(l), QueryResult::Scalar(r)) => {
            let holds = compare_f64(*l, op, *r);
            if holds {
                // Return the scalar pair as a filtered result.
                // When both sides are scalar, return the matching paths
                // from the LHS function/path if the condition holds.
                Ok(QueryResult::Scalar(*l))
            } else {
                Ok(QueryResult::FilteredPaths(Vec::new()))
            }
        }
        (QueryResult::PathSet(paths), QueryResult::Scalar(threshold)) => {
            // Filter paths where extracted values compare with threshold.
            let mut matched = Vec::new();
            for (wp, _values) in paths {
                // For path-vs-scalar comparisons, we need the path's
                // aggregate value. Try stats or trie lookups.
                let val = path_aggregate_value(wp, ctx);
                if let Some(v) = val {
                    if compare_f64(v, op, *threshold) {
                        matched.push(wp.clone());
                    }
                }
            }
            Ok(QueryResult::FilteredPaths(matched))
        }
        _ => Err(QueryError::TypeError {
            message: "unsupported comparison operand types".to_owned(),
        }),
    }
}

/// Try to get an aggregate numeric value for a path (from count in the trie).
fn path_aggregate_value(wp: &WildcardPath, ctx: &QueryContext<'_>) -> Option<f64> {
    ctx.doc
        .trie()
        .get(wp)
        .map(|node| node.metadata.count as f64)
}

fn compare_f64(l: f64, op: CompareOp, r: f64) -> bool {
    match op {
        CompareOp::Gt => l > r,
        CompareOp::Lt => l < r,
        CompareOp::Gte => l >= r,
        CompareOp::Lte => l <= r,
        CompareOp::Eq => (l - r).abs() < f64::EPSILON,
        CompareOp::Ne => (l - r).abs() >= f64::EPSILON,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_expr;
    use vajra_stats::StatsAnalyzer;
    use vajra_types::traits::Analyzer;

    fn parse_doc(json: &str) -> Document {
        vajra_core::parse_str(json).unwrap()
    }

    fn make_ctx_with_stats<'a>(doc: &'a Document, stats: &'a StatsResult) -> QueryContext<'a> {
        QueryContext {
            doc,
            stats: Some(stats),
        }
    }

    fn make_ctx<'a>(doc: &'a Document) -> QueryContext<'a> {
        QueryContext { doc, stats: None }
    }

    #[test]
    fn path_lookup_existing() {
        let doc = parse_doc(r#"{"a": {"b": 42}}"#);
        let ctx = make_ctx(&doc);
        let expr = parse_expr("$.a.b").unwrap();
        let result = evaluate(&expr, &ctx).unwrap();
        match result {
            QueryResult::PathSet(ps) => {
                assert_eq!(ps.len(), 1);
                assert_eq!(ps[0].0.to_string(), "$.a.b");
                assert_eq!(ps[0].1, vec![serde_json::json!(42)]);
            }
            other => panic!("expected PathSet, got {other:?}"),
        }
    }

    #[test]
    fn path_lookup_nonexistent() {
        let doc = parse_doc(r#"{"a": 1}"#);
        let ctx = make_ctx(&doc);
        let expr = parse_expr("$.z.missing").unwrap();
        let result = evaluate(&expr, &ctx).unwrap();
        match result {
            QueryResult::PathSet(ps) => {
                assert!(ps.is_empty());
            }
            other => panic!("expected empty PathSet, got {other:?}"),
        }
    }

    #[test]
    fn entropy_returns_value() {
        let doc = parse_doc(r#"["x", "y", "x", "x", "y"]"#);
        let analyzer = StatsAnalyzer;
        let stats = analyzer.analyze(&doc).unwrap();
        let ctx = make_ctx_with_stats(&doc, &stats);
        let expr = parse_expr("entropy($[*])").unwrap();
        let result = evaluate(&expr, &ctx).unwrap();
        match result {
            QueryResult::Scalar(v) => {
                // Entropy for p=[3/5, 2/5]
                let expected =
                    -(3.0 / 5.0 * (3.0_f64 / 5.0).log2()) - (2.0 / 5.0 * (2.0_f64 / 5.0).log2());
                assert!((v - expected).abs() < 1e-8);
            }
            other => panic!("expected Scalar, got {other:?}"),
        }
    }

    #[test]
    fn cardinality_returns_value() {
        let doc = parse_doc(r#"["a", "b", "c", "a"]"#);
        let analyzer = StatsAnalyzer;
        let stats = analyzer.analyze(&doc).unwrap();
        let ctx = make_ctx_with_stats(&doc, &stats);
        let expr = parse_expr("cardinality($[*])").unwrap();
        let result = evaluate(&expr, &ctx).unwrap();
        match result {
            QueryResult::Scalar(v) => assert!((v - 3.0).abs() < f64::EPSILON),
            other => panic!("expected Scalar, got {other:?}"),
        }
    }

    #[test]
    fn null_rate_returns_value() {
        let doc = parse_doc(r#"[1, null, 2, null]"#);
        let ctx = make_ctx(&doc);
        let expr = parse_expr("null_rate($[*])").unwrap();
        let result = evaluate(&expr, &ctx).unwrap();
        match result {
            QueryResult::Scalar(v) => {
                // 2 nulls out of 4
                assert!((v - 0.5).abs() < 1e-8);
            }
            other => panic!("expected Scalar, got {other:?}"),
        }
    }

    #[test]
    fn instability_returns_value() {
        let doc = parse_doc(r#"[1, "two", 3]"#);
        let ctx = make_ctx(&doc);
        let expr = parse_expr("instability($[*])").unwrap();
        let result = evaluate(&expr, &ctx).unwrap();
        match result {
            QueryResult::Scalar(v) => {
                // 2 integers, 1 string => dominant=2/3, instability=1/3
                assert!(v > 0.0);
                assert!((v - 1.0 / 3.0).abs() < 1e-8);
            }
            other => panic!("expected Scalar, got {other:?}"),
        }
    }

    #[test]
    fn count_returns_value() {
        let doc = parse_doc(r#"[10, 20, 30]"#);
        let ctx = make_ctx(&doc);
        let expr = parse_expr("count($[*])").unwrap();
        let result = evaluate(&expr, &ctx).unwrap();
        match result {
            QueryResult::Scalar(v) => assert!((v - 3.0).abs() < f64::EPSILON),
            other => panic!("expected Scalar, got {other:?}"),
        }
    }

    #[test]
    fn depth_returns_value() {
        let doc = parse_doc(r#"{"a": {"b": {"c": 1}}}"#);
        let ctx = make_ctx(&doc);
        let expr = parse_expr("depth($.a.b.c)").unwrap();
        let result = evaluate(&expr, &ctx).unwrap();
        match result {
            QueryResult::Scalar(v) => assert!((v - 3.0).abs() < f64::EPSILON),
            other => panic!("expected Scalar, got {other:?}"),
        }
    }

    #[test]
    fn comparison_entropy_gt_zero() {
        let doc = parse_doc(r#"["x", "y", "x"]"#);
        let analyzer = StatsAnalyzer;
        let stats = analyzer.analyze(&doc).unwrap();
        let ctx = make_ctx_with_stats(&doc, &stats);
        let expr = parse_expr("entropy($[*]) > 0").unwrap();
        let result = evaluate(&expr, &ctx).unwrap();
        // With distinct values, entropy > 0, so the comparison succeeds.
        match result {
            QueryResult::Scalar(v) => assert!(v > 0.0),
            other => panic!("expected Scalar (comparison true), got {other:?}"),
        }
    }

    #[test]
    fn comparison_null_rate_filter() {
        let doc = parse_doc(r#"[null, null, 1]"#);
        let ctx = make_ctx(&doc);
        // null_rate($[*]) should be 2/3 ~= 0.666
        let expr = parse_expr("null_rate($[*]) > 0.5").unwrap();
        let result = evaluate(&expr, &ctx).unwrap();
        match result {
            QueryResult::Scalar(v) => {
                // Comparison holds, returns the LHS scalar
                assert!(v > 0.5);
            }
            other => panic!("expected Scalar (comparison true), got {other:?}"),
        }
    }

    #[test]
    fn stats_required_error() {
        let doc = parse_doc(r#"{"x": 1}"#);
        let ctx = make_ctx(&doc); // no stats
        let expr = parse_expr("entropy($.x)").unwrap();
        let result = evaluate(&expr, &ctx);
        assert!(result.is_err());
        match result {
            Err(QueryError::StatsRequired) => {}
            other => panic!("expected StatsRequired, got {other:?}"),
        }
    }

    #[test]
    fn unknown_function_error() {
        let doc = parse_doc(r#"{"x": 1}"#);
        let ctx = make_ctx(&doc);
        let expr = parse_expr("bogus($.x)").unwrap();
        let result = evaluate(&expr, &ctx);
        assert!(result.is_err());
        match result {
            Err(QueryError::UnknownFunction { name }) => assert_eq!(name, "bogus"),
            other => panic!("expected UnknownFunction, got {other:?}"),
        }
    }

    #[test]
    fn rarity_returns_value() {
        // ["a", "a", "a", "rare"] => min_count=1, total=4, rarity=-log2(1/4)=2.0
        let doc = parse_doc(r#"["a", "a", "a", "rare"]"#);
        let analyzer = StatsAnalyzer;
        let stats = analyzer.analyze(&doc).unwrap();
        let ctx = make_ctx_with_stats(&doc, &stats);
        let expr = parse_expr("rarity($[*])").unwrap();
        let result = evaluate(&expr, &ctx).unwrap();
        match result {
            QueryResult::Scalar(v) => assert!((v - 2.0).abs() < 1e-8),
            other => panic!("expected Scalar, got {other:?}"),
        }
    }

    #[test]
    fn path_with_array_values() {
        let doc = parse_doc(r#"{"items": [{"name": "a"}, {"name": "b"}]}"#);
        let ctx = make_ctx(&doc);
        let expr = parse_expr("$.items[*].name").unwrap();
        let result = evaluate(&expr, &ctx).unwrap();
        match result {
            QueryResult::PathSet(ps) => {
                assert_eq!(ps.len(), 1);
                assert_eq!(ps[0].0.to_string(), "$.items[*].name");
                assert_eq!(
                    ps[0].1,
                    vec![serde_json::json!("a"), serde_json::json!("b")]
                );
            }
            other => panic!("expected PathSet, got {other:?}"),
        }
    }

    #[test]
    fn comparison_false_returns_empty_filtered() {
        // entropy of constant path is 0; 0 > 999 is false
        let doc = parse_doc(r#"["x", "x", "x"]"#);
        let analyzer = StatsAnalyzer;
        let stats = analyzer.analyze(&doc).unwrap();
        let ctx = make_ctx_with_stats(&doc, &stats);
        let expr = parse_expr("entropy($[*]) > 999").unwrap();
        let result = evaluate(&expr, &ctx).unwrap();
        match result {
            QueryResult::FilteredPaths(ps) => assert!(ps.is_empty()),
            other => panic!("expected FilteredPaths (empty), got {other:?}"),
        }
    }
}

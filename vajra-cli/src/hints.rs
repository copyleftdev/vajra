//! Matching discovered relationships against domain relationship hints.
//!
//! Domain plugins declare hints — "in GitHub data, `reviewer`, `pr_author` and
//! `review_state` co-occur" — and `invariants` discovers relationships
//! empirically. Connecting the two gives two things neither has alone:
//!
//! - **confirmation**: a discovered relationship that a domain expected, which
//!   is stronger evidence than an unexplained correlation.
//! - **absence**: a relationship the domain expected, whose fields are present,
//!   that was *not* found. That is often the more interesting result — a merge
//!   gate that should tie `state` to `merged_by` and doesn't is a finding.
//!
//! Hints deliberately do **not** weight the information theory. Conditional
//! entropy is measured from the data; a hint is context laid alongside it, never
//! an adjustment to it.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use vajra_types::traits::RelationshipHint;

/// A hint matched against what was discovered.
#[derive(Debug, Clone, Serialize)]
pub struct HintOutcome {
    /// The hint's name.
    pub name: String,
    /// The relationship the domain expects.
    pub relationship: String,
    /// The hint's declared weight, as authored by the plugin.
    pub weight: f64,
    /// Document paths matching the hint's field patterns.
    pub matched_fields: Vec<String>,
    /// Whether a discovered relationship connected two of those fields.
    pub observed: bool,
    /// Strongest relationship strength found among the hint's fields.
    pub max_strength: f64,
}

/// Does a document path match a hint field pattern?
///
/// Patterns are authored as `**/name`, so matching is on the trailing key. That
/// keeps a hint portable across `$.pr.reviewer`, `$[*].reviewer` and
/// `$.items[*].pr.reviewer` without the plugin having to know the shape.
fn pattern_matches(pattern: &str, path: &str) -> bool {
    let key = pattern.rsplit('/').next().unwrap_or(pattern);
    if key.is_empty() {
        return false;
    }
    path.rsplit('.').next().is_some_and(|last| last == key)
}

/// Evaluate every hint against the document's paths and discovered relationships.
///
/// `relationships` is `(field_x, field_y, strength)`. A hint counts as observed
/// when some discovered relationship connects two *different* patterns of that
/// hint at or above `min_strength` — a field related to itself, or two paths
/// matching the same pattern, proves nothing about the hint.
pub fn evaluate_hints(
    hints: &[RelationshipHint],
    document_paths: &BTreeSet<String>,
    relationships: &[(String, String, f64)],
    min_strength: f64,
) -> Vec<HintOutcome> {
    let mut out = Vec::new();

    for hint in hints {
        // Testability: which of the hint's patterns reach a real document path?
        // A hint needs at least two present fields to say anything.
        //
        // Note the two path vocabularies in play. `invariants` reports fields
        // relative to each *record* (`$.reviewer`) while the document trie
        // reports them relative to the whole document (`$[*].reviewer`).
        // Everything here therefore matches on the trailing key rather than
        // intersecting the two sets, which would never match.
        let mut present: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for pattern in &hint.fields {
            let hit: BTreeSet<&str> = document_paths
                .iter()
                .filter(|p| pattern_matches(pattern, p))
                .map(String::as_str)
                .collect();
            if !hit.is_empty() {
                present.insert(pattern.as_str(), hit);
            }
        }
        if present.len() < 2 {
            continue;
        }

        let matched_fields: BTreeSet<&str> =
            present.values().flat_map(|s| s.iter().copied()).collect();

        let mut max_strength = 0.0_f64;
        let mut observed = false;
        for (x, y, strength) in relationships {
            // Which of this hint's present patterns does each side match?
            let x_pat: Vec<&str> = present
                .keys()
                .copied()
                .filter(|pat| pattern_matches(pat, x))
                .collect();
            let y_pat: Vec<&str> = present
                .keys()
                .copied()
                .filter(|pat| pattern_matches(pat, y))
                .collect();
            // Must span two *distinct* patterns: a field related to another
            // field matching the same pattern proves nothing about the hint.
            let spans = x_pat.iter().any(|xp| y_pat.iter().any(|yp| xp != yp));
            if !spans {
                continue;
            }
            if *strength > max_strength {
                max_strength = *strength;
            }
            if *strength >= min_strength {
                observed = true;
            }
        }

        out.push(HintOutcome {
            name: hint.name.clone(),
            relationship: format!("{:?}", hint.relationship),
            weight: hint.weight,
            matched_fields: matched_fields.into_iter().map(str::to_owned).collect(),
            observed,
            max_strength,
        });
    }

    // Unobserved first — an expected relationship that is missing is the more
    // actionable result. Then by name, so ordering is fully specified.
    out.sort_by(|a, b| {
        a.observed
            .cmp(&b.observed)
            .then_with(|| a.name.cmp(&b.name))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use vajra_types::traits::RelationshipType;

    fn hint(name: &str, fields: &[&str]) -> RelationshipHint {
        RelationshipHint {
            name: name.to_owned(),
            fields: fields.iter().map(|f| (*f).to_owned()).collect(),
            relationship: RelationshipType::CoOccurrence,
            weight: 0.9,
        }
    }

    fn paths(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn pattern_matches_trailing_key_at_any_depth() {
        assert!(pattern_matches("**/reviewer", "$.pr.reviewer"));
        assert!(pattern_matches("**/reviewer", "$[*].reviewer"));
        assert!(pattern_matches("**/reviewer", "$.items[*].pr.reviewer"));
        assert!(!pattern_matches("**/reviewer", "$.reviewer_id"));
        assert!(!pattern_matches("**/reviewer", "$.pr.author"));
    }

    #[test]
    fn hint_is_observed_when_a_relationship_spans_two_of_its_fields() {
        let hints = [hint("gate", &["**/state", "**/merged_by"])];
        let p = paths(&["$[*].state", "$[*].merged_by"]);
        let rels = vec![("$[*].state".to_owned(), "$[*].merged_by".to_owned(), 0.8)];
        let out = evaluate_hints(&hints, &p, &rels, 0.5);
        assert_eq!(out.len(), 1);
        assert!(out[0].observed);
        assert!((out[0].max_strength - 0.8).abs() < 1e-9);
        assert_eq!(out[0].matched_fields.len(), 2);
    }

    /// The point of the feature: fields present, relationship absent.
    #[test]
    fn hint_is_unobserved_when_the_relationship_is_missing() {
        let hints = [hint("gate", &["**/state", "**/merged_by"])];
        let p = paths(&["$[*].state", "$[*].merged_by"]);
        let out = evaluate_hints(&hints, &p, &[], 0.5);
        assert_eq!(out.len(), 1);
        assert!(!out[0].observed, "no relationship was discovered");
        assert!(out[0].max_strength.abs() < 1e-9);
    }

    #[test]
    fn weak_relationship_does_not_count_as_observed() {
        let hints = [hint("gate", &["**/state", "**/merged_by"])];
        let p = paths(&["$[*].state", "$[*].merged_by"]);
        let rels = vec![("$[*].state".to_owned(), "$[*].merged_by".to_owned(), 0.1)];
        let out = evaluate_hints(&hints, &p, &rels, 0.5);
        assert!(!out[0].observed, "0.1 is below the 0.5 threshold");
        // But the strength found is still reported, so the near-miss is visible.
        assert!((out[0].max_strength - 0.1).abs() < 1e-9);
    }

    /// A hint whose fields are mostly absent is untestable and must be skipped
    /// rather than reported as a failure.
    #[test]
    fn hint_needs_two_present_fields_to_be_evaluated() {
        let hints = [hint("gate", &["**/state", "**/merged_by"])];
        let out = evaluate_hints(&hints, &paths(&["$[*].state"]), &[], 0.5);
        assert!(out.is_empty(), "one field present is not testable");
        let none = evaluate_hints(&hints, &paths(&["$[*].unrelated"]), &[], 0.5);
        assert!(none.is_empty());
    }

    /// Two paths matching the *same* pattern prove nothing about the hint.
    #[test]
    fn relationship_within_one_pattern_does_not_confirm() {
        let hints = [hint("gate", &["**/state", "**/merged_by"])];
        let p = paths(&["$.a.state", "$.b.state", "$[*].merged_by"]);
        let rels = vec![("$.a.state".to_owned(), "$.b.state".to_owned(), 0.99)];
        let out = evaluate_hints(&hints, &p, &rels, 0.5);
        assert!(!out[0].observed, "state<->state spans one pattern, not two");
    }

    #[test]
    fn unobserved_hints_are_listed_first() {
        let hints = [
            hint("zzz_seen", &["**/a", "**/b"]),
            hint("aaa_missing", &["**/c", "**/d"]),
        ];
        let p = paths(&["$.a", "$.b", "$.c", "$.d"]);
        let rels = vec![("$.a".to_owned(), "$.b".to_owned(), 0.9)];
        let out = evaluate_hints(&hints, &p, &rels, 0.5);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].name, "aaa_missing", "missing first");
        assert!(!out[0].observed);
        assert!(out[1].observed);
    }

    /// Regression: `invariants` names fields relative to each record
    /// (`$.reviewer`) while the document trie names them relative to the whole
    /// document (`$[*].reviewer`). Matching must bridge that, or hints silently
    /// never fire.
    #[test]
    fn bridges_record_and_document_path_vocabularies() {
        let hints = [hint("review_network", &["**/reviewer", "**/review_state"])];
        // Document paths as the trie reports them.
        let p = paths(&["$[*].reviewer", "$[*].review_state"]);
        // Relationships as `invariants` reports them.
        let rels = vec![(
            "$.reviewer".to_owned(),
            "$.review_state".to_owned(),
            1.0_f64,
        )];
        let out = evaluate_hints(&hints, &p, &rels, 0.25);
        assert_eq!(out.len(), 1);
        assert!(
            out[0].observed,
            "record-relative relationship must match document-relative paths"
        );
        assert!((out[0].max_strength - 1.0).abs() < 1e-9);
    }

    #[test]
    fn is_deterministic() {
        let hints = [hint("b", &["**/x", "**/y"]), hint("a", &["**/x", "**/y"])];
        let p = paths(&["$.x", "$.y"]);
        let rels = vec![("$.x".to_owned(), "$.y".to_owned(), 0.7)];
        let one = evaluate_hints(&hints, &p, &rels, 0.5);
        let two = evaluate_hints(&hints, &p, &rels, 0.5);
        let names =
            |v: &[HintOutcome]| -> Vec<String> { v.iter().map(|h| h.name.clone()).collect() };
        assert_eq!(names(&one), names(&two));
        assert_eq!(names(&one), vec!["a", "b"]);
    }
}

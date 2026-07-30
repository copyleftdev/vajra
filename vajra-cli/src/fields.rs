//! Field-selector defaults resolved against the document.
//!
//! Every reader has its own vocabulary. The git reader emits `author_name` and
//! `subject`; GitHub ingest emits `author` and `message`. A single static clap
//! default cannot serve both, so pointing `governance` at a git repository used
//! to fail with `author field '$.author' not found` even though the records
//! carried an author under a different name.
//!
//! When the caller does not pass a selector explicitly, resolve it against the
//! fields the records actually carry. An explicit selector is always honoured
//! verbatim — resolution never overrides the user.

use serde_json::Value;

/// How many records to sample when deciding whether a candidate is present.
/// Bounded so resolution costs the same on a 10-record and a 10-million-record
/// document.
const SAMPLE: usize = 64;

/// Candidate selectors for one logical field, in preference order.
pub struct Candidates {
    /// The flag that overrides resolution, for use in diagnostics.
    pub flag: &'static str,
    /// Selectors to try, most conventional first. Never empty.
    pub options: &'static [&'static str],
}

impl Candidates {
    /// The selector reported when nothing matches, so error messages name a
    /// stable field rather than varying with the input.
    fn primary(&self) -> &'static str {
        // `options` is a non-empty constant in every instance below.
        self.options.first().copied().unwrap_or("$.author")
    }
}

/// Author/contributor identity.
pub const AUTHOR: Candidates = Candidates {
    flag: "--author-field",
    options: &["$.author", "$.author_name"],
};

/// Commit or issue message.
pub const MESSAGE: Candidates = Candidates {
    flag: "--message-field",
    options: &["$.message", "$.subject", "$.title"],
};

/// Outcome of resolving one selector.
pub struct Resolved {
    /// The selector to use.
    pub selector: String,
    /// Set when resolution chose a candidate other than the primary one, so the
    /// caller can say which field it picked and why.
    pub note: Option<String>,
}

/// Resolve a field selector.
///
/// Returns `explicit` unchanged when the user passed one. Otherwise picks the
/// first candidate present in every sampled record, falling back to the primary
/// candidate so the caller's own missing-field error names a stable selector.
pub fn resolve(explicit: Option<&str>, candidates: &Candidates, records: &[Value]) -> Resolved {
    if let Some(sel) = explicit {
        return Resolved {
            selector: sel.to_owned(),
            note: None,
        };
    }

    let sample = &records[..records.len().min(SAMPLE)];
    for option in candidates.options {
        if !sample.is_empty() && sample.iter().all(|r| has_field(r, option)) {
            let note = (*option != candidates.primary()).then(|| {
                format!(
                    "using {} {option} (records carry no {}); override with {}",
                    candidates.flag.trim_start_matches("--").replace('-', " "),
                    candidates.primary(),
                    candidates.flag,
                )
            });
            return Resolved {
                selector: (*option).to_owned(),
                note,
            };
        }
    }

    Resolved {
        selector: candidates.primary().to_owned(),
        note: None,
    }
}

/// Whether a record carries a non-null value at a `$.name` selector.
fn has_field(record: &Value, selector: &str) -> bool {
    let key = selector.trim_start_matches("$.");
    record
        .get(key)
        .is_some_and(|v| !v.is_null() && !v.is_object() && !v.is_array())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn git_records() -> Vec<Value> {
        vec![
            json!({"author_name": "Alice", "author_email": "a@x", "subject": "feat", "date": "2026-01-01"}),
            json!({"author_name": "Bob", "author_email": "b@x", "subject": "fix", "date": "2026-01-02"}),
        ]
    }

    fn github_records() -> Vec<Value> {
        vec![
            json!({"author": "Alice", "message": "feat", "date": "2026-01-01"}),
            json!({"author": "Bob", "message": "fix", "date": "2026-01-02"}),
        ]
    }

    #[test]
    fn explicit_selector_wins() {
        let r = resolve(Some("$.custom"), &AUTHOR, &git_records());
        assert_eq!(r.selector, "$.custom");
        assert!(r.note.is_none(), "an explicit selector needs no note");
    }

    #[test]
    fn git_records_resolve_to_author_name() {
        let r = resolve(None, &AUTHOR, &git_records());
        assert_eq!(r.selector, "$.author_name");
        assert!(
            r.note.is_some(),
            "picking a non-primary candidate must be reported"
        );
    }

    #[test]
    fn github_records_resolve_to_author() {
        let r = resolve(None, &AUTHOR, &github_records());
        assert_eq!(r.selector, "$.author");
        assert!(r.note.is_none(), "the primary candidate needs no note");
    }

    #[test]
    fn git_records_resolve_message_to_subject() {
        let r = resolve(None, &MESSAGE, &git_records());
        assert_eq!(r.selector, "$.subject");
    }

    #[test]
    fn unresolvable_falls_back_to_primary() {
        let records = vec![json!({"who": "Alice"})];
        let r = resolve(None, &AUTHOR, &records);
        assert_eq!(
            r.selector, "$.author",
            "fallback must name a stable selector for the error path"
        );
        assert!(r.note.is_none());
    }

    #[test]
    fn a_candidate_missing_from_one_record_is_not_chosen() {
        let records = vec![
            json!({"author_name": "Alice", "author": "Alice"}),
            json!({"author_name": "Bob"}),
        ];
        let r = resolve(None, &AUTHOR, &records);
        assert_eq!(
            r.selector, "$.author_name",
            "$.author is absent from record 2, so it cannot be the default"
        );
    }

    #[test]
    fn object_valued_field_is_not_a_scalar_candidate() {
        let records = vec![json!({"author": {"login": "alice"}, "author_name": "Alice"})];
        let r = resolve(None, &AUTHOR, &records);
        assert_eq!(
            r.selector, "$.author_name",
            "a nested object is not usable as an identity selector"
        );
    }

    #[test]
    fn empty_records_fall_back_to_primary() {
        let r = resolve(None, &AUTHOR, &[]);
        assert_eq!(r.selector, "$.author");
    }
}

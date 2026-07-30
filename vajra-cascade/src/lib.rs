//! Temporal cause-effect chain detection for structured event data.
mod error;
mod extract;
mod types;
pub use error::CascadeError;
use std::collections::BTreeMap;
pub use types::{CascadeChain, CascadeConfig, CascadeResult, HotEntity, TriggerResponse};
pub fn detect_cascades(
    records: &[serde_json::Value],
    config: &CascadeConfig,
) -> Result<CascadeResult, CascadeError> {
    if records.is_empty() {
        return Ok(CascadeResult {
            cascades: Vec::new(),
            hot_entities: Vec::new(),
            cascade_rate: 0.0,
            self_fix_rate: Some(0.0),
            self_fix_rate_note: None,
            total_events: 0,
        });
    }
    let mut groups: BTreeMap<String, Vec<&serde_json::Value>> = BTreeMap::new();
    for record in records {
        let entity = extract::extract_field(record, &config.entity_field).unwrap_or_default();
        if entity.is_empty() {
            continue;
        }
        groups.entry(entity).or_default().push(record);
    }
    let mut all_cascades: Vec<CascadeChain> = Vec::new();
    let mut entity_stats: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for (entity, mut group) in groups {
        group.sort_by(|a, b| {
            let ta = extract::extract_field(a, &config.time_field).unwrap_or_default();
            let tb = extract::extract_field(b, &config.time_field).unwrap_or_default();
            ta.cmp(&tb)
        });
        let total_in_group = group.len();
        let mut cascade_count = 0usize;
        let mut i = 0;
        while i + 1 < group.len() {
            let ev = extract::extract_field(group[i], &config.event_field).unwrap_or_default();
            if !config.is_response(&ev) {
                let nev =
                    extract::extract_field(group[i + 1], &config.event_field).unwrap_or_default();
                if config.is_response(&nev) {
                    let ta = ea(group[i]);
                    let ra = ea(group[i + 1]);
                    let sa = ta == ra && !ta.is_empty();
                    let tt =
                        extract::extract_field(group[i], &config.time_field).unwrap_or_default();
                    let rt = extract::extract_field(group[i + 1], &config.time_field)
                        .unwrap_or_default();
                    all_cascades.push(CascadeChain {
                        entity: entity.clone(),
                        trigger: TriggerResponse {
                            value: ev,
                            author: ta,
                            time: tt,
                        },
                        response: TriggerResponse {
                            value: nev,
                            author: ra,
                            time: rt,
                        },
                        same_author: sa,
                    });
                    cascade_count += 1;
                    i += 2;
                    continue;
                }
            }
            i += 1;
        }
        entity_stats.insert(entity, (total_in_group, cascade_count));
    }
    let te = records.len();
    let tc = all_cascades.len();
    #[allow(clippy::cast_precision_loss)]
    let cr = if te > 0 { tc as f64 / te as f64 } else { 0.0 };
    let sfc = all_cascades.iter().filter(|c| c.same_author).count();
    #[allow(clippy::cast_precision_loss)]
    let sfr = if tc > 0 { sfc as f64 / tc as f64 } else { 0.0 };
    // Grouping by the author makes every cascade same-author by construction,
    // so the rate is 1.0 for any input and measures nothing. Report it absent
    // rather than report a constant. See #92.
    let (sfr, sfr_note) = if entity_selects_author(&config.entity_field) {
        (
            None,
            Some(format!(
                "undefined: --entity-field '{}' selects the author, so every cascade is same-author by construction",
                config.entity_field
            )),
        )
    } else {
        (Some(sfr), None)
    };
    let mut he: Vec<HotEntity> = entity_stats
        .into_iter()
        .filter(|(_, (_, c))| *c > 0)
        .map(|(e, (t, c))| {
            #[allow(clippy::cast_precision_loss)]
            let r = c as f64 / t as f64;
            HotEntity {
                entity: e,
                total: t,
                cascades: c,
                cascade_ratio: r,
                cascade_ratio_lower_bound: wilson_lower_bound(c, t),
            }
        })
        .collect();
    // Ranked by the lower bound, not the raw ratio: at total=2 the ratio can
    // only be 0, 0.5 or 1, so it ties with or beats every well-supported entity
    // by construction. See #93.
    he.sort_by(|a, b| {
        b.cascade_ratio_lower_bound
            .partial_cmp(&a.cascade_ratio_lower_bound)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.entity.cmp(&b.entity))
    });
    Ok(CascadeResult {
        cascades: all_cascades,
        hot_entities: he,
        cascade_rate: cr,
        self_fix_rate: sfr,
        self_fix_rate_note: sfr_note,
        total_events: te,
    })
}

/// Author keys [`ea`] consults, in order.
const AUTHOR_KEYS: [&str; 4] = ["author", "committer", "user", "name"];

/// Whether the entity selector picks the same value the author lookup does.
///
/// Compared on the trailing key, so `$.author`, `$[*].author` and `author` all
/// match — the document- and record-relative vocabularies both reach here.
fn entity_selects_author(entity_field: &str) -> bool {
    let key = entity_field
        .rsplit('.')
        .next()
        .unwrap_or(entity_field)
        .trim_end_matches("[*]");
    AUTHOR_KEYS.contains(&key)
}

/// Wilson score interval lower bound for `successes` of `total`, at 95%.
///
/// Chosen over the raw ratio because it needs no tuned support threshold: the
/// bound falls automatically as evidence thins, so 1-of-2 (0.095) ranks below
/// 7-of-19 (0.192) without a cutoff anyone has to justify.
#[allow(clippy::cast_precision_loss)]
fn wilson_lower_bound(successes: usize, total: usize) -> f64 {
    if total == 0 {
        return 0.0;
    }
    // Two-sided 95% normal quantile.
    const Z: f64 = 1.959_963_984_540_054;
    let n = total as f64;
    let p = successes as f64 / n;
    let z2 = Z * Z;
    let denominator = 1.0 + z2 / n;
    let centre = p + z2 / (2.0 * n);
    let margin = Z * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt();
    ((centre - margin) / denominator).max(0.0)
}
fn ea(r: &serde_json::Value) -> String {
    // Shares AUTHOR_KEYS with entity_selects_author so the two cannot drift:
    // a key added here but not there would silently restore the tautology.
    for k in &AUTHOR_KEYS {
        if let Some(v) = r.get(k) {
            if let Some(s) = v.as_str() {
                return s.to_owned();
            }
        }
    }
    String::new()
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn dc() -> CascadeConfig {
        CascadeConfig::default()
    }
    fn er() -> CascadeResult {
        CascadeResult {
            cascades: Vec::new(),
            hot_entities: Vec::new(),
            cascade_rate: 0.0,
            self_fix_rate: Some(0.0),
            self_fix_rate_note: None,
            total_events: 0,
        }
    }
    /// The defect from #93: ranked by raw ratio, an entity touched twice
    /// outranks one touched nineteen times, because 1/2 > 7/19.
    #[test]
    fn support_outranks_ratio() {
        let mut recs = Vec::new();
        // thin.rs: 1 cascade out of 2 events -> ratio 0.50
        recs.push(json!({"file":"thin.rs","date":"2026-01-01","intent":"feat"}));
        recs.push(json!({"file":"thin.rs","date":"2026-01-02","intent":"fix"}));
        // thick.rs: 7 cascades out of 19 events -> ratio 0.368
        for i in 0..7 {
            recs.push(
                json!({"file":"thick.rs","date":format!("2026-02-{:02}", i*2+1),"intent":"feat"}),
            );
            recs.push(
                json!({"file":"thick.rs","date":format!("2026-02-{:02}", i*2+2),"intent":"fix"}),
            );
        }
        for i in 0..5 {
            recs.push(
                json!({"file":"thick.rs","date":format!("2026-03-{:02}", i+1),"intent":"refactor"}),
            );
        }
        let r = detect_cascades(&recs, &dc()).unwrap_or_else(|_| er());
        let thick = r
            .hot_entities
            .iter()
            .find(|h| h.entity == "thick.rs")
            .expect("thick.rs");
        let thin = r
            .hot_entities
            .iter()
            .find(|h| h.entity == "thin.rs")
            .expect("thin.rs");
        assert_eq!((thick.total, thick.cascades), (19, 7));
        assert_eq!((thin.total, thin.cascades), (2, 1));
        assert!(
            thin.cascade_ratio > thick.cascade_ratio,
            "the raw ratio must still favour the thin entity, or the test proves nothing"
        );
        assert_eq!(
            r.hot_entities[0].entity, "thick.rs",
            "ranking must follow the lower bound, not the ratio"
        );
    }

    #[test]
    fn wilson_bound_is_below_the_ratio_and_grows_with_support() {
        // Hand-computed at 95%: 1-of-2 -> 0.0945, 7-of-19 -> 0.1915.
        assert!((wilson_lower_bound(1, 2) - 0.094_5).abs() < 1e-3);
        assert!((wilson_lower_bound(7, 19) - 0.191_5).abs() < 1e-3);
        // Same ratio, more evidence -> a tighter, higher bound.
        assert!(wilson_lower_bound(50, 100) > wilson_lower_bound(5, 10));
        assert!(wilson_lower_bound(5, 10) > wilson_lower_bound(1, 2));
        // Never above the point estimate, never below zero.
        assert!(wilson_lower_bound(1, 1) < 1.0);
        assert!(wilson_lower_bound(0, 10) >= 0.0);
        assert!((wilson_lower_bound(0, 0) - 0.0).abs() < f64::EPSILON);
    }

    /// The defect from #92: grouping by author makes every cascade
    /// same-author by construction, so the rate is 1.0 for any input.
    #[test]
    fn self_fix_rate_is_absent_when_the_entity_is_the_author() {
        let recs = vec![
            json!({"author":"alice","file":"a.rs","date":"2026-01-01","intent":"feat"}),
            json!({"author":"alice","file":"a.rs","date":"2026-01-02","intent":"fix"}),
            json!({"author":"bob","file":"b.rs","date":"2026-01-03","intent":"feat"}),
            json!({"author":"bob","file":"b.rs","date":"2026-01-04","intent":"fix"}),
        ];
        let by_author = CascadeConfig {
            entity_field: "$.author".to_owned(),
            ..CascadeConfig::default()
        };
        let r = detect_cascades(&recs, &by_author).unwrap_or_else(|_| er());
        assert!(!r.cascades.is_empty(), "the fixture must produce cascades");
        assert!(
            r.self_fix_rate.is_none(),
            "a rate that is 1.0 by construction must not be reported as a measurement"
        );
        assert!(r
            .self_fix_rate_note
            .as_deref()
            .unwrap_or_default()
            .contains("$.author"));

        // The same data grouped by file yields a real number.
        let by_file = CascadeConfig {
            entity_field: "$.file".to_owned(),
            ..CascadeConfig::default()
        };
        let r = detect_cascades(&recs, &by_file).unwrap_or_else(|_| er());
        assert!(r.self_fix_rate.is_some());
        assert!(r.self_fix_rate_note.is_none());
    }

    #[test]
    fn the_author_collision_is_detected_in_both_path_vocabularies() {
        for field in ["author", "$.author", "$[*].author", "$.committer", "$.user"] {
            assert!(
                entity_selects_author(field),
                "{field} selects the author and must disable self_fix_rate"
            );
        }
        for field in ["$.file", "$.entity", "$[*].path", "$.author_email"] {
            assert!(
                !entity_selects_author(field),
                "{field} does not select the author"
            );
        }
    }

    #[test]
    fn empty_input() {
        let r = detect_cascades(&[], &dc()).unwrap_or_else(|_| er());
        assert_eq!(r.total_events, 0);
        assert!(r.cascades.is_empty());
    }
    #[test]
    fn single_record() {
        let r = detect_cascades(
            &[json!({"file":"a.rs","date":"2024-01-01","intent":"feat","author":"A"})],
            &dc(),
        )
        .unwrap_or_else(|_| er());
        assert_eq!(r.total_events, 1);
        assert!(r.cascades.is_empty());
    }
    #[test]
    fn basic_cascade() {
        let recs = vec![
            json!({"file":"src/E.ts","date":"2024-01-01","intent":"feat","author":"Tim"}),
            json!({"file":"src/E.ts","date":"2024-01-02","intent":"fix","author":"Max"}),
            json!({"file":"src/E.ts","date":"2024-01-03","intent":"refactor","author":"Tim"}),
            json!({"file":"src/E.ts","date":"2024-01-04","intent":"fix","author":"Tim"}),
            json!({"file":"src/O.ts","date":"2024-01-01","intent":"feat","author":"Alice"}),
            json!({"file":"src/O.ts","date":"2024-01-02","intent":"feat","author":"Bob"}),
        ];
        let r = detect_cascades(&recs, &dc()).unwrap_or_else(|_| er());
        assert_eq!(r.cascades.len(), 2);
        assert!(!r.cascades[0].same_author);
        assert!(r.cascades[1].same_author);
        assert!((r.cascade_rate - 2.0 / 6.0).abs() < f64::EPSILON);
    }
    #[test]
    fn no_cascades() {
        let recs = vec![
            json!({"file":"a.rs","date":"2024-01-01","intent":"feat","author":"A"}),
            json!({"file":"a.rs","date":"2024-01-02","intent":"refactor","author":"B"}),
        ];
        let r = detect_cascades(&recs, &dc()).unwrap_or_else(|_| er());
        assert!(r.cascades.is_empty());
    }
    #[test]
    fn all_cascades() {
        let recs = vec![
            json!({"file":"x.rs","date":"2024-01-01","intent":"feat","author":"A"}),
            json!({"file":"x.rs","date":"2024-01-02","intent":"fix","author":"A"}),
            json!({"file":"x.rs","date":"2024-01-03","intent":"feat","author":"A"}),
            json!({"file":"x.rs","date":"2024-01-04","intent":"revert","author":"A"}),
        ];
        let r = detect_cascades(&recs, &dc()).unwrap_or_else(|_| er());
        assert_eq!(r.cascades.len(), 2);
        assert!((r.self_fix_rate.unwrap_or(0.0) - 1.0).abs() < f64::EPSILON);
    }
    #[test]
    fn revert_response() {
        let recs = vec![
            json!({"file":"z.rs","date":"2024-01-01","intent":"feat","author":"A"}),
            json!({"file":"z.rs","date":"2024-01-02","intent":"revert","author":"B"}),
        ];
        let r = detect_cascades(&recs, &dc()).unwrap_or_else(|_| er());
        assert_eq!(r.cascades.len(), 1);
        assert!(!r.cascades[0].same_author);
    }
    #[test]
    fn missing_fields() {
        let recs = vec![
            json!({"date":"2024-01-01","intent":"feat"}),
            json!({"file":"a.rs","date":"2024-01-01","intent":"feat","author":"A"}),
            json!({"file":"a.rs","date":"2024-01-02","intent":"fix","author":"B"}),
        ];
        let r = detect_cascades(&recs, &dc()).unwrap_or_else(|_| er());
        assert_eq!(r.cascades.len(), 1);
    }
    #[test]
    fn deterministic() {
        let recs = vec![
            json!({"file":"b.rs","date":"2024-01-01","intent":"feat","author":"A"}),
            json!({"file":"b.rs","date":"2024-01-02","intent":"fix","author":"B"}),
            json!({"file":"a.rs","date":"2024-01-01","intent":"feat","author":"C"}),
            json!({"file":"a.rs","date":"2024-01-02","intent":"fix","author":"D"}),
        ];
        let r = detect_cascades(&recs, &dc()).unwrap_or_else(|_| er());
        assert_eq!(r.cascades[0].entity, "a.rs");
    }
    #[test]
    fn nested() {
        let cfg = CascadeConfig {
            entity_field: "m.f".to_owned(),
            time_field: "m.d".to_owned(),
            event_field: "m.i".to_owned(),
            trigger_values: Vec::new(),
            response_values: vec!["fix".to_owned()],
        };
        let recs = vec![
            json!({"m":{"f":"x.rs","d":"2024-01-01","i":"feat"},"author":"A"}),
            json!({"m":{"f":"x.rs","d":"2024-01-02","i":"fix"},"author":"B"}),
        ];
        let r = detect_cascades(&recs, &cfg).unwrap_or_else(|_| er());
        assert_eq!(r.cascades.len(), 1);
    }
    #[cfg(test)]
    mod prop_tests {
        use super::*;
        use proptest::prelude::*;
        proptest! { #[test] fn rate_bounded(intents in proptest::collection::vec(prop_oneof![Just("feat"),Just("fix"),Just("refactor"),Just("revert")], 0..30), files in proptest::collection::vec(prop_oneof![Just("a.rs"),Just("b.rs")], 0..30)) {
        let len = intents.len().min(files.len()); let recs: Vec<serde_json::Value> = (0..len).map(|i| json!({"file":files[i],"date":format!("2024-01-{:02}",(i%31)+1),"intent":intents[i],"author":"x"})).collect();
        let r = detect_cascades(&recs, &dc()).unwrap_or_else(|_| er()); prop_assert!(r.cascade_rate >= 0.0 && r.cascade_rate <= 1.0); prop_assert!(r.self_fix_rate.is_none_or(|v| (0.0..=1.0).contains(&v)));
        for h in &r.hot_entities { prop_assert!(h.cascade_ratio_lower_bound >= 0.0 && h.cascade_ratio_lower_bound <= h.cascade_ratio + 1e-9); } } }
    }
}

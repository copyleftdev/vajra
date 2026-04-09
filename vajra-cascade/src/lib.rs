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
            self_fix_rate: 0.0,
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
            }
        })
        .collect();
    he.sort_by(|a, b| {
        b.cascade_ratio
            .partial_cmp(&a.cascade_ratio)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.entity.cmp(&b.entity))
    });
    Ok(CascadeResult {
        cascades: all_cascades,
        hot_entities: he,
        cascade_rate: cr,
        self_fix_rate: sfr,
        total_events: te,
    })
}
fn ea(r: &serde_json::Value) -> String {
    for k in &["author", "committer", "user", "name"] {
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
            self_fix_rate: 0.0,
            total_events: 0,
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
        assert!((r.self_fix_rate - 1.0).abs() < f64::EPSILON);
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
        let r = detect_cascades(&recs, &dc()).unwrap_or_else(|_| er()); prop_assert!(r.cascade_rate >= 0.0 && r.cascade_rate <= 1.0); prop_assert!(r.self_fix_rate >= 0.0 && r.self_fix_rate <= 1.0); } }
    }
}

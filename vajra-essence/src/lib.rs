//! Dataset essence extraction and compact representation.
//!
//! This crate builds on the analysis results from `vajra-stats`,
//! `vajra-anomaly`, and `vajra-fingerprint` to produce a scored,
//! ranked summary of a document's most notable characteristics.

pub mod builder;
pub mod chunking;
pub mod compact_ai;
pub mod config;
pub mod markdown;
pub mod profiles;
pub mod render;
pub mod tokens;

pub use builder::EssenceBuilder;
pub use chunking::{chunk_essence, ChunkContext, EssenceChunk};
pub use compact_ai::render_compact_ai;
pub use config::{load_profiles_from_file, load_profiles_from_toml, CustomProfile};
pub use markdown::render_markdown;
pub use profiles::{AiProfile, AuditorProfile, EngineerProfile, FraudProfile, StaffProfile};
pub use render::{render_json, render_text};
pub use tokens::{estimate_json_tokens, estimate_tokens};

#[cfg(test)]
mod tests {
    use super::*;
    use vajra_types::traits::{ConcernProfile, OutputFormat};

    fn parse_doc(json: &str) -> vajra_types::document::Document {
        vajra_core::parse_str(json).unwrap_or_else(|e| {
            panic!("parse failed: {e}");
        })
    }

    #[test]
    fn staff_profile_renders_narrative_sections() {
        let doc = parse_doc(r#"{"name": "test", "values": [1, 2, 3]}"#);
        let profile = StaffProfile;
        let essence = EssenceBuilder::new(&doc, &profile).build();
        let text = profile
            .render(&essence, OutputFormat::Text)
            .unwrap_or_else(|e| panic!("{e}"));

        assert!(
            text.contains("Document Summary"),
            "staff text should contain 'Document Summary'"
        );
        assert!(
            text.contains("What Stands Out"),
            "staff text should contain 'What Stands Out'"
        );
        assert!(
            text.contains("What This Likely Means"),
            "staff text should contain 'What This Likely Means'"
        );
    }

    #[test]
    fn engineer_profile_renders_technical_sections() {
        let doc = parse_doc(r#"{"name": "test", "values": [1, 2, 3]}"#);
        let profile = EngineerProfile;
        let essence = EssenceBuilder::new(&doc, &profile).build();
        let text = profile
            .render(&essence, OutputFormat::Text)
            .unwrap_or_else(|e| panic!("{e}"));

        assert!(
            text.contains("Structure"),
            "engineer text should contain 'Structure'"
        );
        assert!(
            text.contains("Paths"),
            "engineer text should contain 'Paths'"
        );
        assert!(
            text.contains("Anomalies"),
            "engineer text should contain 'Anomalies'"
        );
        assert!(
            text.contains("Statistics"),
            "engineer text should contain 'Statistics'"
        );
    }

    #[test]
    fn render_json_produces_valid_json_with_expected_keys() {
        let doc = parse_doc(r#"{"a": 1, "b": [2, 3]}"#);
        let profile = EngineerProfile;
        let essence = EssenceBuilder::new(&doc, &profile).build();

        let json = render_json(&essence);

        assert!(
            json.get("identity").is_some(),
            "JSON should have 'identity' key"
        );
        assert!(
            json.get("structure").is_some(),
            "JSON should have 'structure' key"
        );
        assert!(
            json.get("observations").is_some(),
            "JSON should have 'observations' key"
        );
        assert!(
            json.get("anomalies").is_some(),
            "JSON should have 'anomalies' key"
        );

        // Structure should have the right subkeys
        let structure = json.get("structure").unwrap_or(&serde_json::Value::Null);
        assert!(structure.get("total_nodes").is_some());
        assert!(structure.get("distinct_paths").is_some());
        assert!(structure.get("max_depth").is_some());

        // Observations should be an array
        assert!(json["observations"].is_array());
        assert!(json["anomalies"].is_array());
    }

    #[test]
    fn render_json_via_profile() {
        let doc = parse_doc(r#"{"x": 42}"#);
        let profile = StaffProfile;
        let essence = EssenceBuilder::new(&doc, &profile).build();

        let json_str = profile
            .render(&essence, OutputFormat::Json)
            .unwrap_or_else(|e| panic!("{e}"));

        let parsed: serde_json::Value =
            serde_json::from_str(&json_str).unwrap_or_else(|e| panic!("{e}"));
        assert!(parsed.is_object());
        assert!(parsed.get("identity").is_some());
    }

    #[test]
    fn empty_document_produces_minimal_valid_essence() {
        let doc = parse_doc("{}");
        let profile = StaffProfile;

        let essence = EssenceBuilder::new(&doc, &profile).build();
        assert!(essence.observations.is_empty());
        assert!(!essence.document_identity.is_empty());

        // Text rendering should still work
        let text = profile
            .render(&essence, OutputFormat::Text)
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(!text.is_empty());
        assert!(text.contains("Document Summary"));

        // JSON rendering should still work
        let json = render_json(&essence);
        assert!(json["observations"].is_array());
        assert_eq!(json["observations"].as_array().map(Vec::len), Some(0));
    }

    #[test]
    fn full_pipeline_deterministic() {
        let json_input = r#"{"users": [{"name": "Alice", "age": 30}, {"name": "Bob", "age": 25}]}"#;
        let doc = parse_doc(json_input);

        use vajra_stats::StatsAnalyzer;
        use vajra_types::traits::Analyzer;
        let stats_analyzer = StatsAnalyzer;
        let stats = stats_analyzer
            .analyze(&doc)
            .unwrap_or_else(|e| panic!("{e}"));

        let profile = EngineerProfile;
        let e1 = EssenceBuilder::new(&doc, &profile)
            .with_stats(&stats)
            .build();
        let e2 = EssenceBuilder::new(&doc, &profile)
            .with_stats(&stats)
            .build();

        assert_eq!(e1.observations.len(), e2.observations.len());
        for (a, b) in e1.observations.iter().zip(e2.observations.iter()) {
            assert_eq!(a.observation.path, b.observation.path);
            assert_eq!(a.observation.description, b.observation.description);
            assert!((a.score - b.score).abs() < f64::EPSILON);
        }

        // Render should also be identical
        let t1 = render_text(&e1, "engineer");
        let t2 = render_text(&e2, "engineer");
        assert_eq!(t1, t2);

        let j1 = render_json(&e1);
        let j2 = render_json(&e2);
        assert_eq!(j1, j2);
    }

    #[test]
    fn profile_names_correct() {
        assert_eq!(StaffProfile.name(), "staff");
        assert_eq!(EngineerProfile.name(), "engineer");
        assert_eq!(FraudProfile.name(), "fraud");
        assert_eq!(AuditorProfile.name(), "auditor");
        assert_eq!(AiProfile.name(), "ai");
    }

    #[test]
    fn fraud_profile_renders_investigative_sections() {
        let doc = parse_doc(r#"{"name": "test", "values": [1, 2, 3]}"#);
        let profile = FraudProfile;
        let essence = EssenceBuilder::new(&doc, &profile).build();
        let text = profile
            .render(&essence, OutputFormat::Text)
            .unwrap_or_else(|e| panic!("{e}"));

        assert!(
            text.contains("Risk Indicators"),
            "fraud text should contain 'Risk Indicators'"
        );
        assert!(
            text.contains("Numeric Anomalies"),
            "fraud text should contain 'Numeric Anomalies'"
        );
        assert!(
            text.contains("Pattern Analysis"),
            "fraud text should contain 'Pattern Analysis'"
        );
    }

    #[test]
    fn auditor_profile_renders_formal_sections() {
        let doc = parse_doc(r#"{"name": "test", "values": [1, 2, 3]}"#);
        let profile = AuditorProfile;
        let essence = EssenceBuilder::new(&doc, &profile).build();
        let text = profile
            .render(&essence, OutputFormat::Text)
            .unwrap_or_else(|e| panic!("{e}"));

        assert!(
            text.contains("Completeness Assessment"),
            "auditor text should contain 'Completeness Assessment'"
        );
        assert!(
            text.contains("Consistency Findings"),
            "auditor text should contain 'Consistency Findings'"
        );
        assert!(
            text.contains("Traceability Notes"),
            "auditor text should contain 'Traceability Notes'"
        );
    }

    #[test]
    fn ai_profile_renders_compact_sections() {
        let doc = parse_doc(r#"{"name": "test", "values": [1, 2, 3]}"#);
        let profile = AiProfile;
        let essence = EssenceBuilder::new(&doc, &profile).build();
        let text = profile
            .render(&essence, OutputFormat::Text)
            .unwrap_or_else(|e| panic!("{e}"));

        assert!(
            text.contains("structure"),
            "ai text should contain 'structure'"
        );
        assert!(
            text.contains("observations"),
            "ai text should contain 'observations'"
        );
        assert!(
            text.contains("anomalies"),
            "ai text should contain 'anomalies'"
        );
    }

    #[test]
    fn new_profiles_render_json_successfully() {
        let doc = parse_doc(r#"{"x": 42}"#);

        for profile in &[
            &FraudProfile as &dyn ConcernProfile,
            &AuditorProfile as &dyn ConcernProfile,
            &AiProfile as &dyn ConcernProfile,
        ] {
            let essence = EssenceBuilder::new(&doc, *profile).build();
            let json_str = profile
                .render(&essence, OutputFormat::Json)
                .unwrap_or_else(|e| panic!("{e}"));
            let parsed: serde_json::Value =
                serde_json::from_str(&json_str).unwrap_or_else(|e| panic!("{e}"));
            assert!(parsed.is_object());
            assert!(parsed.get("identity").is_some());
        }
    }
}

//! HTML report generation for vajra analysis results.
//!
//! This crate reads pre-computed JSON analysis files (stats, anomalies,
//! governance, score, temporal, cascade) and produces a self-contained HTML
//! report with all CSS inlined. Users who need PDF output can render the
//! HTML via WeasyPrint: `weasyprint report.html report.pdf`.

mod css;
mod sections;
mod template;

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Configuration for report generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportConfig {
    pub title: String,
    pub subtitle: String,
    pub repo_name: String,
    pub analysis_date: String,
    pub data_sources: Vec<DataSourceInfo>,
}

impl Default for ReportConfig {
    fn default() -> Self {
        Self {
            title: "Analysis Report".to_string(),
            subtitle: "Health \u{00b7} Governance \u{00b7} Performance".to_string(),
            repo_name: "unknown".to_string(),
            analysis_date: "unknown".to_string(),
            data_sources: Vec::new(),
        }
    }
}

/// Describes a single data source used in the analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSourceInfo {
    pub name: String,
    pub records: usize,
    pub method: String,
}

/// All data that feeds into the report.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReportData {
    pub config: ReportConfig,
    pub stats: Option<serde_json::Value>,
    pub anomalies: Option<serde_json::Value>,
    pub governance: Option<serde_json::Value>,
    pub score: Option<serde_json::Value>,
    pub temporal: Option<serde_json::Value>,
    pub cascade: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Load report data from a directory of pre-computed JSON files.
///
/// Reads whichever files exist among: `stats.json`, `anomalies.json`,
/// `governance.json`, `score.json`, `temporal.json`, `cascade.json`.
///
/// # Errors
///
/// Returns an error only if the directory itself cannot be read.
pub fn load_report_data(dir: &Path, title: &str, repo_name: &str) -> Result<ReportData, String> {
    if !dir.is_dir() {
        return Err(format!("Input path is not a directory: {}", dir.display()));
    }

    let mut data = ReportData {
        config: ReportConfig {
            title: title.to_string(),
            repo_name: repo_name.to_string(),
            analysis_date: current_date(),
            ..ReportConfig::default()
        },
        ..ReportData::default()
    };

    let mut sources = Vec::new();

    if let Some(v) = try_load_json(dir, "stats.json") {
        let count = count_records(&v);
        sources.push(DataSourceInfo {
            name: "Statistics".to_string(),
            records: count,
            method: "vajra stats".to_string(),
        });
        data.stats = Some(v);
    }
    if let Some(v) = try_load_json(dir, "anomalies.json") {
        let count = count_records(&v);
        sources.push(DataSourceInfo {
            name: "Anomalies".to_string(),
            records: count,
            method: "vajra anomalies".to_string(),
        });
        data.anomalies = Some(v);
    }
    if let Some(v) = try_load_json(dir, "governance.json") {
        let count = count_records(&v);
        sources.push(DataSourceInfo {
            name: "Governance".to_string(),
            records: count,
            method: "vajra governance".to_string(),
        });
        data.governance = Some(v);
    }
    if let Some(v) = try_load_json(dir, "score.json") {
        let count = count_records(&v);
        sources.push(DataSourceInfo {
            name: "Health Score".to_string(),
            records: count,
            method: "vajra score".to_string(),
        });
        data.score = Some(v);
    }
    if let Some(v) = try_load_json(dir, "temporal.json") {
        let count = count_records(&v);
        sources.push(DataSourceInfo {
            name: "Temporal Analysis".to_string(),
            records: count,
            method: "vajra stats --window".to_string(),
        });
        data.temporal = Some(v);
    }
    if let Some(v) = try_load_json(dir, "cascade.json") {
        let count = count_records(&v);
        sources.push(DataSourceInfo {
            name: "Cascade Analysis".to_string(),
            records: count,
            method: "vajra cascade".to_string(),
        });
        data.cascade = Some(v);
    }

    data.config.data_sources = sources;
    Ok(data)
}

/// Generate a self-contained HTML report from [`ReportData`].
pub fn generate_html(data: &ReportData) -> String {
    template::render(data)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn try_load_json(dir: &Path, filename: &str) -> Option<serde_json::Value> {
    let path = dir.join(filename);
    let content = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

fn count_records(v: &serde_json::Value) -> usize {
    match v {
        serde_json::Value::Array(arr) => arr.len(),
        serde_json::Value::Object(obj) => obj.len(),
        _ => 1,
    }
}

fn current_date() -> String {
    // Simple approach: read from environment or fall back to a placeholder.
    // We avoid pulling in chrono just for this.
    std::env::var("VAJRA_REPORT_DATE").unwrap_or_else(|_| {
        // Try to get date from system command as a fallback.
        // If that fails, use a placeholder.
        std::process::Command::new("date")
            .arg("+%Y-%m-%d")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "unknown".to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_generate_report_with_full_data() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir failed: {e}"));

        // Write sample JSON files
        let stats = serde_json::json!({
            "total_records": 100,
            "fields": {
                "author": {
                    "type_distribution": {"string": 100},
                    "entropy": 3.45,
                    "unique_values": 20
                }
            }
        });
        let anomalies = serde_json::json!([
            {"path": "$.author", "kind": "outlier", "z_mad": 5.2, "description": "Unusual author distribution"},
            {"path": "$.lines", "kind": "outlier", "z_mad": 3.1, "description": "High line count"}
        ]);
        let governance = serde_json::json!({
            "bus_factor_80": 3,
            "top_author": "alice",
            "top_author_share": 0.35,
            "gini_coefficient": 0.72,
            "unique_authors": 15,
            "total_commits": 200,
            "churn": {"one_commit_authors": 8, "total_authors": 15, "one_commit_rate": 0.53}
        });
        let score = serde_json::json!({
            "overall_grade": "B",
            "overall_score": 72.5,
            "dimensions": [
                {"name": "Bus Factor", "grade": "C", "score": 55.0, "detail": "bus_factor=3"},
                {"name": "Commit Entropy", "grade": "B", "score": 75.0, "detail": "entropy=3.45"}
            ]
        });
        let temporal = serde_json::json!([
            {"window": "2025-01", "record_count": 50, "fields": {}},
            {"window": "2025-02", "record_count": 45, "fields": {}}
        ]);
        let cascade = serde_json::json!({
            "chains": [
                {"trigger": "deploy", "response": "fix", "count": 5, "avg_delay_hours": 2.3}
            ]
        });

        fs::write(
            dir.path().join("stats.json"),
            serde_json::to_string(&stats).unwrap_or_default(),
        )
        .unwrap_or_else(|e| panic!("write failed: {e}"));
        fs::write(
            dir.path().join("anomalies.json"),
            serde_json::to_string(&anomalies).unwrap_or_default(),
        )
        .unwrap_or_else(|e| panic!("write failed: {e}"));
        fs::write(
            dir.path().join("governance.json"),
            serde_json::to_string(&governance).unwrap_or_default(),
        )
        .unwrap_or_else(|e| panic!("write failed: {e}"));
        fs::write(
            dir.path().join("score.json"),
            serde_json::to_string(&score).unwrap_or_default(),
        )
        .unwrap_or_else(|e| panic!("write failed: {e}"));
        fs::write(
            dir.path().join("temporal.json"),
            serde_json::to_string(&temporal).unwrap_or_default(),
        )
        .unwrap_or_else(|e| panic!("write failed: {e}"));
        fs::write(
            dir.path().join("cascade.json"),
            serde_json::to_string(&cascade).unwrap_or_default(),
        )
        .unwrap_or_else(|e| panic!("write failed: {e}"));

        let data = load_report_data(dir.path(), "Test Report", "test/repo")
            .unwrap_or_else(|e| panic!("load failed: {e}"));
        let html = generate_html(&data);

        // Verify HTML structure
        assert!(html.contains("<!DOCTYPE html>"), "missing doctype");
        assert!(html.contains("Test Report"), "missing title");
        assert!(html.contains("test/repo"), "missing repo name");
        assert!(html.contains("<style>"), "missing inline CSS");

        // Verify all sections present
        assert!(
            html.contains("Executive Summary"),
            "missing executive summary"
        );
        assert!(
            html.contains("Author Distribution"),
            "missing author distribution"
        );
        assert!(html.contains("Governance"), "missing governance section");
        assert!(html.contains("Anomalies"), "missing anomalies section");
        assert!(html.contains("Methodology"), "missing methodology section");

        // Verify CSS is inline (no external stylesheets)
        assert!(
            !html.contains("<link rel=\"stylesheet\""),
            "should not have external CSS"
        );
        assert!(html.contains("--purple: #8b5cf6"), "missing CSS variables");
    }

    #[test]
    fn test_generate_report_with_partial_data() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir failed: {e}"));

        // Only write stats.json
        let stats = serde_json::json!({
            "total_records": 50,
            "fields": {"name": {"type_distribution": {"string": 50}}}
        });
        fs::write(
            dir.path().join("stats.json"),
            serde_json::to_string(&stats).unwrap_or_default(),
        )
        .unwrap_or_else(|e| panic!("write failed: {e}"));

        let data = load_report_data(dir.path(), "Partial Report", "partial/repo")
            .unwrap_or_else(|e| panic!("load failed: {e}"));
        let html = generate_html(&data);

        assert!(html.contains("Partial Report"), "missing title");
        assert!(
            html.contains("Methodology"),
            "methodology should always be present"
        );
        assert!(
            html.contains("Executive Summary"),
            "executive summary should always be present"
        );
    }

    #[test]
    fn test_generate_report_empty_directory() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir failed: {e}"));

        let data = load_report_data(dir.path(), "Empty Report", "empty/repo")
            .unwrap_or_else(|e| panic!("load failed: {e}"));
        let html = generate_html(&data);

        assert!(html.contains("Empty Report"), "missing title");
        assert!(
            html.contains("Methodology"),
            "methodology should always be present"
        );
        assert!(html.contains("<!DOCTYPE html>"), "missing doctype");
        assert!(html.contains("<style>"), "missing inline CSS");
        assert!(
            data.config.data_sources.is_empty(),
            "should have no data sources"
        );
    }

    #[test]
    fn test_css_is_included_inline() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir failed: {e}"));

        let data = load_report_data(dir.path(), "CSS Test", "css/test")
            .unwrap_or_else(|e| panic!("load failed: {e}"));
        let html = generate_html(&data);

        // CSS should be inlined
        assert!(html.contains("<style>"), "CSS must be in a <style> tag");
        assert!(
            html.contains("var(--purple)"),
            "CSS variables must be present"
        );
        assert!(html.contains(".callout"), "callout styles must be present");
        assert!(
            html.contains(".stat-grid"),
            "stat-grid styles must be present"
        );
        assert!(html.contains(".grade-a"), "grade styles must be present");
        assert!(!html.contains("<link"), "no external links allowed");
    }

    #[test]
    fn test_html_contains_expected_sections() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir failed: {e}"));

        // Create full data set
        let score = serde_json::json!({
            "overall_grade": "A",
            "overall_score": 92.0,
            "dimensions": []
        });
        let governance = serde_json::json!({
            "bus_factor_80": 5,
            "top_author": "bob",
            "top_author_share": 0.20,
            "gini_coefficient": 0.45,
            "unique_authors": 25,
            "total_commits": 500
        });
        let anomalies = serde_json::json!([
            {"path": "$.x", "kind": "outlier", "z_mad": 4.0, "description": "test anomaly"}
        ]);

        fs::write(
            dir.path().join("score.json"),
            serde_json::to_string(&score).unwrap_or_default(),
        )
        .unwrap_or_else(|e| panic!("write failed: {e}"));
        fs::write(
            dir.path().join("governance.json"),
            serde_json::to_string(&governance).unwrap_or_default(),
        )
        .unwrap_or_else(|e| panic!("write failed: {e}"));
        fs::write(
            dir.path().join("anomalies.json"),
            serde_json::to_string(&anomalies).unwrap_or_default(),
        )
        .unwrap_or_else(|e| panic!("write failed: {e}"));

        let data = load_report_data(dir.path(), "Sections Test", "sections/repo")
            .unwrap_or_else(|e| panic!("load failed: {e}"));
        let html = generate_html(&data);

        // Cover page elements
        assert!(html.contains("Sections Test"), "cover should have title");
        assert!(
            html.contains("sections/repo"),
            "cover should have repo name"
        );
        assert!(
            html.contains("Powered by vajra"),
            "cover should have vajra badge"
        );

        // All major sections
        assert!(
            html.contains("Executive Summary"),
            "missing executive summary"
        );
        assert!(
            html.contains("Author Distribution"),
            "missing author distribution"
        );
        assert!(html.contains("Governance"), "missing governance section");
        assert!(html.contains("Anomalies"), "missing anomalies section");
        assert!(html.contains("Methodology"), "missing methodology section");

        // Methodology specifics
        assert!(
            html.contains("Shannon Entropy"),
            "methodology should list algorithms"
        );
        assert!(
            html.contains("About This Report"),
            "methodology should have about section"
        );
    }

    #[test]
    fn test_load_report_data_invalid_path() {
        let result = load_report_data(Path::new("/nonexistent/path"), "Test", "test");
        assert!(result.is_err(), "should error on nonexistent path");
    }
}

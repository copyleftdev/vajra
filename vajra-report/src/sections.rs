//! Section renderers for each part of the report.
//!
//! Each function takes the relevant [`serde_json::Value`] and returns an HTML
//! fragment string to be embedded in the full template.

use crate::ReportData;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn grade_class(grade: &str) -> &'static str {
    match grade.to_uppercase().chars().next() {
        Some('A') => "grade-a",
        Some('B') => "grade-b",
        Some('C') => "grade-c",
        _ => "grade-f",
    }
}

fn score_to_grade_class(score: f64) -> &'static str {
    if score >= 90.0 {
        "grade-a"
    } else if score >= 75.0 {
        "grade-b"
    } else if score >= 60.0 {
        "grade-c"
    } else {
        "grade-f"
    }
}

fn json_str<'a>(v: &'a serde_json::Value, key: &str) -> &'a str {
    v.get(key).and_then(|x| x.as_str()).unwrap_or("")
}

fn json_f64(v: &serde_json::Value, key: &str) -> f64 {
    v.get(key).and_then(|x| x.as_f64()).unwrap_or(0.0)
}

fn json_u64(v: &serde_json::Value, key: &str) -> u64 {
    v.get(key).and_then(|x| x.as_u64()).unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Cover page
// ---------------------------------------------------------------------------

pub fn render_cover(data: &ReportData) -> String {
    let cfg = &data.config;
    let sources_line = if cfg.data_sources.is_empty() {
        String::from("No pre-computed analysis files found")
    } else {
        cfg.data_sources
            .iter()
            .map(|s| format!("{} ({} records)", html_escape(&s.name), s.records))
            .collect::<Vec<_>>()
            .join(" &middot; ")
    };

    format!(
        r#"<div class="cover">
    <h1>{title}</h1>
    <h2>{subtitle}</h2>
    <div class="cover-divider"></div>
    <div class="cover-meta">
        <strong>Repository:</strong> {repo}<br>
        <strong>Data Sources:</strong> {sources}<br>
        <strong>Analysis Date:</strong> {date}
    </div>
    <div class="cover-badge">Powered by vajra &mdash; deterministic semantic reduction</div>
</div>"#,
        title = html_escape(&cfg.title),
        subtitle = html_escape(&cfg.subtitle),
        repo = html_escape(&cfg.repo_name),
        sources = sources_line,
        date = html_escape(&cfg.analysis_date),
    )
}

// ---------------------------------------------------------------------------
// Executive Summary
// ---------------------------------------------------------------------------

pub fn render_executive_summary(data: &ReportData) -> String {
    let mut html = String::from(
        r#"<div class="page-break"></div>
<h1>Executive Summary</h1>
"#,
    );

    // Stat boxes from score data
    if let Some(score) = &data.score {
        let grade = json_str(score, "overall_grade");
        let score_val = json_f64(score, "overall_score");
        let gc = grade_class(grade);

        html.push_str(&format!(
            r#"<div class="stat-grid">
    <div class="stat-box">
        <div class="val {gc}">{grade}</div>
        <div class="desc">Overall Health</div>
    </div>
    <div class="stat-box">
        <div class="val {gc}">{score_val:.1}</div>
        <div class="desc">Health Score</div>
    </div>
"#,
            gc = gc,
            grade = html_escape(grade),
            score_val = score_val,
        ));

        // Additional stat boxes from stats data
        if let Some(stats) = &data.stats {
            if let Some(fields) = stats.get("fields") {
                if let Some(author) = fields.get("author") {
                    let entropy = json_f64(author, "entropy");
                    let unique = json_u64(author, "unique_values");
                    html.push_str(&format!(
                        r#"    <div class="stat-box">
        <div class="val">{entropy:.2}</div>
        <div class="desc">Author Entropy (bits)</div>
    </div>
    <div class="stat-box">
        <div class="val">{unique}</div>
        <div class="desc">Unique Authors</div>
    </div>
"#,
                        entropy = entropy,
                        unique = unique,
                    ));
                } else {
                    html.push_str(&empty_stat_boxes(2));
                }
            } else {
                html.push_str(&empty_stat_boxes(2));
            }
        } else {
            html.push_str(&empty_stat_boxes(2));
        }

        html.push_str("</div>\n");

        // Scorecard table from dimensions
        if let Some(dims) = score.get("dimensions").and_then(|d| d.as_array()) {
            if !dims.is_empty() {
                html.push_str(
                    r#"<h2>Health Scorecard</h2>
<table>
<thead><tr><th>Dimension</th><th>Grade</th><th>Score</th><th>Detail</th></tr></thead>
<tbody>
"#,
                );
                for dim in dims {
                    let name = json_str(dim, "name");
                    let dgrade = json_str(dim, "grade");
                    let dscore = json_f64(dim, "score");
                    let detail = json_str(dim, "detail");
                    let dgc = grade_class(dgrade);
                    html.push_str(&format!(
                        r#"<tr><td>{name}</td><td class="{dgc}"><strong>{dgrade}</strong></td><td class="num">{dscore:.1}</td><td>{detail}</td></tr>
"#,
                        name = html_escape(name),
                        dgc = dgc,
                        dgrade = html_escape(dgrade),
                        dscore = dscore,
                        detail = html_escape(detail),
                    ));
                }
                html.push_str("</tbody>\n</table>\n");
            }
        }
    } else {
        html.push_str(
            r#"<div class="callout callout-info">
    <strong>No score data available</strong>
    Run <code>vajra score</code> to generate health scores, then re-generate this report.
</div>
"#,
        );
    }

    html
}

fn empty_stat_boxes(count: usize) -> String {
    let mut s = String::new();
    for _ in 0..count {
        s.push_str(
            r#"    <div class="stat-box">
        <div class="val dim">&mdash;</div>
        <div class="desc">&mdash;</div>
    </div>
"#,
        );
    }
    s
}

// ---------------------------------------------------------------------------
// Author Distribution
// ---------------------------------------------------------------------------

pub fn render_author_distribution(data: &ReportData) -> String {
    let mut html = String::from(
        r#"<div class="page-break"></div>
<h1>Author Distribution</h1>
"#,
    );

    if let Some(stats) = &data.stats {
        // Try to extract author field stats
        if let Some(fields) = stats.get("fields") {
            if let Some(author_field) = fields.get("author") {
                let entropy = json_f64(author_field, "entropy");
                let unique = json_u64(author_field, "unique_values");
                let egc = score_to_grade_class(entropy * 20.0); // rough mapping

                html.push_str(&format!(
                    r#"<div class="stat-grid" style="grid-template-columns: 1fr 1fr;">
    <div class="stat-box">
        <div class="val {egc}">{entropy:.2}</div>
        <div class="desc">Author Entropy (bits)</div>
    </div>
    <div class="stat-box">
        <div class="val">{unique}</div>
        <div class="desc">Unique Authors</div>
    </div>
</div>
"#,
                    egc = egc,
                    entropy = entropy,
                    unique = unique,
                ));

                // Top values as horizontal bars
                if let Some(top) = author_field.get("top_values").and_then(|t| t.as_array()) {
                    if !top.is_empty() {
                        html.push_str("<h2>Top Authors</h2>\n");
                        let max_count = top
                            .iter()
                            .filter_map(|v| v.get("count").and_then(|c| c.as_u64()))
                            .max()
                            .unwrap_or(1);
                        for (i, entry) in top.iter().enumerate() {
                            let name = json_str(entry, "value");
                            let count = json_u64(entry, "count");
                            let pct = if max_count > 0 {
                                (count as f64 / max_count as f64) * 100.0
                            } else {
                                0.0
                            };
                            let color = match i {
                                0 => "purple",
                                1 | 2 => "blue",
                                _ => "slate",
                            };
                            html.push_str(&format!(
                                r#"<div class="hbar-row"><div class="hbar-label">{name}</div><div class="hbar-track"><div class="hbar-fill {color}" style="width:{pct:.0}%">{count}</div></div></div>
"#,
                                name = html_escape(name),
                                color = color,
                                pct = pct,
                                count = count,
                            ));
                        }
                    }
                }

                // Entropy dashboard callout
                html.push_str(&format!(
                    r#"<div class="callout callout-info">
    <strong>Entropy Dashboard</strong>
    Shannon entropy of {entropy:.2} bits across {unique} unique authors.
    Higher entropy indicates more distributed contributions. Maximum possible entropy for {unique} authors would be {max_entropy:.2} bits.
</div>
"#,
                    entropy = entropy,
                    unique = unique,
                    max_entropy = if unique > 0 {
                        (unique as f64).log2()
                    } else {
                        0.0
                    },
                ));
            } else {
                html.push_str(
                    r#"<div class="callout callout-info">
    <strong>No author field detected</strong>
    The stats data does not contain an &quot;author&quot; field. Author distribution analysis requires an author field in the source data.
</div>
"#,
                );
            }
        } else {
            html.push_str(
                r#"<p class="dim">Stats data available but no field breakdown found.</p>
"#,
            );
        }
    } else {
        html.push_str(
            r#"<div class="callout callout-info">
    <strong>No statistics data available</strong>
    Run <code>vajra stats</code> to generate statistics, then re-generate this report.
</div>
"#,
        );
    }

    html
}

// ---------------------------------------------------------------------------
// Governance
// ---------------------------------------------------------------------------

pub fn render_governance(data: &ReportData) -> String {
    let mut html = String::from(
        r#"<div class="page-break"></div>
<h1>Governance</h1>
"#,
    );

    if let Some(gov) = &data.governance {
        let bus_factor = json_u64(gov, "bus_factor_80");
        let top_author = json_str(gov, "top_author");
        let top_share = json_f64(gov, "top_author_share");
        let gini = json_f64(gov, "gini_coefficient");
        let unique_authors = json_u64(gov, "unique_authors");
        let total_commits = json_u64(gov, "total_commits");

        let bf_class = if bus_factor <= 2 {
            "grade-f"
        } else if bus_factor <= 5 {
            "grade-c"
        } else {
            "grade-a"
        };
        let gini_class = if gini > 0.7 {
            "grade-f"
        } else if gini > 0.5 {
            "grade-c"
        } else {
            "grade-a"
        };

        html.push_str(&format!(
            r#"<div class="stat-grid">
    <div class="stat-box">
        <div class="val {bf_class}">{bus_factor}</div>
        <div class="desc">Bus Factor (80%)</div>
    </div>
    <div class="stat-box">
        <div class="val {gini_class}">{gini:.2}</div>
        <div class="desc">Gini Coefficient</div>
    </div>
    <div class="stat-box">
        <div class="val">{unique_authors}</div>
        <div class="desc">Unique Authors</div>
    </div>
    <div class="stat-box">
        <div class="val">{total_commits}</div>
        <div class="desc">Total Commits</div>
    </div>
</div>
"#,
            bf_class = bf_class,
            bus_factor = bus_factor,
            gini_class = gini_class,
            gini = gini,
            unique_authors = unique_authors,
            total_commits = total_commits,
        ));

        // Top author callout
        let share_pct = top_share * 100.0;
        let share_class = if top_share > 0.5 {
            "callout-critical"
        } else if top_share > 0.3 {
            "callout-warning"
        } else {
            "callout-good"
        };
        html.push_str(&format!(
            r#"<div class="callout {share_class}">
    <strong>Top Contributor: {top_author}</strong>
    Accounts for {share_pct:.1}% of all commits. Bus factor is {bus_factor} (number of authors needed to cover 80% of commits).
</div>
"#,
            share_class = share_class,
            top_author = html_escape(top_author),
            share_pct = share_pct,
            bus_factor = bus_factor,
        ));

        // Churn data if available
        if let Some(churn) = gov.get("churn") {
            let one_commit = json_u64(churn, "one_commit_authors");
            let total_auth = json_u64(churn, "total_authors");
            let rate = json_f64(churn, "one_commit_rate");
            let rate_pct = rate * 100.0;
            let churn_class = if rate > 0.5 {
                "grade-f"
            } else if rate > 0.3 {
                "grade-c"
            } else {
                "grade-a"
            };

            html.push_str(&format!(
                r#"<h2>Contributor Churn</h2>
<table>
<thead><tr><th>Metric</th><th>Value</th><th>Assessment</th></tr></thead>
<tbody>
<tr><td>One-commit authors</td><td class="num">{one_commit} / {total_auth}</td><td class="{churn_class}"><strong>{rate_pct:.1}%</strong></td></tr>
</tbody>
</table>
"#,
                one_commit = one_commit,
                total_auth = total_auth,
                churn_class = churn_class,
                rate_pct = rate_pct,
            ));
        }
    } else {
        html.push_str(
            r#"<div class="callout callout-info">
    <strong>No governance data available</strong>
    Run <code>vajra governance</code> to generate governance metrics, then re-generate this report.
</div>
"#,
        );
    }

    html
}

// ---------------------------------------------------------------------------
// Anomalies
// ---------------------------------------------------------------------------

pub fn render_anomalies(data: &ReportData) -> String {
    let mut html = String::from(
        r#"<div class="page-break"></div>
<h1>Anomalies</h1>
"#,
    );

    if let Some(anomalies) = &data.anomalies {
        if let Some(arr) = anomalies.as_array() {
            if arr.is_empty() {
                html.push_str(
                    r#"<div class="callout callout-good">
    <strong>No anomalies detected</strong>
    vajra found no statistically significant outliers in the dataset.
</div>
"#,
                );
            } else {
                html.push_str(&format!(
                    "<p>vajra detected <strong>{}</strong> anomalies in the dataset.</p>\n",
                    arr.len()
                ));

                html.push_str(
                    r#"<table>
<thead><tr><th>Path</th><th>Kind</th><th>z_mad</th><th>Description</th></tr></thead>
<tbody>
"#,
                );

                // Show top 20 anomalies
                for anomaly in arr.iter().take(20) {
                    let path = json_str(anomaly, "path");
                    let kind = json_str(anomaly, "kind");
                    let z_mad = json_f64(anomaly, "z_mad");
                    let desc = json_str(anomaly, "description");

                    let z_class = if z_mad >= 5.0 {
                        "grade-f"
                    } else if z_mad >= 3.0 {
                        "grade-c"
                    } else {
                        "grade-b"
                    };

                    html.push_str(&format!(
                        r#"<tr><td class="mono">{path}</td><td>{kind}</td><td class="num {z_class}"><strong>{z_mad:.1}</strong></td><td>{desc}</td></tr>
"#,
                        path = html_escape(path),
                        kind = html_escape(kind),
                        z_class = z_class,
                        z_mad = z_mad,
                        desc = html_escape(desc),
                    ));
                }

                html.push_str("</tbody>\n</table>\n");

                if arr.len() > 20 {
                    html.push_str(&format!(
                        r#"<p class="dim">Showing top 20 of {} anomalies.</p>
"#,
                        arr.len()
                    ));
                }
            }
        } else {
            // anomalies is an object, render key-value style
            render_anomalies_object(&mut html, anomalies);
        }
    } else {
        html.push_str(
            r#"<div class="callout callout-info">
    <strong>No anomaly data available</strong>
    Run <code>vajra anomalies</code> to detect outliers, then re-generate this report.
</div>
"#,
        );
    }

    html
}

fn render_anomalies_object(html: &mut String, obj: &serde_json::Value) {
    if let Some(map) = obj.as_object() {
        // Try to find anomaly arrays within the object
        if let Some(arr) = map.get("anomalies").and_then(|a| a.as_array()) {
            html.push_str(&format!(
                "<p>vajra detected <strong>{}</strong> anomalies.</p>\n",
                arr.len()
            ));
            html.push_str(
                r#"<table>
<thead><tr><th>Path</th><th>Kind</th><th>z_mad</th><th>Description</th></tr></thead>
<tbody>
"#,
            );
            for anomaly in arr.iter().take(20) {
                let path = json_str(anomaly, "path");
                let kind = json_str(anomaly, "kind");
                let z_mad = json_f64(anomaly, "z_mad");
                let desc = json_str(anomaly, "description");
                let z_class = if z_mad >= 5.0 {
                    "grade-f"
                } else if z_mad >= 3.0 {
                    "grade-c"
                } else {
                    "grade-b"
                };
                html.push_str(&format!(
                    r#"<tr><td class="mono">{path}</td><td>{kind}</td><td class="num {z_class}"><strong>{z_mad:.1}</strong></td><td>{desc}</td></tr>
"#,
                    path = html_escape(path),
                    kind = html_escape(kind),
                    z_class = z_class,
                    z_mad = z_mad,
                    desc = html_escape(desc),
                ));
            }
            html.push_str("</tbody>\n</table>\n");
        } else {
            // Generic object rendering
            html.push_str("<p>Anomaly data summary:</p>\n<table>\n<thead><tr><th>Key</th><th>Value</th></tr></thead>\n<tbody>\n");
            for (k, v) in map {
                let val_str = match v {
                    serde_json::Value::String(s) => html_escape(s),
                    other => html_escape(&other.to_string()),
                };
                html.push_str(&format!(
                    "<tr><td class=\"mono\">{}</td><td>{}</td></tr>\n",
                    html_escape(k),
                    val_str,
                ));
            }
            html.push_str("</tbody>\n</table>\n");
        }
    }
}

// ---------------------------------------------------------------------------
// Methodology (always rendered)
// ---------------------------------------------------------------------------

pub fn render_methodology(data: &ReportData) -> String {
    let mut html = String::from(
        r#"<div class="page-break"></div>
<h1>Methodology</h1>

<h2>Analysis Engine</h2>
<p><strong>vajra</strong> &mdash; a deterministic semantic reduction engine for structured data. Every analysis in this report is reproducible: same input + same config = byte-identical output. No ML, no heuristics, no sentiment analysis. Pure information theory on structured data.</p>

<h2>Algorithms Applied</h2>
<table>
<thead><tr><th>Algorithm</th><th>Purpose</th><th>Where Used</th></tr></thead>
<tbody>
<tr><td>Shannon Entropy</td><td>Value diversity / concentration</td><td>Author entropy, field diversity</td></tr>
<tr><td>MAD (Median Absolute Deviation)</td><td>Robust outlier detection</td><td>Anomaly z-scores, burnout signals</td></tr>
<tr><td>Conditional Entropy H(Y|X)</td><td>Cross-field dependency</td><td>Field correlation analysis</td></tr>
<tr><td>Jensen-Shannon Divergence</td><td>Distribution comparison</td><td>Drift detection between populations</td></tr>
<tr><td>Wasserstein Distance</td><td>Distribution shift magnitude</td><td>Numeric field drift measurement</td></tr>
<tr><td>BLAKE3</td><td>Structural fingerprinting</td><td>Document identity and clustering</td></tr>
<tr><td>MinHash + LSH</td><td>Similarity clustering</td><td>Near-duplicate detection</td></tr>
<tr><td>Gini Coefficient</td><td>Inequality measurement</td><td>Commit concentration analysis</td></tr>
</tbody>
</table>
"#,
    );

    // Data sources table
    if !data.config.data_sources.is_empty() {
        html.push_str(
            r#"<h2>Data Sources</h2>
<table>
<thead><tr><th>Source</th><th>Records</th><th>Method</th></tr></thead>
<tbody>
"#,
        );
        for src in &data.config.data_sources {
            html.push_str(&format!(
                "<tr><td>{name}</td><td class=\"num\">{records}</td><td><code>{method}</code></td></tr>\n",
                name = html_escape(&src.name),
                records = src.records,
                method = html_escape(&src.method),
            ));
        }
        html.push_str("</tbody>\n</table>\n");
    }

    html.push_str(&format!(
        r#"<div class="callout callout-info" style="margin-top: 20px;">
    <strong>About This Report</strong>
    Generated on {date} using vajra. All findings are based on deterministic information-theoretic
    analysis of the provided data. This report is intended as a constructive health assessment.
    Open source maintainers deserve immense gratitude for their work.
</div>

<div style="text-align: center; margin-top: 30px; padding-top: 20px; border-top: 1px solid var(--slate-200);">
    <p class="dim" style="font-size: 7pt; margin-top: 8px;">Deterministic semantic reduction &mdash; vajra</p>
</div>
"#,
        date = html_escape(&data.config.analysis_date),
    ));

    html
}

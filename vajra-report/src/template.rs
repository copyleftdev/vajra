//! Top-level HTML template that composes all sections.

use crate::css::report_css;
use crate::sections;
use crate::ReportData;

/// Render the complete HTML report from [`ReportData`].
pub fn render(data: &ReportData) -> String {
    let mut html = String::with_capacity(32 * 1024);

    // Document head
    html.push_str(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<meta name="generator" content="vajra report">
<style>
"#,
    );
    html.push_str(report_css());
    html.push_str(
        r#"</style>
</head>
<body>

"#,
    );

    // 1. Cover page
    html.push_str(&sections::render_cover(data));
    html.push('\n');

    // 2. Executive Summary
    html.push_str(&sections::render_executive_summary(data));
    html.push('\n');

    // 3. Author Distribution
    html.push_str(&sections::render_author_distribution(data));
    html.push('\n');

    // 4. Governance
    html.push_str(&sections::render_governance(data));
    html.push('\n');

    // 5. Anomalies
    html.push_str(&sections::render_anomalies(data));
    html.push('\n');

    // 6. Methodology (always present)
    html.push_str(&sections::render_methodology(data));
    html.push('\n');

    // Close document
    html.push_str(
        r#"</body>
</html>
"#,
    );

    html
}

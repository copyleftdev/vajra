//! Governance metrics: bus factor, merge equity, contributor churn.
//!
//! All algorithms are O(n log n) or better. Uses `BTreeMap` for determinism.
//! No `unwrap`/`expect`/`panic`.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::temporal::{parse_iso8601, truncate_to_window, value_to_epoch, WindowGranularity};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Governance metrics computed from author/item distributions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceMetrics {
    /// Minimum authors covering 80% of items.
    pub bus_factor_80: usize,
    /// Minimum authors covering 50% of items.
    pub bus_factor_50: usize,
    /// Shannon entropy of the author distribution (bits).
    pub author_entropy: f64,
    /// Gini coefficient: 0 = equal, 1 = one person does everything.
    pub gini_coefficient: f64,
    /// Fraction of items by the top author.
    pub top1_share: f64,
    /// Fraction of items by the top 3 authors.
    pub top3_share: f64,
    /// Fraction of items by the top 5 authors.
    pub top5_share: f64,
    /// Fraction of authors with exactly 1 item.
    pub one_commit_rate: f64,
    /// Number of distinct authors.
    pub unique_authors: usize,
    /// Total number of items.
    pub total_items: usize,
}

/// Contributor churn analysis over monthly windows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChurnMetrics {
    /// Per-month new/returning/churned/active breakdown.
    pub monthly: Vec<MonthChurn>,
    /// Mean net contributor growth per month.
    pub mean_net_growth: f64,
    /// Median author tenure (first to last item), in days.
    pub contributor_half_life_days: Option<f64>,
}

/// A single month's contributor churn breakdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthChurn {
    /// Month label (YYYY-MM).
    pub month: String,
    /// Authors seen for the first time this month.
    pub new: usize,
    /// Authors who were seen before and are active this month.
    pub returning: usize,
    /// Authors active last month but not this month.
    pub churned: usize,
    /// Total unique active authors this month.
    pub active: usize,
    /// Net = new - churned.
    pub net: i64,
}

/// Combined governance + churn report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceReport {
    #[serde(flatten)]
    pub governance: GovernanceMetrics,
    pub churn: ChurnMetrics,
}

/// Errors from governance computation.
#[derive(Debug, thiserror::Error)]
pub enum GovernanceError {
    #[error("no items provided")]
    EmptyInput,
    #[error("author field not found in record")]
    AuthorFieldMissing,
    #[error("time field not found in record")]
    TimeFieldMissing,
}

// ---------------------------------------------------------------------------
// Input extraction
// ---------------------------------------------------------------------------

/// A single authored, timestamped item extracted from a JSON record.
struct AuthoredItem {
    author: String,
    epoch: i64,
}

/// Extract `(author, epoch)` pairs from a JSON array using JSONPath-style field selectors.
fn extract_items(
    records: &[serde_json::Value],
    author_field: &str,
    time_field: &str,
) -> Result<Vec<AuthoredItem>, GovernanceError> {
    let mut items = Vec::with_capacity(records.len());
    for record in records {
        let author_val = crate::temporal::extract_json_path(record, author_field)
            .ok_or(GovernanceError::AuthorFieldMissing)?;
        let author = match author_val.as_str() {
            Some(s) => s.to_owned(),
            None => author_val.to_string(),
        };

        let time_val = crate::temporal::extract_json_path(record, time_field)
            .ok_or(GovernanceError::TimeFieldMissing)?;
        let epoch = value_to_epoch(time_val).or_else(|| {
            // Fallback: try string representation
            time_val.as_str().and_then(parse_iso8601)
        });

        // Skip records where the time cannot be parsed rather than failing hard.
        if let Some(epoch) = epoch {
            items.push(AuthoredItem { author, epoch });
        }
    }
    Ok(items)
}

// ---------------------------------------------------------------------------
// Bus factor
// ---------------------------------------------------------------------------

/// Compute the bus factor at a given threshold: minimum number of top authors
/// whose items cover at least `threshold` fraction of total items.
///
/// `counts` must be sorted descending. Returns at least 1 if there is any author.
fn bus_factor(counts_desc: &[usize], total: usize, threshold: f64) -> usize {
    if counts_desc.is_empty() || total == 0 {
        return 0;
    }
    let target = (total as f64 * threshold).ceil() as usize;
    let mut cumulative: usize = 0;
    for (i, &c) in counts_desc.iter().enumerate() {
        cumulative = cumulative.saturating_add(c);
        if cumulative >= target {
            return i + 1;
        }
    }
    counts_desc.len()
}

// ---------------------------------------------------------------------------
// Gini coefficient
// ---------------------------------------------------------------------------

/// Gini coefficient from a slice of non-negative values.
///
/// Formula (sorted ascending):
///   G = (2 * sum(i * x_i)) / (n * sum(x_i)) - (n + 1) / n
///
/// O(n log n) for sort.
fn gini(values: &[usize]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted: Vec<usize> = values.to_vec();
    sorted.sort_unstable();

    let n = sorted.len();
    #[allow(clippy::cast_precision_loss)]
    let n_f = n as f64;
    let total: usize = sorted.iter().sum();
    if total == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let total_f = total as f64;

    let mut weighted_sum: f64 = 0.0;
    for (i, &x) in sorted.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let idx = (i + 1) as f64; // 1-based
        #[allow(clippy::cast_precision_loss)]
        let x_f = x as f64;
        weighted_sum += idx * x_f;
    }

    let g = (2.0 * weighted_sum) / (n_f * total_f) - (n_f + 1.0) / n_f;
    g.clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// Shannon entropy of author distribution
// ---------------------------------------------------------------------------

fn author_entropy(counts: &[usize], total: usize) -> f64 {
    if total == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let total_f = total as f64;
    let mut h = 0.0_f64;
    for &c in counts {
        if c == 0 {
            continue;
        }
        #[allow(clippy::cast_precision_loss)]
        let p = c as f64 / total_f;
        h -= p * p.log2();
    }
    if h < 0.0 {
        0.0
    } else {
        h
    }
}

// ---------------------------------------------------------------------------
// Top-K share
// ---------------------------------------------------------------------------

fn top_k_share(counts_desc: &[usize], total: usize, k: usize) -> f64 {
    if total == 0 {
        return 0.0;
    }
    let top_sum: usize = counts_desc.iter().take(k).sum();
    #[allow(clippy::cast_precision_loss)]
    let share = top_sum as f64 / total as f64;
    share.clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// Governance metrics (from author->count map)
// ---------------------------------------------------------------------------

fn compute_governance(author_counts: &BTreeMap<String, usize>, total: usize) -> GovernanceMetrics {
    let unique_authors = author_counts.len();
    let counts: Vec<usize> = author_counts.values().copied().collect();

    // Descending-sorted counts for bus factor / top-K
    let mut counts_desc = counts.clone();
    counts_desc.sort_unstable_by(|a, b| b.cmp(a));

    let one_commit_count = counts.iter().filter(|&&c| c == 1).count();
    #[allow(clippy::cast_precision_loss)]
    let one_commit_rate = if unique_authors == 0 {
        0.0
    } else {
        one_commit_count as f64 / unique_authors as f64
    };

    GovernanceMetrics {
        bus_factor_80: bus_factor(&counts_desc, total, 0.80),
        bus_factor_50: bus_factor(&counts_desc, total, 0.50),
        author_entropy: author_entropy(&counts, total),
        gini_coefficient: gini(&counts),
        top1_share: top_k_share(&counts_desc, total, 1),
        top3_share: top_k_share(&counts_desc, total, 3),
        top5_share: top_k_share(&counts_desc, total, 5),
        one_commit_rate,
        unique_authors,
        total_items: total,
    }
}

// ---------------------------------------------------------------------------
// Churn metrics
// ---------------------------------------------------------------------------

fn compute_churn(items: &[AuthoredItem]) -> ChurnMetrics {
    if items.is_empty() {
        return ChurnMetrics {
            monthly: Vec::new(),
            mean_net_growth: 0.0,
            contributor_half_life_days: None,
        };
    }

    // Group authors by month
    let mut month_authors: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for item in items {
        if let Some(label) = truncate_to_window(item.epoch, WindowGranularity::Month) {
            month_authors
                .entry(label)
                .or_default()
                .insert(item.author.clone());
        }
    }

    let months: Vec<String> = month_authors.keys().cloned().collect();
    let mut ever_seen: BTreeSet<String> = BTreeSet::new();
    let mut prev_active: BTreeSet<String> = BTreeSet::new();
    let mut monthly: Vec<MonthChurn> = Vec::with_capacity(months.len());

    for month in &months {
        let active = match month_authors.get(month) {
            Some(set) => set.clone(),
            None => BTreeSet::new(),
        };

        let new_authors: usize = active.iter().filter(|a| !ever_seen.contains(*a)).count();
        let returning: usize = active.iter().filter(|a| ever_seen.contains(*a)).count();
        let churned: usize = prev_active.iter().filter(|a| !active.contains(*a)).count();

        #[allow(clippy::cast_possible_wrap)]
        let net = new_authors as i64 - churned as i64;

        monthly.push(MonthChurn {
            month: month.clone(),
            new: new_authors,
            returning,
            churned,
            active: active.len(),
            net,
        });

        for a in &active {
            ever_seen.insert(a.clone());
        }
        prev_active = active;
    }

    // Mean net growth
    #[allow(clippy::cast_precision_loss)]
    let mean_net_growth = if monthly.is_empty() {
        0.0
    } else {
        let sum: i64 = monthly.iter().map(|m| m.net).sum();
        sum as f64 / monthly.len() as f64
    };

    // Contributor half-life: median tenure in days
    let contributor_half_life_days = compute_half_life(items);

    ChurnMetrics {
        monthly,
        mean_net_growth,
        contributor_half_life_days,
    }
}

/// Compute median author tenure (first item to last item) in days.
fn compute_half_life(items: &[AuthoredItem]) -> Option<f64> {
    if items.is_empty() {
        return None;
    }

    // For each author, find min/max epoch
    let mut author_range: BTreeMap<&str, (i64, i64)> = BTreeMap::new();
    for item in items {
        let entry = author_range
            .entry(item.author.as_str())
            .or_insert((item.epoch, item.epoch));
        if item.epoch < entry.0 {
            entry.0 = item.epoch;
        }
        if item.epoch > entry.1 {
            entry.1 = item.epoch;
        }
    }

    let mut tenures: Vec<f64> = author_range
        .values()
        .map(|(first, last)| (last - first) as f64 / 86400.0)
        .collect();

    if tenures.is_empty() {
        return None;
    }

    tenures.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = tenures.len() / 2;
    if tenures.len().is_multiple_of(2) && tenures.len() >= 2 {
        Some((tenures[mid - 1] + tenures[mid]) / 2.0)
    } else {
        Some(tenures[mid])
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Compute governance and churn metrics from a JSON array of records.
///
/// `author_field` and `time_field` are JSONPath-style selectors (e.g. `$.author`).
pub fn governance_analysis(
    records: &[serde_json::Value],
    author_field: &str,
    time_field: &str,
) -> Result<GovernanceReport, GovernanceError> {
    if records.is_empty() {
        return Err(GovernanceError::EmptyInput);
    }

    let items = extract_items(records, author_field, time_field)?;
    if items.is_empty() {
        return Err(GovernanceError::EmptyInput);
    }

    // Build author -> count map (deterministic via BTreeMap)
    let mut author_counts: BTreeMap<String, usize> = BTreeMap::new();
    for item in &items {
        *author_counts.entry(item.author.clone()).or_insert(0) += 1;
    }

    let total = items.len();
    let governance = compute_governance(&author_counts, total);
    let churn = compute_churn(&items);

    Ok(GovernanceReport { governance, churn })
}

// ---------------------------------------------------------------------------
// Output rendering
// ---------------------------------------------------------------------------

/// Render the governance report as a human-readable text string.
pub fn render_text(report: &GovernanceReport) -> String {
    use std::fmt::Write;
    let mut o = String::new();

    let g = &report.governance;
    let _ = writeln!(o, "=== Governance Metrics ===");
    let _ = writeln!(o, "  Bus factor (80%):   {}", g.bus_factor_80);
    let _ = writeln!(o, "  Bus factor (50%):   {}", g.bus_factor_50);
    let _ = writeln!(o, "  Author entropy:     {:.2} bits", g.author_entropy);
    let _ = writeln!(o, "  Gini coefficient:   {:.4}", g.gini_coefficient);
    let _ = writeln!(o, "  Top-1 share:        {:.3}", g.top1_share);
    let _ = writeln!(o, "  Top-3 share:        {:.3}", g.top3_share);
    let _ = writeln!(o, "  Top-5 share:        {:.3}", g.top5_share);
    let _ = writeln!(o, "  One-commit rate:    {:.3}", g.one_commit_rate);
    let _ = writeln!(o, "  Unique authors:     {}", g.unique_authors);
    let _ = writeln!(o, "  Total items:        {}", g.total_items);
    let _ = writeln!(o);

    let c = &report.churn;
    let _ = writeln!(o, "=== Contributor Churn ===");
    let _ = writeln!(o, "  Mean net growth:    {:.2}", c.mean_net_growth);
    match c.contributor_half_life_days {
        Some(hl) => {
            let _ = writeln!(o, "  Half-life (days):   {:.1}", hl);
        }
        None => {
            let _ = writeln!(o, "  Half-life (days):   N/A");
        }
    }
    let _ = writeln!(o);

    if !c.monthly.is_empty() {
        let _ = writeln!(o, "  MONTH       NEW  RETURN  CHURNED  ACTIVE   NET");
        for m in &c.monthly {
            let _ = writeln!(
                o,
                "  {:<10} {:>4}  {:>6}  {:>7}  {:>6}  {:>4}",
                m.month, m.new, m.returning, m.churned, m.active, m.net
            );
        }
    }

    o
}

/// Render the governance report as a Markdown string.
pub fn render_markdown(report: &GovernanceReport) -> String {
    use std::fmt::Write;
    let mut o = String::new();

    let g = &report.governance;
    let _ = writeln!(o, "# Governance Metrics\n");
    let _ = writeln!(o, "| Metric | Value |");
    let _ = writeln!(o, "|--------|-------|");
    let _ = writeln!(o, "| Bus factor (80%) | {} |", g.bus_factor_80);
    let _ = writeln!(o, "| Bus factor (50%) | {} |", g.bus_factor_50);
    let _ = writeln!(o, "| Author entropy | {:.2} bits |", g.author_entropy);
    let _ = writeln!(o, "| Gini coefficient | {:.4} |", g.gini_coefficient);
    let _ = writeln!(o, "| Top-1 share | {:.3} |", g.top1_share);
    let _ = writeln!(o, "| Top-3 share | {:.3} |", g.top3_share);
    let _ = writeln!(o, "| Top-5 share | {:.3} |", g.top5_share);
    let _ = writeln!(o, "| One-commit rate | {:.3} |", g.one_commit_rate);
    let _ = writeln!(o, "| Unique authors | {} |", g.unique_authors);
    let _ = writeln!(o, "| Total items | {} |", g.total_items);
    let _ = writeln!(o);

    let c = &report.churn;
    let _ = writeln!(o, "## Contributor Churn\n");
    let _ = writeln!(o, "| Metric | Value |");
    let _ = writeln!(o, "|--------|-------|");
    let _ = writeln!(o, "| Mean net growth | {:.2} |", c.mean_net_growth);
    match c.contributor_half_life_days {
        Some(hl) => {
            let _ = writeln!(o, "| Half-life (days) | {:.1} |", hl);
        }
        None => {
            let _ = writeln!(o, "| Half-life (days) | N/A |");
        }
    }
    let _ = writeln!(o);

    if !c.monthly.is_empty() {
        let _ = writeln!(o, "### Monthly Breakdown\n");
        let _ = writeln!(o, "| Month | New | Returning | Churned | Active | Net |");
        let _ = writeln!(o, "|-------|-----|-----------|---------|--------|-----|");
        for m in &c.monthly {
            let _ = writeln!(
                o,
                "| {} | {} | {} | {} | {} | {} |",
                m.month, m.new, m.returning, m.churned, m.active, m.net
            );
        }
    }

    o
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-6;

    // ---- bus factor ----

    #[test]
    fn bus_factor_three_authors_30_30_40() {
        // 3 authors: 40%, 30%, 30%
        // Cumulative desc = [40, 70, 100]
        // 40 < 80, 70 < 80, 100 >= 80 -> bus_factor_80 = 3
        // 40 < 50, 70 >= 50 -> bus_factor_50 = 2
        let counts_desc = vec![40, 30, 30];
        assert_eq!(bus_factor(&counts_desc, 100, 0.80), 3);
        assert_eq!(bus_factor(&counts_desc, 100, 0.50), 2);
    }

    #[test]
    fn bus_factor_single_dominant_author() {
        // One author does 90%
        let counts_desc = vec![90, 5, 3, 2];
        assert_eq!(bus_factor(&counts_desc, 100, 0.80), 1);
        assert_eq!(bus_factor(&counts_desc, 100, 0.50), 1);
    }

    #[test]
    fn bus_factor_equal_authors() {
        // 10 authors, 10 items each -> bus_factor_80 = 8
        let counts_desc = vec![10; 10];
        assert_eq!(bus_factor(&counts_desc, 100, 0.80), 8);
        assert_eq!(bus_factor(&counts_desc, 100, 0.50), 5);
    }

    #[test]
    fn bus_factor_empty() {
        let counts_desc: Vec<usize> = vec![];
        assert_eq!(bus_factor(&counts_desc, 0, 0.80), 0);
    }

    // ---- gini coefficient ----

    #[test]
    fn gini_equal_distribution() {
        // All equal -> Gini = 0
        let values = vec![10, 10, 10, 10];
        let g = gini(&values);
        assert!(g.abs() < EPS, "expected ~0, got {g}");
    }

    #[test]
    fn gini_single_author_does_everything() {
        // One author: 100, rest: 0 -> Gini close to 1
        let values = vec![100, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let g = gini(&values);
        assert!(g > 0.7, "expected high gini, got {g}");
    }

    #[test]
    fn gini_two_equal() {
        let values = vec![50, 50];
        let g = gini(&values);
        assert!(g.abs() < EPS, "expected ~0, got {g}");
    }

    #[test]
    fn gini_in_range() {
        let values = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 100];
        let g = gini(&values);
        assert!((0.0..=1.0).contains(&g), "gini out of range: {g}");
    }

    // ---- one-commit rate ----

    #[test]
    fn one_commit_rate_known() {
        let mut counts = BTreeMap::new();
        counts.insert("alice".to_owned(), 10);
        counts.insert("bob".to_owned(), 1);
        counts.insert("carol".to_owned(), 1);
        counts.insert("dave".to_owned(), 5);
        // 2 out of 4 have 1 item -> 0.5
        let gov = compute_governance(&counts, 17);
        assert!((gov.one_commit_rate - 0.5).abs() < EPS);
    }

    // ---- churn: 3-month dataset ----

    #[test]
    fn churn_three_months() {
        // Month 1: alice, bob
        // Month 2: bob, carol (alice churned, carol new)
        // Month 3: carol, dave (bob churned, dave new)
        let items = vec![
            AuthoredItem {
                author: "alice".to_owned(),
                epoch: 1672531200, // 2023-01-01
            },
            AuthoredItem {
                author: "bob".to_owned(),
                epoch: 1672617600, // 2023-01-02
            },
            AuthoredItem {
                author: "bob".to_owned(),
                epoch: 1675209600, // 2023-02-01
            },
            AuthoredItem {
                author: "carol".to_owned(),
                epoch: 1675296000, // 2023-02-02
            },
            AuthoredItem {
                author: "carol".to_owned(),
                epoch: 1677628800, // 2023-03-01
            },
            AuthoredItem {
                author: "dave".to_owned(),
                epoch: 1677715200, // 2023-03-02
            },
        ];

        let churn = compute_churn(&items);
        assert_eq!(churn.monthly.len(), 3);

        // Month 1: new=2 (alice,bob), returning=0, churned=0, active=2, net=2
        assert_eq!(churn.monthly[0].new, 2);
        assert_eq!(churn.monthly[0].returning, 0);
        assert_eq!(churn.monthly[0].churned, 0);
        assert_eq!(churn.monthly[0].active, 2);
        assert_eq!(churn.monthly[0].net, 2);

        // Month 2: new=1 (carol), returning=1 (bob), churned=1 (alice), active=2, net=0
        assert_eq!(churn.monthly[1].new, 1);
        assert_eq!(churn.monthly[1].returning, 1);
        assert_eq!(churn.monthly[1].churned, 1);
        assert_eq!(churn.monthly[1].active, 2);
        assert_eq!(churn.monthly[1].net, 0);

        // Month 3: new=1 (dave), returning=1 (carol), churned=1 (bob), active=2, net=0
        assert_eq!(churn.monthly[2].new, 1);
        assert_eq!(churn.monthly[2].returning, 1);
        assert_eq!(churn.monthly[2].churned, 1);
        assert_eq!(churn.monthly[2].active, 2);
        assert_eq!(churn.monthly[2].net, 0);
    }

    // ---- half-life ----

    #[test]
    fn half_life_known_tenure() {
        // alice: day 0 to day 10 -> tenure 10 days
        // bob:   day 0 to day 0  -> tenure 0 days
        // carol: day 0 to day 20 -> tenure 20 days
        // Sorted tenures: [0, 10, 20] -> median = 10
        let items = vec![
            AuthoredItem {
                author: "alice".to_owned(),
                epoch: 0,
            },
            AuthoredItem {
                author: "alice".to_owned(),
                epoch: 10 * 86400,
            },
            AuthoredItem {
                author: "bob".to_owned(),
                epoch: 0,
            },
            AuthoredItem {
                author: "carol".to_owned(),
                epoch: 0,
            },
            AuthoredItem {
                author: "carol".to_owned(),
                epoch: 20 * 86400,
            },
        ];

        let hl = compute_half_life(&items);
        assert!((hl.unwrap_or(0.0) - 10.0).abs() < EPS);
    }

    // ---- property tests via known constraints ----

    #[test]
    fn property_bus_factor_at_least_one() {
        let counts_desc = vec![50, 30, 20];
        assert!(bus_factor(&counts_desc, 100, 0.80) >= 1);
        assert!(bus_factor(&counts_desc, 100, 0.50) >= 1);
    }

    #[test]
    fn property_gini_in_zero_one() {
        for distribution in [
            vec![1],
            vec![1, 1],
            vec![1, 100],
            vec![1, 1, 1, 1000],
            vec![10, 10, 10, 10, 10],
        ] {
            let g = gini(&distribution);
            assert!(
                (0.0..=1.0).contains(&g),
                "gini out of range for {distribution:?}: {g}"
            );
        }
    }

    #[test]
    fn property_rates_in_zero_one() {
        let mut counts = BTreeMap::new();
        counts.insert("a".to_owned(), 50);
        counts.insert("b".to_owned(), 30);
        counts.insert("c".to_owned(), 20);
        let gov = compute_governance(&counts, 100);
        assert!((0.0..=1.0).contains(&gov.top1_share));
        assert!((0.0..=1.0).contains(&gov.top3_share));
        assert!((0.0..=1.0).contains(&gov.top5_share));
        assert!((0.0..=1.0).contains(&gov.one_commit_rate));
        assert!((0.0..=1.0).contains(&gov.gini_coefficient));
    }

    // ---- end-to-end via JSON records ----

    #[test]
    fn end_to_end_governance_analysis() {
        let records: Vec<serde_json::Value> = vec![
            serde_json::json!({"author": "alice", "date": "2023-01-15T00:00:00Z"}),
            serde_json::json!({"author": "alice", "date": "2023-01-20T00:00:00Z"}),
            serde_json::json!({"author": "alice", "date": "2023-02-01T00:00:00Z"}),
            serde_json::json!({"author": "bob", "date": "2023-01-25T00:00:00Z"}),
            serde_json::json!({"author": "bob", "date": "2023-02-10T00:00:00Z"}),
            serde_json::json!({"author": "carol", "date": "2023-03-01T00:00:00Z"}),
        ];

        let report = governance_analysis(&records, "$.author", "$.date");
        assert!(report.is_ok());
        let report = match report {
            Ok(r) => r,
            Err(_) => return,
        };

        assert_eq!(report.governance.unique_authors, 3);
        assert_eq!(report.governance.total_items, 6);
        assert!(report.governance.bus_factor_80 >= 1);
        assert!((0.0..=1.0).contains(&report.governance.gini_coefficient));
        assert!(!report.churn.monthly.is_empty());
    }

    #[test]
    fn empty_records_returns_error() {
        let records: Vec<serde_json::Value> = vec![];
        let result = governance_analysis(&records, "$.author", "$.date");
        assert!(result.is_err());
    }

    // ---- rendering smoke tests ----

    #[test]
    fn render_text_non_empty() {
        let report = GovernanceReport {
            governance: GovernanceMetrics {
                bus_factor_80: 4,
                bus_factor_50: 2,
                author_entropy: 3.71,
                gini_coefficient: 0.82,
                top1_share: 0.234,
                top3_share: 0.587,
                top5_share: 0.712,
                one_commit_rate: 0.591,
                unique_authors: 66,
                total_items: 800,
            },
            churn: ChurnMetrics {
                monthly: vec![MonthChurn {
                    month: "2023-01".to_owned(),
                    new: 5,
                    returning: 0,
                    churned: 0,
                    active: 5,
                    net: 5,
                }],
                mean_net_growth: -1.08,
                contributor_half_life_days: Some(14.0),
            },
        };
        let text = render_text(&report);
        assert!(text.contains("Bus factor (80%):"));
        assert!(text.contains("Gini coefficient:"));
    }

    #[test]
    fn render_markdown_non_empty() {
        let report = GovernanceReport {
            governance: GovernanceMetrics {
                bus_factor_80: 2,
                bus_factor_50: 1,
                author_entropy: 1.0,
                gini_coefficient: 0.5,
                top1_share: 0.5,
                top3_share: 1.0,
                top5_share: 1.0,
                one_commit_rate: 0.0,
                unique_authors: 2,
                total_items: 10,
            },
            churn: ChurnMetrics {
                monthly: vec![],
                mean_net_growth: 0.0,
                contributor_half_life_days: None,
            },
        };
        let md = render_markdown(&report);
        assert!(md.contains("# Governance Metrics"));
        assert!(md.contains("| Bus factor (80%)"));
    }
}

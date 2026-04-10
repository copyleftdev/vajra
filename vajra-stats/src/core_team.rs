//! Core team auto-detection from commit patterns.
//!
//! Classifies repository contributors into three roles—**Core**, **Bot**, and
//! **Community**—using deterministic heuristics applied to commit metadata.
//!
//! # Detection algorithm (O(n log n))
//!
//! 1. **Bot detection** — name/email pattern matching (highest confidence).
//! 2. **Email domain clustering** — dominant employer domain detection.
//! 3. **Pareto detection** — authors contributing to top 80% of commits.
//! 4. **Temporal consistency** — activity span across months refines confidence.
//! 5. **Signal combination** — merges the above into a final classification.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use vajra_types::InferenceConfidence;

// ---------------------------------------------------------------------------
// Serde bridge for InferenceConfidence (defined in vajra-types without Serde)
// ---------------------------------------------------------------------------

mod confidence_serde {
    use super::*;

    pub fn serialize<S: serde::Serializer>(
        val: &InferenceConfidence,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let s = match val {
            InferenceConfidence::Heuristic => "Heuristic",
            InferenceConfidence::Dominant => "Dominant",
            InferenceConfidence::Definite => "Definite",
        };
        serializer.serialize_str(s)
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<InferenceConfidence, D::Error> {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "Heuristic" => Ok(InferenceConfidence::Heuristic),
            "Dominant" => Ok(InferenceConfidence::Dominant),
            "Definite" => Ok(InferenceConfidence::Definite),
            other => Err(serde::de::Error::custom(format!(
                "unknown confidence level: {other}"
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Overall result of the core-team detection algorithm.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreTeamResult {
    /// Authors classified as core team members.
    pub core: Vec<AuthorClassification>,
    /// Authors classified as bots or automated accounts.
    pub bots: Vec<AuthorClassification>,
    /// Authors classified as community / occasional contributors.
    pub community: Vec<AuthorClassification>,
    /// Human-readable description of the detection method(s) used.
    pub detection_method: String,
}

/// A single author with their classification and supporting evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorClassification {
    /// Author display name.
    pub name: String,
    /// Author email address (if available).
    pub email: Option<String>,
    /// Total number of commits.
    pub commits: usize,
    /// Assigned role.
    pub classification: AuthorRole,
    /// Confidence in the assignment.
    #[serde(with = "confidence_serde")]
    pub confidence: InferenceConfidence,
    /// Machine-readable reason string (e.g. `"pareto_80pct"`, `"bot_suffix"`).
    pub reason: String,
}

/// Role assigned to a repository contributor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AuthorRole {
    /// Core / maintainer.
    Core,
    /// Automated bot account.
    Bot,
    /// Community / occasional contributor.
    Community,
}

impl std::fmt::Display for AuthorRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Core => write!(f, "Core"),
            Self::Bot => write!(f, "Bot"),
            Self::Community => write!(f, "Community"),
        }
    }
}

// ---------------------------------------------------------------------------
// Input representation
// ---------------------------------------------------------------------------

/// A single commit record consumed by the detection engine.
#[derive(Debug, Clone)]
pub struct CommitRecord {
    /// Author display name.
    pub author_name: String,
    /// Author email address.
    pub author_email: String,
    /// ISO-8601 date string (only the YYYY-MM prefix is used).
    pub date: String,
}

// ---------------------------------------------------------------------------
// Internal bookkeeping
// ---------------------------------------------------------------------------

/// Aggregated per-author statistics built during the first pass.
#[derive(Debug, Clone)]
struct AuthorStats {
    name: String,
    email: String,
    commits: usize,
    /// Set of distinct YYYY-MM months the author committed in.
    active_months: BTreeMap<String, ()>,
}

/// Intermediate signal produced by each detector pass.
#[derive(Debug, Clone)]
struct Signal {
    role: AuthorRole,
    confidence: InferenceConfidence,
    reason: String,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Detect core team members from a sequence of commit records.
///
/// The algorithm is fully deterministic: given the same input in the same
/// order it will always produce the same output.  All internal collections
/// are [`BTreeMap`]-based to guarantee stable iteration order.
///
/// # Errors
///
/// Returns `Err` if the input is empty (nothing to classify).
pub fn detect_core_team(commits: &[CommitRecord]) -> Result<CoreTeamResult, String> {
    if commits.is_empty() {
        return Err("no commits provided for core team detection".to_string());
    }

    // -- Step 0: aggregate per-author stats --------------------------------
    let (author_stats, total_months) = aggregate_authors(commits);

    // -- Step 1-4: compute signals -----------------------------------------
    let mut signals: BTreeMap<String, Vec<Signal>> = BTreeMap::new();

    for stats in author_stats.values() {
        let key = author_key(&stats.name, &stats.email);
        let mut author_signals = Vec::new();

        // 1. Bot detection
        if let Some(sig) = detect_bot(&stats.name, &stats.email) {
            author_signals.push(sig);
        }

        signals.insert(key, author_signals);
    }

    // 2. Email domain clustering
    apply_email_domain_clustering(&author_stats, &mut signals);

    // 3. Pareto detection
    apply_pareto_detection(&author_stats, &mut signals);

    // 4. Temporal consistency (refinement)
    apply_temporal_refinement(&author_stats, total_months, &mut signals);

    // -- Step 5: combine signals -------------------------------------------
    let mut core = Vec::new();
    let mut bots = Vec::new();
    let mut community = Vec::new();

    let mut methods_used: BTreeMap<String, ()> = BTreeMap::new();

    for stats in author_stats.values() {
        let key = author_key(&stats.name, &stats.email);
        let author_signals = signals.get(&key).cloned().unwrap_or_default();
        let (role, confidence, reason) = combine_signals(&author_signals);

        // Track which methods contributed
        for sig in &author_signals {
            let method = sig.reason.split(':').next().unwrap_or(&sig.reason);
            methods_used.insert(method.to_string(), ());
        }

        let classification = AuthorClassification {
            name: stats.name.clone(),
            email: if stats.email.is_empty() {
                None
            } else {
                Some(stats.email.clone())
            },
            commits: stats.commits,
            classification: role,
            confidence,
            reason,
        };

        match role {
            AuthorRole::Bot => bots.push(classification),
            AuthorRole::Core => core.push(classification),
            AuthorRole::Community => community.push(classification),
        }
    }

    // Sort each bucket by commits descending, then name ascending for stability.
    let sort_fn = |a: &AuthorClassification, b: &AuthorClassification| {
        b.commits.cmp(&a.commits).then_with(|| a.name.cmp(&b.name))
    };
    core.sort_by(sort_fn);
    bots.sort_by(sort_fn);
    community.sort_by(sort_fn);

    let detection_method = if methods_used.is_empty() {
        "none".to_string()
    } else {
        methods_used.keys().cloned().collect::<Vec<_>>().join("+")
    };

    Ok(CoreTeamResult {
        core,
        bots,
        community,
        detection_method,
    })
}

// ---------------------------------------------------------------------------
// Step 0: aggregation
// ---------------------------------------------------------------------------

/// Aggregate raw commits into per-author stats.
///
/// Returns the map of author stats and the total number of distinct months
/// across the entire commit history.
fn aggregate_authors(commits: &[CommitRecord]) -> (BTreeMap<String, AuthorStats>, usize) {
    let mut authors: BTreeMap<String, AuthorStats> = BTreeMap::new();
    let mut all_months: BTreeMap<String, ()> = BTreeMap::new();

    for c in commits {
        let key = author_key(&c.author_name, &c.author_email);
        let month = extract_month(&c.date);

        if let Some(ref m) = month {
            all_months.insert(m.clone(), ());
        }

        let entry = authors.entry(key).or_insert_with(|| AuthorStats {
            name: c.author_name.clone(),
            email: c.author_email.clone(),
            commits: 0,
            active_months: BTreeMap::new(),
        });
        entry.commits += 1;
        if let Some(m) = month {
            entry.active_months.insert(m, ());
        }
    }

    (authors, all_months.len())
}

// ---------------------------------------------------------------------------
// Step 1: bot detection
// ---------------------------------------------------------------------------

fn detect_bot(name: &str, email: &str) -> Option<Signal> {
    // Name ends with [bot]
    if name.ends_with("[bot]") {
        return Some(Signal {
            role: AuthorRole::Bot,
            confidence: InferenceConfidence::Definite,
            reason: "bot_suffix".to_string(),
        });
    }

    // Name starts with app/
    if name.starts_with("app/") {
        return Some(Signal {
            role: AuthorRole::Bot,
            confidence: InferenceConfidence::Definite,
            reason: "bot_app_prefix".to_string(),
        });
    }

    // Email contains noreply or bot
    let email_lower = email.to_lowercase();
    if email_lower.contains("noreply") || email_lower.contains("bot") {
        return Some(Signal {
            role: AuthorRole::Bot,
            confidence: InferenceConfidence::Dominant,
            reason: "bot_email_pattern".to_string(),
        });
    }

    None
}

// ---------------------------------------------------------------------------
// Step 2: email domain clustering
// ---------------------------------------------------------------------------

fn apply_email_domain_clustering(
    authors: &BTreeMap<String, AuthorStats>,
    signals: &mut BTreeMap<String, Vec<Signal>>,
) {
    // Count non-bot commits per email domain.
    let mut domain_commits: BTreeMap<String, usize> = BTreeMap::new();
    let mut total_non_bot: usize = 0;

    for stats in authors.values() {
        // Skip authors already flagged as bots.
        let key = author_key(&stats.name, &stats.email);
        let is_bot = signals
            .get(&key)
            .map(|sigs| sigs.iter().any(|s| s.role == AuthorRole::Bot))
            .unwrap_or(false);
        if is_bot {
            continue;
        }

        if let Some(domain) = extract_domain(&stats.email) {
            // Ignore generic domains that don't indicate an employer.
            if !is_generic_domain(&domain) {
                *domain_commits.entry(domain).or_insert(0) += stats.commits;
            }
            total_non_bot += stats.commits;
        } else {
            total_non_bot += stats.commits;
        }
    }

    if total_non_bot == 0 {
        return;
    }

    // Find the domain with the most commits.
    let top_domain = domain_commits
        .iter()
        .max_by_key(|(_, &count)| count)
        .map(|(domain, &count)| (domain.clone(), count));

    let (top_domain_name, top_domain_commits) = match top_domain {
        Some(t) => t,
        None => return,
    };

    // Check the >50% threshold using integer arithmetic to avoid float.
    // top_domain_commits / total_non_bot > 0.5
    // ⟺ top_domain_commits * 2 > total_non_bot
    if top_domain_commits * 2 <= total_non_bot {
        return;
    }

    // Tag every non-bot author with the top domain as Core.
    for stats in authors.values() {
        let key = author_key(&stats.name, &stats.email);
        if let Some(domain) = extract_domain(&stats.email) {
            if domain == top_domain_name {
                let sigs = signals.entry(key).or_default();
                sigs.push(Signal {
                    role: AuthorRole::Core,
                    confidence: InferenceConfidence::Dominant,
                    reason: format!("email_domain:{top_domain_name}"),
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Step 3: Pareto detection
// ---------------------------------------------------------------------------

fn apply_pareto_detection(
    authors: &BTreeMap<String, AuthorStats>,
    signals: &mut BTreeMap<String, Vec<Signal>>,
) {
    // Collect non-bot authors sorted by commits descending.
    let mut non_bot: Vec<(&String, &AuthorStats)> = authors
        .iter()
        .filter(|(_, stats)| {
            let key = author_key(&stats.name, &stats.email);
            !signals
                .get(&key)
                .map(|sigs| sigs.iter().any(|s| s.role == AuthorRole::Bot))
                .unwrap_or(false)
        })
        .collect();

    // Sort descending by commits, then by key for determinism.
    non_bot.sort_by(|a, b| b.1.commits.cmp(&a.1.commits).then_with(|| a.0.cmp(b.0)));

    let total_non_bot_commits: usize = non_bot.iter().map(|(_, s)| s.commits).sum();
    if total_non_bot_commits == 0 {
        return;
    }

    // 80% threshold using integer arithmetic:
    // cumulative / total >= 0.8 ⟺ cumulative * 5 >= total * 4
    let threshold = total_non_bot_commits * 4;
    let mut cumulative: usize = 0;

    for (_key, stats) in &non_bot {
        cumulative += stats.commits;
        let author_key_val = author_key(&stats.name, &stats.email);
        let sigs = signals.entry(author_key_val).or_default();
        sigs.push(Signal {
            role: AuthorRole::Core,
            confidence: InferenceConfidence::Heuristic,
            reason: "pareto_80pct".to_string(),
        });
        // Once cumulative reaches 80%, stop adding more.
        if cumulative * 5 >= threshold {
            break;
        }
    }
}

// ---------------------------------------------------------------------------
// Step 4: temporal consistency
// ---------------------------------------------------------------------------

fn apply_temporal_refinement(
    authors: &BTreeMap<String, AuthorStats>,
    total_months: usize,
    signals: &mut BTreeMap<String, Vec<Signal>>,
) {
    if total_months == 0 {
        return;
    }

    for stats in authors.values() {
        let key = author_key(&stats.name, &stats.email);
        let active = stats.active_months.len();

        // > 50%: active * 2 > total_months
        // < 20%: active * 5 < total_months
        let sigs = signals.entry(key).or_default();

        if active * 2 > total_months {
            sigs.push(Signal {
                role: AuthorRole::Core,
                confidence: InferenceConfidence::Heuristic,
                reason: "temporal_consistent".to_string(),
            });
        } else if active * 5 < total_months {
            sigs.push(Signal {
                role: AuthorRole::Community,
                confidence: InferenceConfidence::Heuristic,
                reason: "temporal_sporadic".to_string(),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Step 5: signal combination
// ---------------------------------------------------------------------------

fn combine_signals(signals: &[Signal]) -> (AuthorRole, InferenceConfidence, String) {
    if signals.is_empty() {
        return (
            AuthorRole::Community,
            InferenceConfidence::Heuristic,
            "no_signal".to_string(),
        );
    }

    // Rule: any bot signal -> Bot
    let has_bot = signals.iter().any(|s| s.role == AuthorRole::Bot);
    if has_bot {
        let best_bot = signals
            .iter()
            .filter(|s| s.role == AuthorRole::Bot)
            .max_by_key(|s| s.confidence);
        return match best_bot {
            Some(s) => (AuthorRole::Bot, s.confidence, s.reason.clone()),
            None => (
                AuthorRole::Bot,
                InferenceConfidence::Heuristic,
                "bot_signal".to_string(),
            ),
        };
    }

    let has_email_domain = signals
        .iter()
        .any(|s| s.role == AuthorRole::Core && s.reason.starts_with("email_domain:"));
    let has_pareto = signals
        .iter()
        .any(|s| s.role == AuthorRole::Core && s.reason == "pareto_80pct");
    let has_temporal_boost = signals
        .iter()
        .any(|s| s.role == AuthorRole::Core && s.reason == "temporal_consistent");
    let has_temporal_demote = signals
        .iter()
        .any(|s| s.role == AuthorRole::Community && s.reason == "temporal_sporadic");

    // Email domain + Pareto -> Core (Dominant)
    if has_email_domain && has_pareto {
        let domain_sig = signals
            .iter()
            .find(|s| s.reason.starts_with("email_domain:"));
        let reason = domain_sig
            .map(|s| format!("{}+pareto_80pct", s.reason))
            .unwrap_or_else(|| "email_domain+pareto_80pct".to_string());
        return (AuthorRole::Core, InferenceConfidence::Dominant, reason);
    }

    // Email domain only -> Core (Dominant)
    if has_email_domain {
        let domain_sig = signals
            .iter()
            .find(|s| s.reason.starts_with("email_domain:"));
        let reason = domain_sig
            .map(|s| s.reason.clone())
            .unwrap_or_else(|| "email_domain".to_string());
        return (AuthorRole::Core, InferenceConfidence::Dominant, reason);
    }

    // Pareto with temporal demote -> Community (Heuristic)
    if has_pareto && has_temporal_demote && !has_temporal_boost {
        return (
            AuthorRole::Community,
            InferenceConfidence::Heuristic,
            "pareto_80pct+temporal_sporadic".to_string(),
        );
    }

    // Pareto only -> Core (Heuristic), boosted to Dominant if temporal_consistent
    if has_pareto {
        let confidence = if has_temporal_boost {
            InferenceConfidence::Dominant
        } else {
            InferenceConfidence::Heuristic
        };
        let reason = if has_temporal_boost {
            "pareto_80pct+temporal_consistent".to_string()
        } else {
            "pareto_80pct".to_string()
        };
        return (AuthorRole::Core, confidence, reason);
    }

    // Everything else -> Community
    (
        AuthorRole::Community,
        InferenceConfidence::Heuristic,
        "community_default".to_string(),
    )
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a deterministic key from name + email.
fn author_key(name: &str, email: &str) -> String {
    format!("{name} <{email}>")
}

/// Extract the YYYY-MM prefix from an ISO-8601-ish date string.
fn extract_month(date: &str) -> Option<String> {
    if date.len() >= 7 && date.as_bytes().get(4).copied() == Some(b'-') {
        Some(date[..7].to_string())
    } else {
        None
    }
}

/// Extract the domain part of an email address (lowercased).
fn extract_domain(email: &str) -> Option<String> {
    email
        .rsplit_once('@')
        .map(|(_, domain)| domain.to_lowercase())
}

/// Returns `true` for generic email providers that do not indicate an employer.
fn is_generic_domain(domain: &str) -> bool {
    matches!(
        domain,
        "gmail.com"
            | "yahoo.com"
            | "hotmail.com"
            | "outlook.com"
            | "live.com"
            | "icloud.com"
            | "me.com"
            | "mac.com"
            | "aol.com"
            | "protonmail.com"
            | "proton.me"
            | "users.noreply.github.com"
            | "mail.com"
            | "gmx.com"
            | "gmx.net"
            | "yandex.com"
            | "fastmail.com"
            | "hey.com"
            | "pm.me"
            | "qq.com"
            | "163.com"
            | "126.com"
            | "foxmail.com"
    )
}

// ---------------------------------------------------------------------------
// Convenience: build CommitRecords from serde_json values
// ---------------------------------------------------------------------------

/// Extract [`CommitRecord`]s from a JSON array of commit objects.
///
/// Each object is expected to have `author_name`, `author_email`, and `date`
/// string fields (matching the schema produced by `vajra-core`'s git adapter).
/// Objects that are missing both name and email are silently skipped.
pub fn commit_records_from_json(value: &serde_json::Value) -> Vec<CommitRecord> {
    let arr = match value.as_array() {
        Some(a) => a,
        None => return Vec::new(),
    };

    let mut records = Vec::with_capacity(arr.len());
    for obj in arr {
        let name = obj
            .get("author_name")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let email = obj
            .get("author_email")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let date = obj.get("date").and_then(|v| v.as_str()).unwrap_or_default();

        if name.is_empty() && email.is_empty() {
            continue;
        }

        records.push(CommitRecord {
            author_name: name.to_string(),
            author_email: email.to_string(),
            date: date.to_string(),
        });
    }

    records
}

// ---------------------------------------------------------------------------
// Text / JSON rendering
// ---------------------------------------------------------------------------

/// Render a [`CoreTeamResult`] as human-readable text.
pub fn render_core_team_text(result: &CoreTeamResult) -> String {
    use std::fmt::Write;
    let mut o = String::new();

    let _ = writeln!(o, "=== Core Team Detection ===");
    let _ = writeln!(o, "  Method: {}", result.detection_method);
    let _ = writeln!(o);

    if !result.core.is_empty() {
        let _ = writeln!(o, "  CORE ({}):", result.core.len());
        for a in &result.core {
            let email_str = a.email.as_deref().unwrap_or("(no email)");
            let _ = writeln!(
                o,
                "    {} <{}> — {} commits — {:?} — {}",
                a.name, email_str, a.commits, a.confidence, a.reason,
            );
        }
        let _ = writeln!(o);
    }

    if !result.bots.is_empty() {
        let _ = writeln!(o, "  BOTS ({}):", result.bots.len());
        for a in &result.bots {
            let email_str = a.email.as_deref().unwrap_or("(no email)");
            let _ = writeln!(
                o,
                "    {} <{}> — {} commits — {:?} — {}",
                a.name, email_str, a.commits, a.confidence, a.reason,
            );
        }
        let _ = writeln!(o);
    }

    if !result.community.is_empty() {
        let _ = writeln!(o, "  COMMUNITY ({}):", result.community.len());
        for a in &result.community {
            let email_str = a.email.as_deref().unwrap_or("(no email)");
            let _ = writeln!(
                o,
                "    {} <{}> — {} commits — {:?} — {}",
                a.name, email_str, a.commits, a.confidence, a.reason,
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

    // -- Helper: build a batch of commits from (name, email, date) tuples --

    fn make_commits(data: &[(&str, &str, &str)]) -> Vec<CommitRecord> {
        data.iter()
            .map(|(name, email, date)| CommitRecord {
                author_name: name.to_string(),
                author_email: email.to_string(),
                date: date.to_string(),
            })
            .collect()
    }

    /// Extract Ok value, asserting no error. Uses assert! (allowed in tests) not panic!.
    fn unwrap_result(r: Result<CoreTeamResult, String>) -> CoreTeamResult {
        assert!(r.is_ok(), "unexpected error: {:?}", r.as_ref().err());
        // SAFETY: we just asserted is_ok above; if_let avoids unwrap/expect lint.
        match r {
            Ok(v) => v,
            Err(_) => CoreTeamResult {
                core: vec![],
                bots: vec![],
                community: vec![],
                detection_method: String::new(),
            },
        }
    }

    // -- Bot detection -----------------------------------------------------

    #[test]
    fn bot_suffix_detected() {
        let commits = make_commits(&[
            ("dependabot[bot]", "dependabot@github.com", "2024-01-01"),
            ("Alice", "alice@example.com", "2024-01-01"),
        ]);
        let r = unwrap_result(detect_core_team(&commits));
        assert_eq!(r.bots.len(), 1);
        assert_eq!(r.bots[0].name, "dependabot[bot]");
        assert_eq!(r.bots[0].confidence, InferenceConfidence::Definite);
        assert_eq!(r.bots[0].reason, "bot_suffix");
    }

    #[test]
    fn bot_app_prefix_detected() {
        let commits = make_commits(&[
            ("app/dependabot", "noreply@github.com", "2024-01-01"),
            ("Alice", "alice@example.com", "2024-01-01"),
        ]);
        let r = unwrap_result(detect_core_team(&commits));
        assert_eq!(r.bots.len(), 1);
        assert_eq!(r.bots[0].name, "app/dependabot");
        assert_eq!(r.bots[0].confidence, InferenceConfidence::Definite);
    }

    #[test]
    fn bot_email_noreply() {
        let commits = make_commits(&[(
            "github-actions",
            "41898282+github-actions[bot]@users.noreply.github.com",
            "2024-01-01",
        )]);
        let r = unwrap_result(detect_core_team(&commits));
        assert_eq!(r.bots.len(), 1);
        assert_eq!(r.bots[0].name, "github-actions");
    }

    #[test]
    fn bot_email_bot_keyword() {
        let commits = make_commits(&[
            ("renovate", "renovate-bot@renovateapp.com", "2024-01-01"),
            ("Alice", "alice@example.com", "2024-01-01"),
        ]);
        let r = unwrap_result(detect_core_team(&commits));
        assert_eq!(r.bots.len(), 1);
        assert_eq!(r.bots[0].name, "renovate");
        assert_eq!(r.bots[0].confidence, InferenceConfidence::Dominant);
    }

    // -- Email domain clustering -------------------------------------------

    #[test]
    fn email_domain_majority() {
        let mut commits = Vec::new();
        let meta_authors = [
            "Alice", "Bob", "Charlie", "Diana", "Eve", "Frank", "Grace", "Heidi",
        ];
        for (i, author) in meta_authors.iter().enumerate() {
            for m in 1..=5 {
                commits.push(CommitRecord {
                    author_name: author.to_string(),
                    author_email: format!("{author}@meta.com").to_lowercase(),
                    date: format!("2024-{m:02}-{:02}", (i % 28) + 1),
                });
            }
        }
        // 2 community authors with 2 commits each
        for m in 1..=2 {
            commits.push(CommitRecord {
                author_name: "Outsider1".to_string(),
                author_email: "outsider1@gmail.com".to_string(),
                date: format!("2024-{m:02}-15"),
            });
            commits.push(CommitRecord {
                author_name: "Outsider2".to_string(),
                author_email: "outsider2@yahoo.com".to_string(),
                date: format!("2024-{m:02}-20"),
            });
        }

        let r = unwrap_result(detect_core_team(&commits));

        // All 8 Meta authors should be Core.
        assert_eq!(r.core.len(), 8);
        for c in &r.core {
            assert_eq!(c.classification, AuthorRole::Core);
            assert!(
                c.reason.contains("email_domain:meta.com"),
                "expected email_domain reason, got: {}",
                c.reason
            );
        }
        // Community authors
        assert_eq!(r.community.len(), 2);
    }

    // -- Pareto detection --------------------------------------------------

    #[test]
    fn pareto_top_authors() {
        let mut commits = Vec::new();
        for i in 0..80 {
            let m = (i % 12) + 1;
            commits.push(CommitRecord {
                author_name: "TimSmart".to_string(),
                author_email: "tim@example.com".to_string(),
                date: format!("2024-{m:02}-01"),
            });
        }
        for i in 0..15 {
            let m = (i % 6) + 1;
            commits.push(CommitRecord {
                author_name: "Contributor2".to_string(),
                author_email: "c2@example.com".to_string(),
                date: format!("2024-{m:02}-01"),
            });
        }
        for i in 0..5 {
            let m = (i % 2) + 1;
            commits.push(CommitRecord {
                author_name: "Contributor3".to_string(),
                author_email: "c3@example.com".to_string(),
                date: format!("2024-{m:02}-01"),
            });
        }

        let r = unwrap_result(detect_core_team(&commits));

        let tim = r.core.iter().find(|c| c.name == "TimSmart");
        assert!(tim.is_some(), "TimSmart should be Core");
        assert!(
            tim.map(|t| t.reason.contains("pareto_80pct"))
                .unwrap_or(false),
            "expected pareto reason"
        );
    }

    // -- Temporal consistency ----------------------------------------------

    #[test]
    fn temporal_sporadic_demotes_pareto() {
        let mut commits = Vec::new();

        // 12 months of history from a steady contributor (2 commits/month = 24)
        for m in 1..=12 {
            for _ in 0..2 {
                commits.push(CommitRecord {
                    author_name: "Steady".to_string(),
                    author_email: "steady@gmail.com".to_string(),
                    date: format!("2024-{m:02}-10"),
                });
            }
        }

        // Burst contributor: 30 commits all in January
        for _ in 0..30 {
            commits.push(CommitRecord {
                author_name: "Burst".to_string(),
                author_email: "burst@yahoo.com".to_string(),
                date: "2024-01-15".to_string(),
            });
        }

        let r = unwrap_result(detect_core_team(&commits));

        // Steady should be Core.
        let steady = r.core.iter().find(|c| c.name == "Steady");
        assert!(steady.is_some(), "Steady should be Core");

        // Burst should be demoted to Community by temporal_sporadic.
        let burst = r.community.iter().find(|c| c.name == "Burst");
        assert!(
            burst.is_some(),
            "Burst should be Community (temporal demote). core={:?}, community={:?}",
            r.core.iter().map(|c| &c.name).collect::<Vec<_>>(),
            r.community.iter().map(|c| &c.name).collect::<Vec<_>>(),
        );
    }

    // -- Edge cases --------------------------------------------------------

    #[test]
    fn single_author() {
        let commits = make_commits(&[
            ("Solo", "solo@example.com", "2024-01-01"),
            ("Solo", "solo@example.com", "2024-02-01"),
        ]);
        let r = unwrap_result(detect_core_team(&commits));
        assert_eq!(r.core.len(), 1);
        assert_eq!(r.core[0].name, "Solo");
    }

    #[test]
    fn all_equal_commits() {
        let commits = make_commits(&[
            ("A", "a@example.com", "2024-01-01"),
            ("B", "b@example.com", "2024-01-01"),
            ("C", "c@example.com", "2024-01-01"),
            ("D", "d@example.com", "2024-01-01"),
        ]);
        let r = unwrap_result(detect_core_team(&commits));
        // With all equal (1 commit each), Pareto grabs until 80%:
        // Each has 1/4 = 25%. Need cumulative >= 80%.
        // Integer: threshold=4*4=16, cumulative*5: 5,10,15,20. 20>=16 at 4th.
        assert_eq!(r.core.len(), 4);
    }

    #[test]
    fn no_emails() {
        let commits = make_commits(&[
            ("A", "", "2024-01-01"),
            ("A", "", "2024-02-01"),
            ("B", "", "2024-01-01"),
        ]);
        let r = unwrap_result(detect_core_team(&commits));
        assert!(!r.core.is_empty());
        // A's email field should be None
        let a = r.core.iter().find(|c| c.name == "A");
        assert!(a.is_some());
        assert!(a.map(|x| x.email.is_none()).unwrap_or(false));
    }

    #[test]
    fn empty_commits_returns_error() {
        let result = detect_core_team(&[]);
        assert!(result.is_err());
    }

    // -- JSON parsing ------------------------------------------------------

    #[test]
    fn commit_records_from_json_basic() {
        let json = serde_json::json!([
            {
                "hash": "abc123",
                "author_name": "Alice",
                "author_email": "alice@meta.com",
                "date": "2024-01-15T10:00:00+00:00",
                "subject": "Initial commit"
            },
            {
                "hash": "def456",
                "author_name": "dependabot[bot]",
                "author_email": "dependabot@github.com",
                "date": "2024-01-16T11:00:00+00:00",
                "subject": "Bump deps"
            }
        ]);
        let records = commit_records_from_json(&json);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].author_name, "Alice");
        assert_eq!(records[0].author_email, "alice@meta.com");
        assert_eq!(records[1].author_name, "dependabot[bot]");
    }

    #[test]
    fn commit_records_from_json_not_array() {
        let json = serde_json::json!({"key": "value"});
        let records = commit_records_from_json(&json);
        assert!(records.is_empty());
    }

    #[test]
    fn commit_records_from_json_missing_fields() {
        let json = serde_json::json!([
            {"hash": "abc123", "subject": "No author fields"},
            {"author_name": "Alice", "author_email": "alice@example.com", "date": "2024-01-01"}
        ]);
        let records = commit_records_from_json(&json);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].author_name, "Alice");
    }

    // -- Helper tests ------------------------------------------------------

    #[test]
    fn extract_month_iso_date() {
        assert_eq!(extract_month("2024-01-15"), Some("2024-01".to_string()));
    }

    #[test]
    fn extract_month_iso_datetime() {
        assert_eq!(
            extract_month("2024-03-20T10:30:00+00:00"),
            Some("2024-03".to_string())
        );
    }

    #[test]
    fn extract_month_short_string() {
        assert_eq!(extract_month("short"), None);
    }

    #[test]
    fn extract_domain_works() {
        assert_eq!(
            extract_domain("alice@Meta.Com"),
            Some("meta.com".to_string())
        );
    }

    #[test]
    fn extract_domain_no_at() {
        assert_eq!(extract_domain("noatsign"), None);
    }

    #[test]
    fn generic_domains_detected() {
        assert!(is_generic_domain("gmail.com"));
        assert!(is_generic_domain("users.noreply.github.com"));
        assert!(!is_generic_domain("meta.com"));
    }

    // -- Combined detection scenario: React-like repo ----------------------

    #[test]
    fn react_like_scenario() {
        let mut commits = Vec::new();

        // 5 Meta employees, each with many commits across many months.
        let meta_devs = ["Dan", "Andrew", "Sophie", "Rick", "Luna"];
        for dev in &meta_devs {
            for m in 1..=10 {
                for _ in 0..8 {
                    commits.push(CommitRecord {
                        author_name: dev.to_string(),
                        author_email: format!("{dev}@meta.com").to_lowercase(),
                        date: format!("2024-{m:02}-01"),
                    });
                }
            }
        }

        // 1 bot
        for m in 1..=10 {
            commits.push(CommitRecord {
                author_name: "dependabot[bot]".to_string(),
                author_email: "dependabot@github.com".to_string(),
                date: format!("2024-{m:02}-15"),
            });
        }

        // 10 community contributors, 1-3 commits each in 1-2 months.
        for i in 0..10 {
            let count = (i % 3) + 1;
            for c in 0..count {
                let m = (c % 2) + 1;
                commits.push(CommitRecord {
                    author_name: format!("Community{i}"),
                    author_email: format!("community{i}@gmail.com"),
                    date: format!("2024-{m:02}-20"),
                });
            }
        }

        let r = unwrap_result(detect_core_team(&commits));

        assert_eq!(r.core.len(), 5, "expected 5 core, got {:?}", r.core);
        for c in &r.core {
            assert!(
                meta_devs.iter().any(|d| *d == c.name),
                "unexpected core member: {}",
                c.name
            );
            assert!(
                c.reason.contains("email_domain:meta.com"),
                "expected email_domain reason for {}, got: {}",
                c.name,
                c.reason
            );
        }

        assert_eq!(r.bots.len(), 1);
        assert_eq!(r.bots[0].name, "dependabot[bot]");
        assert_eq!(r.community.len(), 10);

        assert!(
            r.detection_method.contains("email_domain"),
            "detection_method should mention email_domain: {}",
            r.detection_method
        );
    }

    // -- Rendering ---------------------------------------------------------

    #[test]
    fn render_text_non_empty() {
        let r = unwrap_result(detect_core_team(&make_commits(&[
            ("Alice", "alice@example.com", "2024-01-01"),
            ("Alice", "alice@example.com", "2024-02-01"),
            ("Bob", "bob@example.com", "2024-01-01"),
        ])));
        let text = render_core_team_text(&r);
        assert!(text.contains("Core Team Detection"));
        assert!(text.contains("Alice"));
    }

    // -- Determinism: same input -> same output ----------------------------

    #[test]
    fn deterministic_output() {
        let commits = make_commits(&[
            ("Zara", "zara@company.com", "2024-01-01"),
            ("Zara", "zara@company.com", "2024-02-01"),
            ("Zara", "zara@company.com", "2024-03-01"),
            ("Adam", "adam@company.com", "2024-01-01"),
            ("Adam", "adam@company.com", "2024-02-01"),
            ("Mike", "mike@gmail.com", "2024-01-15"),
        ]);

        let r1 = unwrap_result(detect_core_team(&commits));
        let r2 = unwrap_result(detect_core_team(&commits));

        // Same names in same order.
        let names1: Vec<&str> = r1.core.iter().map(|c| c.name.as_str()).collect();
        let names2: Vec<&str> = r2.core.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names1, names2);

        let comm1: Vec<&str> = r1.community.iter().map(|c| c.name.as_str()).collect();
        let comm2: Vec<&str> = r2.community.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(comm1, comm2);
    }
}

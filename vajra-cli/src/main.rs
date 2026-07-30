mod batch;
mod corpus;
mod fields;
mod hints;
mod render;
mod treediff;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;

use vajra_anomaly::{AnomalyAnalyzer, AnomalyReport};
use vajra_core::input::load_documents_aggregated;
use vajra_core::InputFormat;
use vajra_drift::full_drift;
use vajra_essence::{
    AiProfile, AuditorProfile, CustomProfile, EngineerProfile, EssenceBuilder, FraudProfile,
    HealthProfile, StaffProfile,
};
use vajra_fingerprint::{
    cluster_documents_with_threshold, FingerprintAnalyzer, FingerprintResult,
    StreamingFingerprintAccumulator,
};
use vajra_stats::{
    commit_records_from_json, detect_core_team, extract_json_path, governance_analysis,
    linear_regression, render_governance_markdown, render_governance_text,
    shannon_entropy_from_counts, StatsAnalyzer, StatsResult, StreamingStatsAccumulator,
};
use vajra_types::scoring::{compute_health_score, HealthMetrics, HealthScore, HealthWeights};
use vajra_types::traits::{ConcernProfile, OutputFormat};
use vajra_types::{Analyzer, Document};

// ---------------------------------------------------------------------------
// CLI argument definitions
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(
    name = "vajra",
    version,
    about = "Structural analysis toolkit for JSON data"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Output format
    #[arg(long, global = true, default_value = "text")]
    format: Format,

    /// Concern profile (used by essence command)
    #[arg(long, global = true, default_value = "engineer")]
    profile: String,

    /// Suppress progress output
    #[arg(long, global = true)]
    quiet: bool,

    /// Show score decomposition (used by essence command)
    #[arg(long, global = true)]
    explain: bool,

    /// Path to TOML config file with custom profile definitions
    #[arg(long, global = true)]
    config: Option<String>,

    /// Apply built-in redaction patterns before output
    #[arg(long, global = true)]
    redact: bool,

    /// Token budget for essence output (approximate max tokens)
    #[arg(long, global = true)]
    budget: Option<usize>,

    /// Use the sketch-based accumulators (NOT yet bounded memory — see #102)
    #[arg(long, global = true)]
    streaming: bool,

    /// Force input format instead of auto-detecting (json, ndjson, yaml, csv, tsv, markdown, pdf, cpuprofile, strace, source)
    #[arg(long, global = true)]
    input_format: Option<InputFormatArg>,

    /// Source code language (rust, python, javascript, go, etc.) — used with source code input
    #[arg(long, global = true)]
    lang: Option<String>,

    /// Include semantic labels on tree-sitter nodes (function, class, import, etc.) — used with source code input
    #[arg(long, global = true)]
    semantic_paths: bool,

    /// Maximum number of git commits to extract (used with --input-format git)
    #[arg(long, global = true, default_value = "500")]
    git_limit: usize,

    /// Git branch or revision to read (used with --input-format git, default: HEAD)
    #[arg(long, global = true)]
    git_branch: Option<String>,
}

#[derive(Clone, Copy, ValueEnum)]
enum InputFormatArg {
    Json,
    Ndjson,
    Yaml,
    Csv,
    Tsv,
    Markdown,
    Pdf,
    Cpuprofile,
    Strace,
    Source,
    Git,
}

#[derive(Clone, Copy, ValueEnum)]
enum WindowArg {
    Month,
    Week,
    Day,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Text,
    Json,
    Markdown,
    CompactAi,
}

#[derive(Subcommand)]
enum Command {
    /// Full structural analysis
    Inspect {
        /// Path to JSON file, or `-` for stdin
        input: String,
    },
    /// Statistical summary
    Stats {
        /// Path to JSON file, or `-` for stdin
        input: String,
        /// Time-series window granularity (month, week, day)
        #[arg(long)]
        window: Option<WindowArg>,
        /// JSONPath to the timestamp field (e.g. '$.date')
        #[arg(long)]
        time_field: Option<String>,
    },
    /// Anomaly detection
    Anomalies {
        /// Path to JSON file, or `-` for stdin
        input: String,
    },
    /// Structural fingerprints
    Fingerprint {
        /// Path to JSON file, or `-` for stdin
        input: String,
        /// Withhold hashes for documents with fewer than N nodes (0 = never)
        #[arg(long, default_value_t = 0)]
        min_nodes: u64,
        /// Index a directory tree instead, reporting which shapes recur across documents
        #[arg(long)]
        corpus: bool,
        /// Path components below the corpus root that identify one unit for clustering
        #[arg(long, default_value_t = 1)]
        corpus_group_depth: usize,
    },
    /// Generate concern-oriented essence
    Essence {
        /// Path to JSON file, or `-` for stdin
        input: String,
    },
    /// Detect drift between two JSON documents (or population-level drift with --group-by)
    Drift {
        /// Baseline JSON file (or single file when using --group-by)
        baseline: String,
        /// Candidate JSON file to compare (omit when using --group-by)
        candidate: Option<String>,
        /// JSONPath field to partition records by for population-level drift
        #[arg(long)]
        group_by: Option<String>,
        /// Compare two directory trees structurally, file by file
        #[arg(long)]
        tree: bool,
    },
    /// Cluster similar documents in a batch
    Cluster {
        /// Files or directories to cluster
        #[arg(required = true, num_args = 1..)]
        inputs: Vec<String>,
        /// Jaccard similarity above which documents are grouped (0.0-1.0)
        #[arg(long, default_value_t = 0.5)]
        similarity_threshold: f64,
    },
    /// Evaluate how well each field separates a labelled corpus
    Separation {
        /// Path to JSON file, or `-` for stdin
        input: String,
        /// Field holding the ground-truth label (e.g. `label` or `$.label`)
        #[arg(long)]
        label_field: String,
        /// Assumed population prevalence of the positive class, for priced precision
        #[arg(long)]
        base_rate: Option<f64>,
        /// Which label value is the positive class (default: first by name)
        #[arg(long)]
        positive_class: Option<String>,
        /// Maximum number of features to report (0 = all)
        #[arg(long, default_value_t = 0)]
        top_k: usize,
    },
    /// Discover cross-field relationships
    Invariants {
        /// Path to JSON file, or `-` for stdin
        input: String,
        /// Maximum number of field pairs to consider
        #[arg(long, default_value = "50")]
        top_k: usize,
        /// How to discretise numeric fields: `quantile:N`, `equal-width:N`, or `none`
        #[arg(long, default_value = "quantile:5")]
        bin: String,
    },
    /// Run a query expression against a document
    Query {
        /// Path to JSON file, or `-` for stdin
        input: String,
        /// Query expression (e.g., 'entropy($.claims[*].status) > 0.5')
        expression: String,
    },
    /// Parallel batch analysis of all JSON files in a directory
    Batch {
        /// Directory containing JSON files to analyze
        directory: String,
    },
    /// Detect temporal cause-effect chains
    Cascade {
        /// Path to JSON file
        input: String,
        #[arg(long, default_value = "file")]
        entity_field: String,
        #[arg(long, default_value = "date")]
        time_field: String,
        #[arg(long, default_value = "intent")]
        event_field: String,
        #[arg(long, default_value = "fix,revert")]
        response_values: String,
    },
    /// Automated health scoring with letter grades
    Score {
        /// Path to JSON file, or `-` for stdin
        input: String,
        /// JSONPath to the author/contributor field (default: '$.author', or '$.author_name' for git input)
        #[arg(long)]
        author_field: Option<String>,
        /// JSONPath to the timestamp field (e.g. '$.date')
        #[arg(long, default_value = "$.date")]
        time_field: String,
        /// JSONPath to the commit message field (default: '$.message', or '$.subject' for git input)
        #[arg(long)]
        message_field: Option<String>,
        /// JSONPath to the issue comments count field (e.g. '$.comments')
        #[arg(long)]
        comments_field: Option<String>,
        /// Merge contributor aliases that share a name or an email before analysing
        #[arg(long)]
        resolve_identities: bool,
        /// JSONPath to the author email field, used only with --resolve-identities
        #[arg(long)]
        email_field: Option<String>,
    },
    /// List all available profiles (built-in and custom)
    Profiles,
    /// Governance metrics: bus factor, merge equity, contributor churn
    Governance {
        /// Path to JSON file, or `-` for stdin
        input: String,
        /// JSONPath to the author field (default: '$.author', or '$.author_name' for git input)
        #[arg(long)]
        author_field: Option<String>,
        /// JSONPath to the timestamp field (e.g. '$.date')
        #[arg(long, default_value = "$.date")]
        time_field: String,
        /// Merge contributor aliases that share a name or an email before analysing
        #[arg(long)]
        resolve_identities: bool,
        /// JSONPath to the author email field, used only with --resolve-identities
        #[arg(long)]
        email_field: Option<String>,
    },
    /// Ingest GitHub repository data (PRs, issues, commits, releases) via gh CLI
    IngestGithub {
        /// Owner/repo identifier (e.g. 'facebook/react')
        repo: String,
        /// Output directory for ingested JSON files
        #[arg(long, default_value = ".")]
        output: PathBuf,
        /// Maximum number of pull requests to fetch
        #[arg(long, default_value = "500")]
        pr_limit: usize,
        /// Maximum number of issues to fetch
        #[arg(long, default_value = "500")]
        issue_limit: usize,
        /// Maximum number of commits to fetch
        #[arg(long, default_value = "800")]
        commit_limit: usize,
    },
    /// Detect core team members from commit patterns
    CoreTeam {
        /// Path to JSON file containing commit data, or `-` for stdin
        input: String,
        /// Merge contributor aliases that share a name or an email before analysing
        #[arg(long)]
        resolve_identities: bool,
    },
    /// Generate an HTML analysis report from pre-computed JSON files
    Report {
        /// Directory containing analysis JSON files (stats.json, anomalies.json, etc.)
        input: String,
        /// Report title
        #[arg(long, default_value = "Analysis Report")]
        title: String,
        /// Output HTML file path
        #[arg(long, default_value = "report.html")]
        output: String,
        /// Repository name (e.g. 'facebook/react')
        #[arg(long)]
        repo_name: Option<String>,
    },
    /// Cross-repo comparison: multi-project benchmarking
    Compare {
        /// Two or more JSON file paths to compare
        #[arg(required = true, num_args = 2..)]
        inputs: Vec<String>,
        /// Comma-separated labels for each dataset (default: filenames)
        #[arg(long)]
        labels: Option<String>,
        /// JSONPath to the author field (default: '$.author', or '$.author_name' for git input)
        #[arg(long)]
        author_field: Option<String>,
        /// JSONPath to the timestamp field (e.g. '$.date')
        #[arg(long, default_value = "$.date")]
        time_field: String,
        /// JSONPath to the commit message field (default: '$.message', or '$.subject' for git input)
        #[arg(long)]
        message_field: Option<String>,
    },
    /// One-command audit: ingest, analyze, and generate an HTML report for a GitHub repo
    Audit {
        /// Repository URL or owner/repo (e.g. 'github.com/owner/repo' or 'owner/repo')
        repo: String,
        /// Output HTML report path (default: '{owner}-{repo}-report.html')
        #[arg(long)]
        output: Option<String>,
        /// Maximum number of commits to fetch
        #[arg(long, default_value = "800")]
        commit_limit: usize,
        /// Maximum number of pull requests to fetch
        #[arg(long, default_value = "500")]
        pr_limit: usize,
        /// Maximum number of issues to fetch
        #[arg(long, default_value = "500")]
        issue_limit: usize,
    },
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Which commands genuinely render which formats.
///
/// Every command implements `text` and `json`. `markdown` and `compact-ai`
/// need a real renderer, and most commands do not yet have one — for those,
/// output is the text format verbatim.
///
/// Migrating a command onto [`render::Report`] is what moves it into this
/// table, so the notice below can never claim more than is true. Silently
/// accepting a format and ignoring it is the same failure as reporting
/// `errors: []` over a partial batch: the caller cannot distinguish "rendered
/// as Markdown" from "fell back to text".
/// Markdown is implemented for every command, so the notice never fires for it.
///
/// Kept as an explicit list rather than removed: the machinery is what stops the
/// claim drifting, and `compact-ai` still needs it. Tests assert both that every
/// listed pair genuinely renders and that unlisted pairs genuinely fall back.
const RENDERS_MARKDOWN: &[&str] = &[
    "anomalies",
    "batch",
    "cascade",
    "cluster",
    "compare",
    "core-team",
    "drift",
    "essence",
    "fingerprint",
    "governance",
    "inspect",
    "invariants",
    "score",
    "separation",
    "stats",
];

/// Commands with a bespoke compact-AI view.
///
/// `cascade` and `score` predate the shared renderer and hand-roll their own;
/// they are listed because they genuinely produce distinct output, verified
/// empirically by `format_honesty.rs` rather than by reading the match arms.
const RENDERS_COMPACT_AI: &[&str] = &["cascade", "compare", "essence", "score"];

/// Warn when the requested format is not actually implemented for this command.
/// Email selectors tried when `--email-field` is absent.
const EMAIL_CANDIDATES: fields::Candidates = fields::Candidates {
    flag: "--email-field",
    options: &["$.author_email", "$.email"],
};

/// Rewrite each record's author to its canonical identity.
///
/// Git records a `(name, email)` pair chosen per commit, so one person appears
/// under several. Keying on a single field counts them separately, which moves
/// every concentration metric — deduplicating one real repository took
/// `bus_factor_50` from 3 to 1. Applied here, before analysis, so every
/// governance command gets the same treatment from one implementation.
///
/// Reports what it merged: name-or-email unification is aggressive enough that
/// folding two people together silently would be worse than not doing it.
/// See #88.
fn apply_identity_resolution(
    records: &mut [serde_json::Value],
    author_field: &str,
    email_field: Option<&str>,
    cli: &Cli,
) {
    let email_field = resolve_field(email_field, &EMAIL_CANDIDATES, records, cli);
    let observations: Vec<(String, String)> = records
        .iter()
        .map(|r| {
            let name = extract_json_path(r, author_field)
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default();
            let email = extract_json_path(r, &email_field)
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default();
            (name, email)
        })
        .collect();

    let resolution =
        vajra_stats::resolve_identities(observations.iter().map(|(n, e)| (n.as_str(), e.as_str())));

    if let Some(key) = author_field.strip_prefix("$.") {
        for record in records.iter_mut() {
            let Some(name) = record.get(key).and_then(|v| v.as_str()).map(str::to_owned) else {
                continue;
            };
            let canonical = resolution.canonical(&name).to_owned();
            if canonical != name {
                if let Some(map) = record.as_object_mut() {
                    map.insert(key.to_owned(), serde_json::Value::String(canonical));
                }
            }
        }
    }

    report_identity_merges(&resolution, cli);
}

/// Say what identity resolution merged.
///
/// Name-or-email unification is aggressive — two distinct people sharing a name
/// merge — so every fold is reported rather than applied silently.
fn report_identity_merges(resolution: &vajra_stats::IdentityResolution, cli: &Cli) {
    if cli.quiet {
        return;
    }
    if resolution.merged.is_empty() {
        eprintln!(
            "vajra: identity resolution merged nothing ({} contributors)",
            resolution.identity_count
        );
        return;
    }
    eprintln!(
        "vajra: identity resolution merged {} name(s) into {} contributor(s), from {}:",
        resolution.names_merged(),
        resolution.identity_count,
        resolution.observed_names
    );
    for identity in &resolution.merged {
        eprintln!(
            "vajra:   {} <- {} ({} commits, {} address(es))",
            identity.canonical,
            identity.names.join(", "),
            identity.occurrences,
            identity.emails.len()
        );
    }
}

/// Flag hint appended to a governance field-resolution failure, so the message
/// says what to pass rather than only what was missing.
fn field_hint(err: &vajra_stats::GovernanceError) -> String {
    let flag = match err {
        vajra_stats::GovernanceError::AuthorFieldMissing { .. } => fields::AUTHOR.flag,
        vajra_stats::GovernanceError::TimeFieldMissing { .. } => "--time-field",
        vajra_stats::GovernanceError::EmptyInput => return String::new(),
    };
    format!("\n  hint: pass {flag} '$.<field>' with one of the fields listed above")
}

/// Resolve a field selector against the records, reporting the choice when it
/// falls back to a non-primary candidate.
///
/// Centralised so `governance`, `score` and `compare` cannot drift apart, and
/// so the reader vocabularies stay in one place — the git reader's `author_name`
/// / `subject` used to make `governance` fail on its own output.
fn resolve_field(
    explicit: Option<&str>,
    candidates: &fields::Candidates,
    records: &[serde_json::Value],
    cli: &Cli,
) -> String {
    resolve_field_labelled(explicit, candidates, records, None, cli)
}

/// As [`resolve_field`], but attributes the note to a named dataset.
///
/// `compare` resolves once per input, so without the label two datasets that
/// both fall back would emit identical notes and neither could be traced to
/// the file it came from.
fn resolve_field_labelled(
    explicit: Option<&str>,
    candidates: &fields::Candidates,
    records: &[serde_json::Value],
    label: Option<&str>,
    cli: &Cli,
) -> String {
    let resolved = fields::resolve(explicit, candidates, records);
    if let Some(note) = &resolved.note {
        if !cli.quiet {
            match label {
                Some(label) => eprintln!("vajra: [{label}] {note}"),
                None => eprintln!("vajra: {note}"),
            }
        }
    }
    resolved.selector
}

/// Say that `--streaming` does not yet bound memory.
///
/// The flag selects the sketch-based accumulators, but reaching them still
/// parses the whole document and then materialises a `Vec<JsonEvent>` beside
/// it: measured at 402 MB against the DOM path's 233 MB on a 15 MB input. A
/// user passing it to survive a large file gets the opposite of what the name
/// promises, so it says so rather than accepting the flag quietly. See #102.
fn warn_streaming_not_bounded(cli: &Cli) {
    if cli.quiet || !cli.streaming {
        return;
    }
    eprintln!(
        "vajra: --streaming selects the sketch-based accumulators but does not yet bound \
         memory — it currently uses more than the default path (see issue #102)"
    );
}

fn warn_unimplemented_format(cli: &Cli) {
    if cli.quiet {
        return;
    }
    let name = command_name(&cli.command);
    let (requested, implemented) = match cli.format {
        Format::Markdown => ("markdown", RENDERS_MARKDOWN),
        Format::CompactAi => ("compact-ai", RENDERS_COMPACT_AI),
        Format::Text | Format::Json => return,
    };
    if implemented.contains(&name) {
        return;
    }
    eprintln!(
        "vajra: `{name}` has no {requested} renderer; output is the text format. \
         Use --format json for a machine-readable form."
    );
}

/// The subcommand's name, for diagnostics.
fn command_name(command: &Command) -> &'static str {
    match command {
        Command::Inspect { .. } => "inspect",
        Command::Stats { .. } => "stats",
        Command::Anomalies { .. } => "anomalies",
        Command::Fingerprint { .. } => "fingerprint",
        Command::Essence { .. } => "essence",
        Command::Drift { .. } => "drift",
        Command::Cluster { .. } => "cluster",
        Command::Separation { .. } => "separation",
        Command::Invariants { .. } => "invariants",
        Command::Query { .. } => "query",
        Command::Batch { .. } => "batch",
        Command::Cascade { .. } => "cascade",
        Command::Score { .. } => "score",
        Command::Profiles => "profiles",
        Command::Governance { .. } => "governance",
        Command::IngestGithub { .. } => "ingest-github",
        Command::CoreTeam { .. } => "core-team",
        Command::Report { .. } => "report",
        Command::Compare { .. } => "compare",
        Command::Audit { .. } => "audit",
    }
}

fn main() {
    let cli = Cli::parse();

    warn_unimplemented_format(&cli);
    warn_streaming_not_bounded(&cli);

    let result = match &cli.command {
        Command::Inspect { input } => cmd_inspect(input, &cli),
        Command::Stats {
            input,
            window,
            time_field,
        } => cmd_stats(input, *window, time_field.as_deref(), &cli),
        Command::Anomalies { input } => cmd_anomalies(input, &cli),
        Command::Fingerprint {
            input,
            min_nodes,
            corpus,
            corpus_group_depth,
        } => {
            if *corpus {
                cmd_fingerprint_corpus(input, *min_nodes, *corpus_group_depth, &cli)
            } else {
                cmd_fingerprint(input, *min_nodes, &cli)
            }
        }
        Command::Essence { input } => cmd_essence(input, &cli),
        Command::Drift {
            baseline,
            candidate,
            group_by,
            tree,
        } => {
            if *tree {
                match candidate {
                    Some(c) => cmd_tree_diff(baseline, c, &cli),
                    None => {
                        eprintln!("vajra: drift --tree requires two directories");
                        std::process::exit(1);
                    }
                }
            } else if let Some(field) = group_by {
                cmd_population_drift(baseline, field, &cli)
            } else {
                match candidate {
                    Some(c) => cmd_drift(baseline, c, &cli),
                    None => {
                        eprintln!("vajra: drift requires a candidate file, or use --group-by for population-level drift");
                        std::process::exit(1);
                    }
                }
            }
        }
        Command::Cluster {
            inputs,
            similarity_threshold,
        } => cmd_cluster(inputs, *similarity_threshold, &cli),
        Command::Separation {
            input,
            label_field,
            base_rate,
            positive_class,
            top_k,
        } => cmd_separation(
            input,
            label_field,
            *base_rate,
            positive_class.as_deref(),
            *top_k,
            &cli,
        ),
        Command::Invariants { input, top_k, bin } => cmd_invariants(input, *top_k, bin, &cli),
        Command::Query { input, expression } => cmd_query(input, expression, &cli),
        Command::Batch { directory } => cmd_batch(directory, &cli),
        Command::Cascade {
            input,
            entity_field,
            time_field,
            event_field,
            response_values,
        } => cmd_cascade(
            input,
            entity_field,
            time_field,
            event_field,
            response_values,
            &cli,
        ),
        Command::Score {
            input,
            author_field,
            time_field,
            message_field,
            comments_field,
            resolve_identities,
            email_field,
        } => cmd_score(
            input,
            author_field.as_deref(),
            time_field,
            message_field.as_deref(),
            comments_field.as_deref(),
            *resolve_identities,
            email_field.as_deref(),
            &cli,
        ),
        Command::Profiles => cmd_profiles(&cli),
        Command::Governance {
            input,
            author_field,
            time_field,
            resolve_identities,
            email_field,
        } => cmd_governance(
            input,
            author_field.as_deref(),
            time_field,
            *resolve_identities,
            email_field.as_deref(),
            &cli,
        ),
        Command::IngestGithub {
            repo,
            output,
            pr_limit,
            issue_limit,
            commit_limit,
        } => cmd_ingest_github(repo, output, *pr_limit, *issue_limit, *commit_limit, &cli),
        Command::CoreTeam {
            input,
            resolve_identities,
        } => cmd_core_team(input, *resolve_identities, &cli),
        Command::Report {
            input,
            title,
            output,
            repo_name,
        } => cmd_report(input, title, output, repo_name.as_deref(), &cli),
        Command::Compare {
            inputs,
            labels,
            author_field,
            time_field,
            message_field,
        } => cmd_compare(
            inputs,
            labels.as_deref(),
            author_field.as_deref(),
            time_field,
            message_field.as_deref(),
            &cli,
        ),
        Command::Audit {
            repo,
            output,
            commit_limit,
            pr_limit,
            issue_limit,
        } => cmd_audit(
            repo,
            output.as_deref(),
            *commit_limit,
            *pr_limit,
            *issue_limit,
            &cli,
        ),
    };

    if let Err(e) = result {
        eprintln!("vajra: {e:#}");
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// Input helpers
// ---------------------------------------------------------------------------

fn to_input_format(arg: Option<InputFormatArg>) -> Option<InputFormat> {
    arg.and_then(|a| match a {
        InputFormatArg::Json => Some(InputFormat::Json),
        InputFormatArg::Ndjson => Some(InputFormat::Ndjson),
        InputFormatArg::Yaml => Some(InputFormat::Yaml),
        InputFormatArg::Csv => Some(InputFormat::Csv),
        InputFormatArg::Tsv => Some(InputFormat::Tsv),
        InputFormatArg::Markdown => Some(InputFormat::Markdown),
        InputFormatArg::Pdf => Some(InputFormat::Pdf),
        InputFormatArg::Cpuprofile => Some(InputFormat::CpuProfile),
        InputFormatArg::Strace => Some(InputFormat::Strace),
        InputFormatArg::Source => None, // handled separately in load_document
        InputFormatArg::Git => None,    // handled separately in load_document
    })
}

/// Check if the input should be parsed as git log.
fn is_git_input(input: &str, cli: &Cli) -> bool {
    if matches!(cli.input_format, Some(InputFormatArg::Git)) {
        return true;
    }
    // Auto-detect: if no format specified, check if input is a directory
    // containing a .git subdirectory.
    if cli.input_format.is_none() {
        let path = std::path::Path::new(input);
        return vajra_core::is_git_repo(path);
    }
    false
}

/// Load a git log from a repository directory via vajra-core.
fn load_git_document(input: &str, cli: &Cli) -> Result<Document> {
    let config = vajra_core::GitLogConfig {
        limit: cli.git_limit,
        branch: cli.git_branch.clone(),
    };
    let path = std::path::Path::new(input);
    vajra_core::load_git_log(path, &config).map_err(|e| anyhow::anyhow!("{e}"))
}

/// Check if the input should be parsed as source code.
#[cfg(feature = "source")]
fn is_source_input(input: &str, cli: &Cli) -> bool {
    if matches!(cli.input_format, Some(InputFormatArg::Source)) {
        return true;
    }
    // Auto-detect: if no format specified, check file extension
    if cli.input_format.is_none() {
        let path = std::path::Path::new(input);
        return vajra_source::is_source_file(path);
    }
    false
}

/// Parse a source language name string into a SourceLanguage.
#[cfg(feature = "source")]
fn parse_lang_flag(lang: &str) -> Result<vajra_source::SourceLanguage> {
    match lang.to_lowercase().as_str() {
        #[cfg(feature = "source")]
        "rust" | "rs" => Ok(vajra_source::SourceLanguage::Rust),
        "python" | "py" => Ok(vajra_source::SourceLanguage::Python),
        "javascript" | "js" => Ok(vajra_source::SourceLanguage::JavaScript),
        "go" => Ok(vajra_source::SourceLanguage::Go),
        other => anyhow::bail!(
            "unsupported language: '{other}'. Available: rust, python, javascript, go"
        ),
    }
}

/// Load a source code file via vajra-source.
#[cfg(feature = "source")]
fn load_source_document(input: &str, cli: &Cli) -> Result<Document> {
    let mut config = vajra_source::SourceConfig::default();
    if let Some(ref lang_str) = cli.lang {
        config.language = Some(parse_lang_flag(lang_str)?);
    }
    config.semantic_paths = cli.semantic_paths;
    let path = std::path::Path::new(input);
    vajra_source::parse_source_file(path, &config).map_err(|e| anyhow::anyhow!("{e}"))
}

/// Load a single document, aggregating multi-document formats into one.
///
/// For NDJSON and multi-document YAML, all records are wrapped into a
/// single JSON array so that downstream analyzers (stats, anomalies,
/// invariants, essence) compute cross-record statistics. Without this,
/// per-line analysis yields entropy=0 and cardinality=1 for every field.
fn load_document(input: &str, cli: &Cli) -> Result<Document> {
    // Check for git input first
    if is_git_input(input, cli) {
        return load_git_document(input, cli);
    }

    // Check for source code input
    #[cfg(feature = "source")]
    if is_source_input(input, cli) {
        return load_source_document(input, cli);
    }

    let fmt = to_input_format(cli.input_format);
    load_documents_aggregated(input, fmt).map_err(|e| anyhow::anyhow!("{e}"))
}

/// `load_document` for callers that already hold a `Path` (directory walks).
fn load_document_path(path: &Path, cli: &Cli) -> Result<Document> {
    // In a directory walk `--lang` is a *fallback*, not an override. A corpus
    // crosses languages, and forcing one grammar onto every file mis-parses the
    // rest: `--corpus --lang javascript` over a mixed tree parsed .py files as
    // JavaScript and dropped them. A recognised extension wins. See #90.
    #[cfg(feature = "source")]
    if cli.lang.is_some() && vajra_source::detect_language(path).is_some() {
        let config = vajra_source::SourceConfig {
            language: None,
            semantic_paths: cli.semantic_paths,
            ..vajra_source::SourceConfig::default()
        };
        return vajra_source::parse_source_file(path, &config).map_err(|e| anyhow::anyhow!("{e}"));
    }
    load_document(&path.display().to_string(), cli)
}

fn hex(bytes: &[u8; 32]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::with_capacity(64), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Load custom profiles from the --config flag, if provided.
fn load_custom_profiles(cli: &Cli) -> Result<Vec<CustomProfile>> {
    match &cli.config {
        Some(path) => {
            let p = Path::new(path);
            vajra_essence::load_profiles_from_file(p)
                .map_err(|e| anyhow::anyhow!("failed to load config: {e}"))
        }
        None => Ok(Vec::new()),
    }
}

/// Apply redaction to rendered output if --redact flag is set.
fn maybe_redact(output: &str, cli: &Cli) -> String {
    if cli.redact {
        let redactor = vajra_core::redact::Redactor::with_builtins();
        if let Some(redacted) = redactor.redact_value(output) {
            redacted
        } else {
            output.to_string()
        }
    } else {
        output.to_string()
    }
}

// ---------------------------------------------------------------------------
// score command
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn cmd_score(
    input: &str,
    author_field: Option<&str>,
    time_field: &str,
    message_field: Option<&str>,
    comments_field: Option<&str>,
    resolve_identities: bool,
    email_field: Option<&str>,
    cli: &Cli,
) -> Result<()> {
    let doc = load_document(input, cli)?;
    let records = match doc.value().as_array() {
        Some(arr) => arr.clone(),
        None => {
            anyhow::bail!(
                "score command expects a JSON array of records (e.g. commit or issue data)"
            );
        }
    };

    if records.is_empty() {
        anyhow::bail!("score command received an empty array — no records to score");
    }

    let author_field = resolve_field(author_field, &fields::AUTHOR, &records, cli);
    let message_field = resolve_field(message_field, &fields::MESSAGE, &records, cli);
    let mut records = records;
    if resolve_identities {
        apply_identity_resolution(&mut records, &author_field, email_field, cli);
    }
    let metrics = extract_health_metrics(
        &records,
        &author_field,
        time_field,
        &message_field,
        comments_field,
    );
    let weights = HealthWeights::default();
    let score = compute_health_score(&metrics, &weights);

    match score {
        Some(ref s) => match cli.format {
            Format::Json => {
                let j = score_to_json(s);
                let out = serde_json::to_string_pretty(&j).context("JSON serialization failed")?;
                let out = maybe_redact(&out, cli);
                println!("{out}");
            }
            Format::Text | Format::Markdown => {
                let report = score_report(s);
                let t = match cli.format {
                    Format::Markdown => report.to_markdown(),
                    _ => report.to_text(),
                };
                print!("{}", maybe_redact(&t, cli));
            }
            Format::CompactAi => {
                let j = score_to_json(s);
                let out = serde_json::to_string(&j).context("JSON serialization failed")?;
                let out = maybe_redact(&out, cli);
                println!("{out}");
            }
        },
        None => {
            anyhow::bail!(
                "could not compute health score: no scorable dimensions found in the data"
            );
        }
    }

    Ok(())
}

/// Extract health metrics from a JSON array of records.
fn extract_health_metrics(
    records: &[serde_json::Value],
    author_field: &str,
    time_field: &str,
    message_field: &str,
    comments_field: Option<&str>,
) -> HealthMetrics {
    let mut metrics = HealthMetrics::default();

    // --- Bus factor: top contributor share, plus entropy for diversity ---
    let mut author_map: BTreeMap<String, u64> = BTreeMap::new();
    let mut total_authors = 0_u64;
    for record in records {
        if let Some(author_val) = extract_json_path(record, author_field) {
            if let Some(name) = author_val.as_str() {
                *author_map.entry(name.to_owned()).or_insert(0) += 1;
                total_authors += 1;
            }
        }
    }
    if total_authors > 0 {
        let counts: Vec<u64> = author_map.values().copied().collect();
        let entropy = shannon_entropy_from_counts(&counts);
        metrics.commit_entropy = Some(entropy);
        // Concentration at the top, which entropy over a long one-commit tail
        // cannot see. This is what the bus_factor dimension grades.
        //
        // The u64 -> f64 casts lose precision only past 2^53 commits, which no
        // repository reaches; the ratio is exact for any real input.
        #[allow(clippy::cast_precision_loss)]
        {
            metrics.top1_share = counts
                .iter()
                .copied()
                .max()
                .map(|top| top as f64 / total_authors as f64);
        }
    }

    // --- Code stability: fix ratio ---
    let fix_patterns = ["fix", "bug", "hotfix", "patch", "revert"];
    let mut total_commits = 0_u64;
    let mut fix_commits = 0_u64;
    for record in records {
        if let Some(msg_val) = extract_json_path(record, message_field) {
            if let Some(msg) = msg_val.as_str() {
                total_commits += 1;
                let lower = msg.to_lowercase();
                if fix_patterns.iter().any(|p| lower.contains(p)) {
                    fix_commits += 1;
                }
            }
        }
    }
    if total_commits > 0 {
        #[allow(clippy::cast_precision_loss)] // u64 counts are well within f64 range
        let ratio = fix_commits as f64 / total_commits as f64;
        metrics.fix_ratio = Some(ratio);
    }

    // --- Contributor retention: one-commit rate ---
    if total_authors > 0 {
        let one_commit_authors = author_map.values().filter(|&&c| c == 1).count();
        #[allow(clippy::cast_precision_loss)] // counts are small
        let rate = one_commit_authors as f64 / author_map.len() as f64;
        metrics.one_commit_rate = Some(rate);
    }

    // --- Velocity trend: linear regression on monthly commit counts ---
    let mut monthly_counts: BTreeMap<String, u64> = BTreeMap::new();
    for record in records {
        if let Some(date_val) = extract_json_path(record, time_field) {
            if let Some(date_str) = date_val.as_str() {
                // Extract YYYY-MM prefix for monthly bucketing
                if date_str.len() >= 7 {
                    let month_key = &date_str[..7];
                    *monthly_counts.entry(month_key.to_owned()).or_insert(0) += 1;
                }
            }
        }
    }
    if monthly_counts.len() >= 2 {
        #[allow(clippy::cast_precision_loss)] // monthly counts are small
        let values: Vec<f64> = monthly_counts.values().map(|&c| c as f64).collect();
        if let Some(trend) = linear_regression(&values) {
            metrics.velocity_slope = Some(trend.slope);
        }
    }

    // --- Issue response: zero-comment rate ---
    if let Some(cf) = comments_field {
        let mut total_issues = 0_u64;
        let mut zero_comment_issues = 0_u64;
        for record in records {
            if let Some(comments_val) = extract_json_path(record, cf) {
                total_issues += 1;
                let count = comments_val.as_u64().or_else(|| {
                    comments_val.as_f64().map(|f| {
                        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                        let u = f as u64;
                        u
                    })
                });
                if count == Some(0) {
                    zero_comment_issues += 1;
                }
            }
        }
        if total_issues > 0 {
            #[allow(clippy::cast_precision_loss)] // counts are small
            let rate = zero_comment_issues as f64 / total_issues as f64;
            metrics.zero_comment_rate = Some(rate);
        }
    }

    metrics
}

fn score_to_json(score: &HealthScore) -> serde_json::Value {
    let mut dims = serde_json::Map::new();
    for (name, ds) in &score.dimensions {
        dims.insert(
            name.clone(),
            serde_json::json!({
                "grade": ds.grade.as_str(),
                "value": ds.value,
                "metric": ds.metric_name,
                "description": ds.description,
            }),
        );
    }
    serde_json::json!({
        "overall": score.overall.as_str(),
        "overall_numeric": score.overall_numeric,
        "dimensions": dims,
    })
}

/// Build the health-score report, independent of output format.
fn score_report(score: &HealthScore) -> render::Report {
    let mut report = render::Report::new();

    report.heading("Health Score");
    report.fields(vec![(
        "Overall".to_owned(),
        format!("{} ({:.2})", score.overall, score.overall_numeric),
    )]);

    report.heading("Dimensions");
    let mut t = render::Table::new(
        &["DIMENSION", "GRADE", "VALUE", "METRIC"],
        "no scorable dimensions",
    );
    for (name, ds) in &score.dimensions {
        t.push(vec![
            name.clone(),
            ds.grade.to_string(),
            format!("{:.4}", ds.value),
            ds.metric_name.clone(),
        ]);
    }
    report.table(t);

    report.heading("Descriptions");
    report.nested(
        score
            .dimensions
            .iter()
            .map(|(name, ds)| (name.clone(), vec![ds.description.clone()]))
            .collect(),
    );

    report
}

// ---------------------------------------------------------------------------
// profiles command
// ---------------------------------------------------------------------------

fn cmd_profiles(cli: &Cli) -> Result<()> {
    let builtin_profiles: Vec<(&str, &str)> = vec![
        (
            "staff",
            "Plain vocabulary, narrative rendering; emphasizes anomalies and structural coverage",
        ),
        (
            "engineer",
            "Technical vocabulary, list-based rendering; balanced scoring",
        ),
        (
            "auditor",
            "Formal vocabulary, completeness-focused; emphasizes instability and concern relevance",
        ),
        (
            "ai",
            "Compact terse rendering optimized for machine consumption",
        ),
        (
            "fraud",
            "Investigative framing; emphasizes outliers, rarity, and suspicious patterns",
        ),
        (
            "health",
            "Project health assessment; emphasizes contributor diversity, governance, and sustainability",
        ),
    ];

    let custom_profiles = load_custom_profiles(cli)?;

    match cli.format {
        Format::Text | Format::Markdown | Format::CompactAi => {
            let mut out = render::Report::new();
            out.heading("Built-in Profiles");
            let mut builtin = render::Table::new(&["PROFILE", "DESCRIPTION"], "(none)");
            for (name, desc) in &builtin_profiles {
                builtin.push(vec![(*name).to_owned(), (*desc).to_owned()]);
            }
            out.table(builtin);

            if !custom_profiles.is_empty() {
                out.heading("Custom Profiles");
                let mut custom = render::Table::new(&["PROFILE", "DESCRIPTION"], "(none)");
                for p in &custom_profiles {
                    custom.push(vec![p.name().to_owned(), p.description().to_owned()]);
                }
                out.table(custom);
            }

            let text = match cli.format {
                Format::Markdown => out.to_markdown(),
                _ => out.to_text(),
            };
            print!("{}", maybe_redact(&text, cli));
        }
        Format::Json => {
            let mut profiles_json: Vec<serde_json::Value> = builtin_profiles
                .iter()
                .map(|(name, desc)| {
                    serde_json::json!({
                        "name": name,
                        "description": desc,
                        "source": "built-in"
                    })
                })
                .collect();

            for p in &custom_profiles {
                profiles_json.push(serde_json::json!({
                    "name": p.name(),
                    "description": p.description(),
                    "source": "custom"
                }));
            }

            let json = serde_json::to_string_pretty(&profiles_json)
                .context("JSON serialization failed")?;
            println!("{json}");
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// inspect
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct InspectOutput {
    metadata: MetadataView,
    paths: Vec<PathView>,
    fingerprints: FingerprintsView,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    domain_hints: Vec<DomainHintView>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    structural_findings: Vec<StructuralFindingView>,
}

#[derive(Serialize)]
struct MetadataView {
    total_nodes: u64,
    max_depth: u32,
    distinct_paths: u32,
    raw_size_bytes: u64,
}

#[derive(Serialize)]
struct PathView {
    path: String,
    dominant_type: String,
    count: u64,
    type_instability: f64,
    null_rate: f64,
}

#[derive(Serialize)]
struct FingerprintsView {
    path_set: String,
    typed_path: String,
    shape: String,
}

#[derive(Serialize)]
struct DomainHintView {
    path: String,
    value: String,
    recognized_type: String,
}

#[derive(Serialize)]
struct StructuralFindingView {
    signal: String,
    path: String,
    detail: String,
    severity: String,
}

/// Collect all type recognizers from enabled domain plugins.
fn collect_recognizers() -> Vec<Box<dyn vajra_types::traits::TypeRecognizer>> {
    let mut recognizers: Vec<Box<dyn vajra_types::traits::TypeRecognizer>> = Vec::new();

    #[cfg(feature = "medical")]
    {
        let plugin = vajra_domain_med::MedicalPlugin;
        recognizers.extend(vajra_types::traits::VajraPlugin::type_recognizers(&plugin));
    }

    #[cfg(feature = "security")]
    {
        let plugin = vajra_domain_sec::SecurityPlugin;
        recognizers.extend(vajra_types::traits::VajraPlugin::type_recognizers(&plugin));
    }

    #[cfg(feature = "devops")]
    {
        let plugin = vajra_domain_devops::DevOpsPlugin;
        recognizers.extend(vajra_types::traits::VajraPlugin::type_recognizers(&plugin));
    }

    #[cfg(feature = "source")]
    {
        let plugin = vajra_domain_source::SourcePlugin;
        recognizers.extend(vajra_types::traits::VajraPlugin::type_recognizers(&plugin));
    }

    #[cfg(feature = "github")]
    {
        let plugin = vajra_domain_github::GitHubPlugin;
        recognizers.extend(vajra_types::traits::VajraPlugin::type_recognizers(&plugin));
    }

    #[cfg(feature = "package")]
    {
        let plugin = vajra_domain_package::PackagePlugin;
        recognizers.extend(vajra_types::traits::VajraPlugin::type_recognizers(&plugin));
    }

    // Encoding plugin registered LAST — other plugins claim specific types
    // (JWT, SHA hashes) first; encoding detects the general encoding pattern.
    #[cfg(feature = "encoding")]
    {
        let plugin = vajra_domain_encoding::EncodingPlugin;
        recognizers.extend(vajra_types::traits::VajraPlugin::type_recognizers(&plugin));
    }

    recognizers
}

/// Collect relationship hints from enabled domain plugins.
fn collect_hints() -> Vec<vajra_types::traits::RelationshipHint> {
    let mut hints: Vec<vajra_types::traits::RelationshipHint> = Vec::new();
    macro_rules! add {
        ($plugin:expr) => {
            hints.extend(vajra_types::traits::VajraPlugin::relationship_hints(
                &$plugin,
            ));
        };
    }
    #[cfg(feature = "medical")]
    add!(vajra_domain_med::MedicalPlugin);
    #[cfg(feature = "security")]
    add!(vajra_domain_sec::SecurityPlugin);
    #[cfg(feature = "devops")]
    add!(vajra_domain_devops::DevOpsPlugin);
    #[cfg(feature = "source")]
    add!(vajra_domain_source::SourcePlugin);
    #[cfg(feature = "github")]
    add!(vajra_domain_github::GitHubPlugin);
    #[cfg(feature = "encoding")]
    add!(vajra_domain_encoding::EncodingPlugin);
    hints
}

/// Collect structural detectors from enabled domain plugins.
fn collect_detectors() -> Vec<Box<dyn vajra_types::traits::StructuralDetector>> {
    let mut detectors: Vec<Box<dyn vajra_types::traits::StructuralDetector>> = Vec::new();

    #[cfg(feature = "package")]
    {
        let plugin = vajra_domain_package::PackagePlugin;
        detectors.extend(vajra_types::traits::VajraPlugin::structural_detectors(
            &plugin,
        ));
    }

    detectors
}

/// Run structural detectors over a document.
///
/// Unlike type recognizers, which classify sampled values, these read the
/// document's shape — so they see facts like "an install-time hook is declared"
/// that are properties of a key existing rather than of any value.
fn detect_structural_findings(doc: &Document) -> Vec<StructuralFindingView> {
    let detectors = collect_detectors();
    if detectors.is_empty() {
        return Vec::new();
    }

    // Records may be wrapped in an array (NDJSON, a batch): check the whole
    // document and each top-level element, so a manifest is found either way.
    let mut candidates: Vec<&serde_json::Value> = vec![doc.value()];
    if let Some(items) = doc.value().as_array() {
        candidates.extend(items.iter());
    }

    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for candidate in candidates {
        for detector in &detectors {
            if !detector.applies(candidate) {
                continue;
            }
            for f in detector.inspect(candidate) {
                let key = (f.signal.clone(), f.path.clone(), f.detail.clone());
                if seen.insert(key) {
                    out.push(StructuralFindingView {
                        signal: f.signal,
                        path: f.path,
                        detail: f.detail,
                        severity: f.severity.as_str().to_owned(),
                    });
                }
            }
        }
    }
    // Concern first, then by signal and path, so ordering is fully specified.
    out.sort_by(|a, b| {
        severity_rank(&b.severity)
            .cmp(&severity_rank(&a.severity))
            .then_with(|| a.signal.cmp(&b.signal))
            .then_with(|| a.path.cmp(&b.path))
    });
    out
}

fn severity_rank(s: &str) -> u8 {
    match s {
        "concern" => 2,
        "notable" => 1,
        _ => 0,
    }
}

/// Try to match document values against domain-specific type recognizers
/// using statistics top_values as the source of sample data.
fn detect_domain_hints(stats: &StatsResult) -> Vec<DomainHintView> {
    let mut hints = Vec::new();

    let recognizers = collect_recognizers();

    if recognizers.is_empty() {
        return hints;
    }

    for (wp, path_stats) in &stats.paths {
        for (value, _count) in &path_stats.top_values {
            for recognizer in &recognizers {
                if recognizer.matches(value) {
                    hints.push(DomainHintView {
                        path: wp.to_string(),
                        value: value.clone(),
                        recognized_type: recognizer.type_name().to_string(),
                    });
                    break; // One match per value is enough
                }
            }
        }
    }

    hints
}

fn cmd_inspect(input: &str, cli: &Cli) -> Result<()> {
    let doc = load_document(input, cli)?;

    let meta = doc.metadata();
    let metadata_view = MetadataView {
        total_nodes: meta.total_nodes,
        max_depth: meta.max_depth,
        distinct_paths: meta.distinct_paths,
        raw_size_bytes: meta.raw_size_bytes,
    };

    let all_paths = doc.trie().all_paths();
    let mut path_views: Vec<PathView> = Vec::with_capacity(all_paths.len());
    for wp in &all_paths {
        if let Some(node) = doc.trie().get(wp) {
            let m = &node.metadata;
            let dom = m
                .dominant_type()
                .map_or_else(|| "unknown".to_owned(), |t| t.to_string());
            path_views.push(PathView {
                path: wp.to_string(),
                dominant_type: dom,
                count: m.count,
                type_instability: m.type_instability(),
                null_rate: m.null_rate(),
            });
        }
    }

    let fp = FingerprintAnalyzer
        .analyze(&doc)
        .context("fingerprint analysis failed")?;
    let fp_view = FingerprintsView {
        path_set: hex(&fp.path_set),
        typed_path: hex(&fp.typed_path),
        shape: hex(&fp.shape),
    };

    // Run stats for domain hint detection
    let stats = StatsAnalyzer
        .analyze(&doc)
        .context("stats analysis for domain hints failed")?;
    let domain_hints = detect_domain_hints(&stats);
    let structural_findings = detect_structural_findings(&doc);

    let output = InspectOutput {
        metadata: metadata_view,
        paths: path_views,
        fingerprints: fp_view,
        domain_hints,
        structural_findings,
    };

    match cli.format {
        Format::Json => {
            let json =
                serde_json::to_string_pretty(&output).context("JSON serialization failed")?;
            let json = maybe_redact(&json, cli);
            println!("{json}");
        }
        Format::Text | Format::Markdown | Format::CompactAi => {
            let mut report = render::Report::new();

            report.heading("Document Metadata");
            report.fields(vec![
                (
                    "Total nodes".to_owned(),
                    output.metadata.total_nodes.to_string(),
                ),
                (
                    "Max depth".to_owned(),
                    output.metadata.max_depth.to_string(),
                ),
                (
                    "Distinct paths".to_owned(),
                    output.metadata.distinct_paths.to_string(),
                ),
                (
                    "Raw size".to_owned(),
                    format!("{} bytes", output.metadata.raw_size_bytes),
                ),
            ]);

            report.heading("Wildcard Paths");
            let mut paths = render::Table::new(
                &["PATH", "TYPE", "COUNT", "INSTABILITY", "NULL_RATE"],
                "no paths",
            );
            for p in &output.paths {
                paths.push(vec![
                    p.path.clone(),
                    p.dominant_type.clone(),
                    p.count.to_string(),
                    format!("{:.4}", p.type_instability),
                    format!("{:.4}", p.null_rate),
                ]);
            }
            report.table(paths);

            report.heading("Fingerprints");
            report.fields(vec![
                ("Path set".to_owned(), output.fingerprints.path_set.clone()),
                (
                    "Typed path".to_owned(),
                    output.fingerprints.typed_path.clone(),
                ),
                ("Shape".to_owned(), output.fingerprints.shape.clone()),
            ]);

            if !output.domain_hints.is_empty() {
                report.heading("Domain Type Recognition");
                let mut t = render::Table::new(&["PATH", "VALUE", "RECOGNIZED AS"], "none");
                for hint in &output.domain_hints {
                    t.push(vec![
                        hint.path.clone(),
                        hint.value.clone(),
                        hint.recognized_type.clone(),
                    ]);
                }
                report.table(t);
            }

            if !output.structural_findings.is_empty() {
                report.heading("Structural Findings");
                let mut t = render::Table::new(&["SEVERITY", "PATH", "DETAIL"], "none");
                for f in &output.structural_findings {
                    t.push(vec![f.severity.clone(), f.path.clone(), f.detail.clone()]);
                }
                report.table(t);
                report.note(
                    "`concern` marks structure that carries known risk, such as code that\nruns at install time. It is not a verdict on the package.",
                );
            }

            let rendered = match cli.format {
                Format::Markdown => report.to_markdown(),
                _ => report.to_text(),
            };
            print!("{}", maybe_redact(&rendered, cli));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// stats
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct StatsOutput {
    paths: Vec<StatsPathView>,
}

#[derive(Serialize)]
struct StatsPathView {
    path: String,
    entropy: f64,
    normalized_entropy: f64,
    cardinality: u64,
    total_count: u64,
    max_rarity: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    numeric: Option<NumericView>,
    top_values: Vec<TopValueView>,
}

#[derive(Serialize)]
struct NumericView {
    min: f64,
    max: f64,
    mean: f64,
    median: f64,
    mad: f64,
    p05: f64,
    p25: f64,
    p75: f64,
    p95: f64,
}

#[derive(Serialize)]
struct TopValueView {
    value: String,
    count: u64,
}

fn build_stats_output(result: &StatsResult) -> StatsOutput {
    let mut paths = Vec::new();
    for (wp, stats) in &result.paths {
        let numeric = stats.numeric_stats.as_ref().map(|ns| NumericView {
            min: ns.min,
            max: ns.max,
            mean: ns.mean,
            median: ns.median,
            mad: ns.mad,
            p05: ns.p05,
            p25: ns.p25,
            p75: ns.p75,
            p95: ns.p95,
        });
        let top_values: Vec<TopValueView> = stats
            .top_values
            .iter()
            .map(|(v, c)| TopValueView {
                value: v.clone(),
                count: *c,
            })
            .collect();
        paths.push(StatsPathView {
            path: wp.to_string(),
            entropy: stats.entropy,
            normalized_entropy: stats.normalized_entropy,
            cardinality: stats.cardinality,
            total_count: stats.total_count,
            max_rarity: stats.max_rarity,
            numeric,
            top_values,
        });
    }
    StatsOutput { paths }
}

fn cmd_stats(
    input: &str,
    window: Option<WindowArg>,
    time_field: Option<&str>,
    cli: &Cli,
) -> Result<()> {
    if let Some(win) = window {
        return cmd_stats_windowed(input, win, time_field, cli);
    }

    let result = if cli.streaming {
        let doc = load_document(input, cli)?;
        let events = vajra_core::emit_events(doc.value());
        let mut acc = StreamingStatsAccumulator::default();
        acc.process_events(&events);
        acc.finalize()
    } else {
        let doc = load_document(input, cli)?;
        StatsAnalyzer
            .analyze(&doc)
            .context("stats analysis failed")?
    };
    let output = build_stats_output(&result);

    match cli.format {
        Format::Json => {
            let json =
                serde_json::to_string_pretty(&output).context("JSON serialization failed")?;
            let json = maybe_redact(&json, cli);
            println!("{json}");
        }
        Format::Text | Format::Markdown | Format::CompactAi => {
            let mut report = render::Report::new();

            report.heading("Per-Path Statistics");
            let mut summary = render::Table::new(
                &["PATH", "ENTROPY", "NORM", "CARD", "COUNT", "MAX RARITY"],
                "no scalar or array paths in this document",
            );
            for sp in &output.paths {
                summary.push(vec![
                    sp.path.clone(),
                    format!("{:.4}", sp.entropy),
                    format!("{:.4}", sp.normalized_entropy),
                    sp.cardinality.to_string(),
                    sp.total_count.to_string(),
                    format!("{:.4}", sp.max_rarity),
                ]);
            }
            report.table(summary);

            // Numeric quantiles only exist for numeric paths, so they get their
            // own table rather than empty columns in the summary above.
            let numeric: Vec<&StatsPathView> = output
                .paths
                .iter()
                .filter(|sp| sp.numeric.is_some())
                .collect();
            if !numeric.is_empty() {
                report.heading("Numeric Distributions");
                let mut t = render::Table::new(
                    &[
                        "PATH", "MIN", "MAX", "MEAN", "MEDIAN", "MAD", "P05", "P25", "P75", "P95",
                    ],
                    "none",
                );
                for sp in numeric {
                    if let Some(ref ns) = sp.numeric {
                        t.push(vec![
                            sp.path.clone(),
                            format!("{:.4}", ns.min),
                            format!("{:.4}", ns.max),
                            format!("{:.4}", ns.mean),
                            format!("{:.4}", ns.median),
                            format!("{:.4}", ns.mad),
                            format!("{:.4}", ns.p05),
                            format!("{:.4}", ns.p25),
                            format!("{:.4}", ns.p75),
                            format!("{:.4}", ns.p95),
                        ]);
                    }
                }
                report.table(t);
                report.note(
                    "MAD is the median absolute deviation — a robust spread estimate with a\n50% breakdown point, unlike the standard deviation.",
                );
            }

            let with_values: Vec<(String, Vec<String>)> = output
                .paths
                .iter()
                .filter(|sp| !sp.top_values.is_empty())
                .map(|sp| {
                    (
                        sp.path.clone(),
                        sp.top_values
                            .iter()
                            .map(|tv| format!("{}x  {}", tv.count, tv.value))
                            .collect(),
                    )
                })
                .collect();
            if !with_values.is_empty() {
                report.heading("Top Values");
                report.nested(with_values);
            }

            let text = match cli.format {
                Format::Markdown => report.to_markdown(),
                _ => report.to_text(),
            };
            let text = maybe_redact(&text, cli);
            print!("{text}");
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// stats windowed
// ---------------------------------------------------------------------------

fn cmd_stats_windowed(
    input: &str,
    window: WindowArg,
    time_field: Option<&str>,
    cli: &Cli,
) -> Result<()> {
    use vajra_stats::temporal::{auto_detect_time_field, windowed_analysis, WindowGranularity};

    let granularity = match window {
        WindowArg::Month => WindowGranularity::Month,
        WindowArg::Week => WindowGranularity::Week,
        WindowArg::Day => WindowGranularity::Day,
    };

    // Use the unified load_document path so git repos are handled.
    let doc = load_document(input, cli)?;

    let mut records: Vec<serde_json::Value> = Vec::new();
    match doc.value() {
        serde_json::Value::Array(arr) => {
            records.extend(arr.iter().cloned());
        }
        obj @ serde_json::Value::Object(_) => {
            records.push(obj.clone());
        }
        _ => {}
    }

    if records.is_empty() {
        anyhow::bail!("no records found in input");
    }

    // When input is git, default time_field to $.date
    let resolved_time_field = match time_field {
        Some(tf) => tf.to_owned(),
        None => {
            if is_git_input(input, cli) {
                "$.date".to_owned()
            } else {
                auto_detect_time_field(&records).ok_or_else(|| {
                    anyhow::anyhow!("could not auto-detect time field; use --time-field to specify")
                })?
            }
        }
    };

    let result = windowed_analysis(&records, &resolved_time_field, granularity)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    match cli.format {
        Format::Json => {
            let json =
                serde_json::to_string_pretty(&result).context("JSON serialization failed")?;
            let json = maybe_redact(&json, cli);
            println!("{json}");
        }
        Format::Text | Format::Markdown | Format::CompactAi => {
            print_windowed_text(&result);
        }
    }

    Ok(())
}

fn print_windowed_text(result: &vajra_stats::temporal::WindowedAnalysisResult) {
    use std::fmt::Write;

    let mut text = String::new();
    let _ = writeln!(text, "=== Temporal Windowed Analysis ===");
    let _ = writeln!(text, "Windows: {}", result.windows.len());
    let _ = writeln!(text);

    for ws in &result.windows {
        let _ = writeln!(text, "--- {} ({} records) ---", ws.window, ws.record_count);
        for (path, stats) in &ws.field_stats {
            let _ = writeln!(
                text,
                "  {path}: entropy={:.4} cardinality={}",
                stats.entropy, stats.cardinality
            );
        }
    }

    if !result.trends.is_empty() {
        let _ = writeln!(text);
        let _ = writeln!(text, "=== Trends ===");
        for (metric, trend) in &result.trends {
            let _ = writeln!(
                text,
                "  {metric}: slope={:.4} direction={} R\u{b2}={:.4}",
                trend.slope, trend.direction, trend.r_squared
            );
        }
    }

    print!("{text}");
}

// ---------------------------------------------------------------------------
// anomalies
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct AnomalyOutput {
    type_instabilities: Vec<TypeInstabilityView>,
    numeric_outliers: Vec<NumericOutlierView>,
    rare_values: Vec<RareValueView>,
}

#[derive(Serialize)]
struct TypeInstabilityView {
    path: String,
    instability: f64,
    dominant_type: String,
    type_distribution: BTreeMap<String, u64>,
}

#[derive(Serialize)]
struct NumericOutlierView {
    path: String,
    value: f64,
    z_mad: f64,
    median: f64,
    mad: f64,
}

#[derive(Serialize)]
struct RareValueView {
    path: String,
    value: String,
    count: u64,
    rarity_bits: f64,
}

fn build_anomaly_output(report: &AnomalyReport) -> AnomalyOutput {
    AnomalyOutput {
        type_instabilities: report
            .type_instabilities
            .iter()
            .map(|ti| TypeInstabilityView {
                path: ti.path.clone(),
                instability: ti.instability,
                dominant_type: ti.dominant_type.clone(),
                type_distribution: ti.type_distribution.clone(),
            })
            .collect(),
        numeric_outliers: report
            .numeric_outliers
            .iter()
            .map(|no| NumericOutlierView {
                path: no.path.clone(),
                value: no.value,
                z_mad: no.z_mad,
                median: no.median,
                mad: no.mad,
            })
            .collect(),
        rare_values: report
            .rare_values
            .iter()
            .map(|rv| RareValueView {
                path: rv.path.clone(),
                value: rv.value.clone(),
                count: rv.count,
                rarity_bits: rv.rarity_bits,
            })
            .collect(),
    }
}

fn cmd_anomalies(input: &str, cli: &Cli) -> Result<()> {
    let doc = load_document(input, cli)?;
    let analyzer = AnomalyAnalyzer::default();
    let report = analyzer.analyze(&doc).context("anomaly analysis failed")?;
    let output = build_anomaly_output(&report);

    match cli.format {
        Format::Json => {
            let json =
                serde_json::to_string_pretty(&output).context("JSON serialization failed")?;
            let json = maybe_redact(&json, cli);
            println!("{json}");
        }
        Format::Text | Format::Markdown | Format::CompactAi => {
            let mut report = render::Report::new();

            report.heading("Anomaly Summary");
            report.fields(vec![
                (
                    "Type instabilities".to_owned(),
                    output.type_instabilities.len().to_string(),
                ),
                (
                    "Numeric outliers".to_owned(),
                    output.numeric_outliers.len().to_string(),
                ),
                (
                    "Rare values".to_owned(),
                    output.rare_values.len().to_string(),
                ),
            ]);

            report.heading("Type Instabilities");
            if output.type_instabilities.is_empty() {
                report.table(render::Table::new(&["PATH"], "none detected"));
            } else {
                report.nested(
                    output
                        .type_instabilities
                        .iter()
                        .map(|ti| {
                            let dist: Vec<String> = ti
                                .type_distribution
                                .iter()
                                .map(|(t, c)| format!("{t}={c}"))
                                .collect();
                            (
                                format!(
                                    "{}: instability={:.4}, dominant={}",
                                    ti.path, ti.instability, ti.dominant_type
                                ),
                                vec![format!("types: {}", dist.join(", "))],
                            )
                        })
                        .collect(),
                );
            }

            report.heading("Numeric Outliers");
            let mut outliers = render::Table::new(
                &["PATH", "VALUE", "Z_MAD", "MEDIAN", "MAD"],
                "none detected",
            );
            for no in &output.numeric_outliers {
                outliers.push(vec![
                    no.path.clone(),
                    no.value.to_string(),
                    format!("{:.4}", no.z_mad),
                    format!("{:.4}", no.median),
                    format!("{:.4}", no.mad),
                ]);
            }
            report.table(outliers);
            if !output.numeric_outliers.is_empty() {
                report.note(
                    "Z_MAD is deviation from the median in MAD units, so it is robust to the\noutliers it is detecting.",
                );
            }

            report.heading("Rare Values");
            let mut rare = render::Table::new(
                &["PATH", "VALUE", "COUNT", "RARITY (bits)"],
                "none detected",
            );
            for rv in &output.rare_values {
                rare.push(vec![
                    rv.path.clone(),
                    rv.value.clone(),
                    rv.count.to_string(),
                    format!("{:.4}", rv.rarity_bits),
                ]);
            }
            report.table(rare);

            let text = match cli.format {
                Format::Markdown => report.to_markdown(),
                _ => report.to_text(),
            };
            let text = maybe_redact(&text, cli);
            print!("{text}");
        }
    }

    Ok(())
}

/// Render a corpus shape index.
fn corpus_index_report(index: &corpus::CorpusIndex) -> render::Report {
    let mut report = render::Report::new();
    report.heading("Corpus Shape Index");
    report.fields(vec![
        ("Files scanned".to_owned(), index.files_scanned.to_string()),
        (
            "Documents indexed".to_owned(),
            index.documents_indexed.to_string(),
        ),
        ("Skipped (format)".to_owned(), index.skipped.to_string()),
        ("Suppressed (size)".to_owned(), index.suppressed.to_string()),
        (
            "Distinct shapes".to_owned(),
            index.distinct_shapes.to_string(),
        ),
        (
            "Reused shapes".to_owned(),
            index.shapes_in_multiple_documents.to_string(),
        ),
    ]);

    report.heading("Reuse Groups");
    if index.reuse_groups.is_empty() {
        report.note("no shape occurs in more than one document");
    } else {
        report.nested(
            index
                .reuse_groups
                .iter()
                .map(|g| {
                    (
                        format!(
                            "{} x{}  nodes={}",
                            &g.shape[..16.min(g.shape.len())],
                            g.count,
                            g.node_count
                        ),
                        g.members.clone(),
                    )
                })
                .collect(),
        );
    }

    report.heading("Clusters (linked transitively)");
    if index.clusters.is_empty() {
        report.note("none");
    } else {
        report.nested(
            index
                .clusters
                .iter()
                .map(|c| {
                    (
                        format!(
                            "{} group(s), {} shared shape(s), min nodes {}",
                            c.size, c.shared_shapes, c.min_node_count
                        ),
                        c.members.clone(),
                    )
                })
                .collect(),
        );
        report.note(
            "A cluster resting on one small shape is weak evidence — check min nodes, \
             and see --min-nodes.",
        );
    }

    if !index.errors.is_empty() {
        report.heading(format!("Errors ({})", index.errors.len()));
        let mut table = render::Table::new(&["FILE", "ERROR"], "(none)");
        for e in &index.errors {
            table.push(vec![e.file.clone(), e.error.clone()]);
        }
        report.table(table);
    }

    report
}

// ---------------------------------------------------------------------------
// fingerprint
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct FingerprintOutput {
    /// Nodes in the parsed tree — the complexity of what was hashed.
    node_count: u64,
    /// The `--min-nodes` floor in effect, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    min_nodes: Option<u64>,
    /// True when `node_count` fell below the floor, so the hashes are withheld.
    suppressed: bool,
    /// `None` when suppressed.
    path_set: Option<String>,
    /// `None` when suppressed.
    typed_path: Option<String>,
    /// `None` when suppressed.
    shape: Option<String>,
    repeated_motifs: Vec<MotifView>,
}

#[derive(Serialize)]
struct MotifView {
    hash: String,
    count: u64,
}

/// Build the fingerprint view, withholding hashes below the complexity floor.
///
/// Structural hashes are only discriminating above some complexity: trivial
/// documents are *supposed* to look alike, so identical hashes below the floor
/// say nothing about whether the documents are related. Rather than emit a
/// hash that will collide across unrelated inputs, report the node count and
/// mark the result suppressed so callers can skip indexing it.
fn build_fingerprint_output(
    result: &FingerprintResult,
    node_count: u64,
    min_nodes: u64,
) -> FingerprintOutput {
    let suppressed = node_count < min_nodes;

    let mut repeated_motifs: Vec<MotifView> = if suppressed {
        Vec::new()
    } else {
        result
            .subtree_frequencies
            .iter()
            .filter(|(_, &count)| count > 1)
            .map(|(h, &count)| MotifView {
                hash: hex(h),
                count,
            })
            .collect()
    };
    // Sort by count descending, breaking ties on the hash so the ordering is
    // fully specified rather than dependent on map iteration.
    repeated_motifs.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.hash.cmp(&b.hash)));

    FingerprintOutput {
        node_count,
        min_nodes: (min_nodes > 0).then_some(min_nodes),
        suppressed,
        path_set: (!suppressed).then(|| hex(&result.path_set)),
        typed_path: (!suppressed).then(|| hex(&result.typed_path)),
        shape: (!suppressed).then(|| hex(&result.shape)),
        repeated_motifs,
    }
}

/// Render an optional hash for text output.
fn show(value: Option<&str>) -> &str {
    value.unwrap_or("(suppressed)")
}

fn hex_slice(bytes: &[u8; 32]) -> String {
    hex(bytes)
}

/// Index a directory tree, reporting which structural shapes recur.
fn cmd_fingerprint_corpus(
    directory: &str,
    min_nodes: u64,
    group_depth: usize,
    cli: &Cli,
) -> Result<()> {
    let dir = Path::new(directory);
    let walk = corpus::collect_corpus_files(dir, &|p| is_selectable_file(p, cli))?;

    if walk.selected.is_empty() {
        anyhow::bail!(
            "no analysable files found under {directory} ({} file(s) scanned). \
             Use --input-format source to index source files.",
            walk.scanned
        );
    }

    if !cli.quiet {
        eprintln!(
            "Indexing {} of {} file(s)...",
            walk.selected.len(),
            walk.scanned
        );
    }

    let index = corpus::build_index(
        dir,
        &walk,
        min_nodes,
        group_depth,
        &|p| load_document_path(p, cli),
        &|doc| {
            let result = FingerprintAnalyzer
                .analyze(doc)
                .context("fingerprint analysis failed")?;
            Ok(hex(&result.shape))
        },
    );

    match cli.format {
        Format::Json => {
            let json = serde_json::to_string_pretty(&index).context("JSON serialization failed")?;
            println!("{json}");
        }
        Format::Text | Format::Markdown | Format::CompactAi => {
            let report = corpus_index_report(&index);
            let text = match cli.format {
                Format::Markdown => report.to_markdown(),
                _ => report.to_text(),
            };
            print!("{}", maybe_redact(&text, cli));
        }
    }

    Ok(())
}

fn cmd_fingerprint(input: &str, min_nodes: u64, cli: &Cli) -> Result<()> {
    if cli.streaming {
        let doc = load_document(input, cli)?;
        let events = vajra_core::emit_events(doc.value());
        let mut acc = StreamingFingerprintAccumulator::new();
        acc.process_events(&events);
        let result = acc.finalize();
        let node_count = doc.metadata().total_nodes;
        let suppressed = node_count < min_nodes;

        match cli.format {
            Format::Json => {
                let json = serde_json::json!({
                    "node_count": node_count,
                    "min_nodes": (min_nodes > 0).then_some(min_nodes),
                    "suppressed": suppressed,
                    "path_set": (!suppressed).then(|| hex_slice(&result.path_set)),
                    "typed_path": (!suppressed).then(|| hex_slice(&result.typed_path)),
                    "shape": serde_json::Value::Null,
                    "repeated_motifs": []
                });
                let json =
                    serde_json::to_string_pretty(&json).context("JSON serialization failed")?;
                println!("{json}");
            }
            Format::Text | Format::Markdown | Format::CompactAi => {
                let mut report = render::Report::new();
                report.heading("Structural Fingerprints (streaming)");
                let mut fields = vec![("Nodes".to_owned(), node_count.to_string())];
                if suppressed {
                    fields.push((
                        "Suppressed".to_owned(),
                        format!("node count below --min-nodes {min_nodes}"),
                    ));
                } else {
                    fields.push(("Path set".to_owned(), hex_slice(&result.path_set)));
                    fields.push(("Typed path".to_owned(), hex_slice(&result.typed_path)));
                }
                fields.push((
                    "Shape".to_owned(),
                    "(not available in streaming mode)".to_owned(),
                ));
                report.fields(fields);
                report.note(
                    "Streaming mode cannot produce the Merkle shape hash, which needs the\nwhole tree in memory.",
                );
                let rendered = match cli.format {
                    Format::Markdown => report.to_markdown(),
                    _ => report.to_text(),
                };
                print!("{}", maybe_redact(&rendered, cli));
            }
        }
    } else {
        let doc = load_document(input, cli)?;
        let result = FingerprintAnalyzer
            .analyze(&doc)
            .context("fingerprint analysis failed")?;
        let output = build_fingerprint_output(&result, doc.metadata().total_nodes, min_nodes);

        match cli.format {
            Format::Json => {
                let json =
                    serde_json::to_string_pretty(&output).context("JSON serialization failed")?;
                println!("{json}");
            }
            Format::Text | Format::Markdown | Format::CompactAi => {
                let mut report = render::Report::new();
                report.heading("Structural Fingerprints");

                let mut fields = vec![("Nodes".to_owned(), output.node_count.to_string())];
                if output.suppressed {
                    fields.push((
                        "Suppressed".to_owned(),
                        format!("node count below --min-nodes {min_nodes}"),
                    ));
                } else {
                    fields.push((
                        "Path set".to_owned(),
                        show(output.path_set.as_deref()).to_owned(),
                    ));
                    fields.push((
                        "Typed path".to_owned(),
                        show(output.typed_path.as_deref()).to_owned(),
                    ));
                    fields.push(("Shape".to_owned(), show(output.shape.as_deref()).to_owned()));
                }
                report.fields(fields);

                if output.suppressed {
                    report.note(
                        "Structural hashes are not discriminating at this size: the space of\ndistinct small shapes is tiny, so trivial documents collide.",
                    );
                } else {
                    report.heading("Repeated Motifs");
                    let mut t =
                        render::Table::new(&["HASH", "COUNT"], "no repeated subtree shapes found");
                    for m in &output.repeated_motifs {
                        t.push(vec![m.hash.clone(), m.count.to_string()]);
                    }
                    report.table(t);
                }

                let rendered = match cli.format {
                    Format::Markdown => report.to_markdown(),
                    _ => report.to_text(),
                };
                print!("{}", maybe_redact(&rendered, cli));
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// essence
// ---------------------------------------------------------------------------

fn cmd_essence(input: &str, cli: &Cli) -> Result<()> {
    let doc = load_document(input, cli)?;

    let stats = StatsAnalyzer
        .analyze(&doc)
        .context("stats analysis failed")?;
    let anomalies = AnomalyAnalyzer::default()
        .analyze(&doc)
        .context("anomaly analysis failed")?;
    let fingerprints = FingerprintAnalyzer
        .analyze(&doc)
        .context("fingerprint analysis failed")?;

    let output_format = match cli.format {
        Format::Text => OutputFormat::Text,
        Format::Json => OutputFormat::Json,
        Format::Markdown => OutputFormat::Markdown,
        Format::CompactAi => OutputFormat::CompactAi,
    };

    // Built-in profiles
    let staff_profile = StaffProfile;
    let engineer_profile = EngineerProfile;
    let auditor_profile = AuditorProfile;
    let ai_profile = AiProfile;
    let fraud_profile = FraudProfile;
    let health_profile = HealthProfile;

    // Load custom profiles from config if provided
    let custom_profiles = load_custom_profiles(cli)?;

    // Find matching profile
    let profile: &dyn ConcernProfile = match cli.profile.as_str() {
        "staff" => &staff_profile,
        "engineer" => &engineer_profile,
        "auditor" => &auditor_profile,
        "ai" => &ai_profile,
        "fraud" => &fraud_profile,
        "health" => &health_profile,
        name => {
            // Search custom profiles
            if let Some(custom) = custom_profiles.iter().find(|p| p.name() == name) {
                custom
            } else {
                eprintln!(
                    "vajra: unknown profile '{}', falling back to 'engineer'",
                    name
                );
                &engineer_profile
            }
        }
    };

    let mut builder = EssenceBuilder::new(&doc, profile)
        .with_stats(&stats)
        .with_anomalies(&anomalies)
        .with_fingerprint(&fingerprints);

    if let Some(budget) = cli.budget {
        builder = builder.with_budget(budget);
    }

    let essence = builder.build();

    let rendered = profile
        .render(&essence, output_format)
        .context("essence rendering failed")?;

    let rendered = maybe_redact(&rendered, cli);

    println!("{rendered}");
    Ok(())
}

// ---------------------------------------------------------------------------
// drift
// ---------------------------------------------------------------------------

fn cmd_drift(baseline: &str, candidate: &str, cli: &Cli) -> Result<()> {
    let lhs = load_document(baseline, cli)?;
    let rhs = load_document(candidate, cli)?;
    let report = full_drift(&lhs, &rhs);

    match cli.format {
        Format::Json => {
            let mut json = serde_json::Map::new();
            json.insert(
                "structural_similarity".into(),
                serde_json::json!(report.structural_similarity),
            );
            json.insert(
                "severity".into(),
                serde_json::json!(format!("{:?}", report.severity)),
            );
            json.insert(
                "added_paths".into(),
                serde_json::json!(report
                    .path_diff
                    .added
                    .iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()),
            );
            json.insert(
                "removed_paths".into(),
                serde_json::json!(report
                    .path_diff
                    .removed
                    .iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()),
            );
            json.insert(
                "type_changes".into(),
                serde_json::json!(report
                    .type_changes
                    .iter()
                    .map(|tc| serde_json::json!({
                        "path": tc.path,
                        "from": tc.from,
                        "to": tc.to,
                    }))
                    .collect::<Vec<_>>()),
            );
            json.insert(
                "distributional_drifts".into(),
                serde_json::json!(report
                    .distributional_drifts
                    .iter()
                    .map(|dd| serde_json::json!({
                        "path": dd.path,
                        "metric": format!("{:?}", dd.metric),
                        "value": dd.value,
                        "effect_size": dd.effect_size,
                    }))
                    .collect::<Vec<_>>()),
            );
            let json_str =
                serde_json::to_string_pretty(&json).context("JSON serialization failed")?;
            println!("{json_str}");
        }
        Format::Text | Format::Markdown | Format::CompactAi => {
            let mut out = render::Report::new();
            out.heading(format!("Drift Report: {baseline} -> {candidate}"));
            out.fields(vec![
                (
                    "Structural similarity".to_owned(),
                    format!("{:.4} (Jaccard)", report.structural_similarity),
                ),
                ("Severity".to_owned(), format!("{:?}", report.severity)),
            ]);

            for (label, paths) in [
                ("Added paths", &report.path_diff.added),
                ("Removed paths", &report.path_diff.removed),
            ] {
                out.heading(format!("{label} ({})", paths.len()));
                let mut t = render::Table::new(&["PATH"], "none");
                for p in paths {
                    t.push(vec![p.to_string()]);
                }
                out.table(t);
            }

            if !report.type_changes.is_empty() {
                out.heading(format!("Type changes ({})", report.type_changes.len()));
                let mut t = render::Table::new(&["PATH", "FROM", "TO"], "none");
                for tc in &report.type_changes {
                    t.push(vec![tc.path.clone(), tc.from.clone(), tc.to.clone()]);
                }
                out.table(t);
            }

            if !report.distributional_drifts.is_empty() {
                out.heading(format!(
                    "Distribution shifts ({})",
                    report.distributional_drifts.len()
                ));
                let mut t = render::Table::new(&["PATH", "METRIC", "VALUE", "EFFECT"], "none");
                for dd in &report.distributional_drifts {
                    t.push(vec![
                        dd.path.clone(),
                        format!("{:?}", dd.metric),
                        format!("{:.4}", dd.value),
                        format!("{:.4}", dd.effect_size),
                    ]);
                }
                out.table(t);
                out.note(
                    "Rank by EFFECT (unit-free, 0-1). VALUE is in each metric's own units, so\nJSD and Wasserstein values are not comparable with each other.",
                );
            }

            let txt = match cli.format {
                Format::Markdown => out.to_markdown(),
                _ => out.to_text(),
            };
            print!("{}", maybe_redact(&txt, cli));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// population drift (--group-by)
// ---------------------------------------------------------------------------

/// Extract a field value from a JSON record using a simple path expression.
fn extract_group_value(record: &serde_json::Value, path: &str) -> Option<String> {
    let key = path.strip_prefix("$.").unwrap_or(path);
    let mut current = record;
    for segment in key.split('.') {
        current = current.get(segment)?;
    }
    match current {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Serialize a per-pair drift report to a JSON value for output.
fn drift_pair_to_json(
    group_a: &str,
    group_b: &str,
    report: &vajra_drift::FullDriftReport,
) -> serde_json::Value {
    serde_json::json!({
        "group_a": group_a,
        "group_b": group_b,
        "severity": format!("{:?}", report.severity),
        "structural_similarity": report.structural_similarity,
        "added_paths": report.path_diff.added.iter().map(|p| p.to_string()).collect::<Vec<_>>(),
        "removed_paths": report.path_diff.removed.iter().map(|p| p.to_string()).collect::<Vec<_>>(),
        "type_changes": report.type_changes.iter().map(|tc| serde_json::json!({
            "path": tc.path,
            "from": tc.from,
            "to": tc.to,
        })).collect::<Vec<serde_json::Value>>(),
        "distributional_drifts": report.distributional_drifts.iter().map(|dd| serde_json::json!({
            "path": dd.path,
            "metric": format!("{:?}", dd.metric),
            "value": dd.value,
            "effect_size": dd.effect_size,
        })).collect::<Vec<serde_json::Value>>(),
    })
}

#[allow(clippy::too_many_lines)]
/// Compare two directory trees structurally, file by file.
fn cmd_tree_diff(baseline: &str, candidate: &str, cli: &Cli) -> Result<()> {
    let diff = treediff::diff_trees(
        Path::new(baseline),
        Path::new(candidate),
        &|p| is_selectable_file(p, cli),
        &|p| load_document_path(p, cli),
        &|doc| {
            let result = FingerprintAnalyzer
                .analyze(doc)
                .context("fingerprint analysis failed")?;
            Ok(hex(&result.shape))
        },
    )?;

    match cli.format {
        Format::Json => {
            let json = serde_json::to_string_pretty(&diff).context("JSON serialization failed")?;
            println!("{json}");
        }
        Format::Text | Format::Markdown | Format::CompactAi => {
            let mut out = render::Report::new();
            out.heading("Structural Tree Diff");
            let mut summary = vec![
                ("Baseline files".to_owned(), diff.baseline_files.to_string()),
                (
                    "Candidate files".to_owned(),
                    diff.candidate_files.to_string(),
                ),
            ];
            for kind in ["added", "removed", "changed", "unchanged"] {
                summary.push((
                    kind.to_owned(),
                    diff.summary.get(kind).copied().unwrap_or(0).to_string(),
                ));
            }
            summary.push((
                "Net node delta".to_owned(),
                format!("{:+}", diff.total_node_delta),
            ));
            out.fields(summary);

            let mut table =
                render::Table::new(&["CHANGE", "NODES", "PATH"], "(no structural differences)");
            for f in &diff.files {
                table.push(vec![
                    format!("{:?}", f.change).to_lowercase(),
                    f.node_delta
                        .map_or_else(|| "--".to_owned(), |d| format!("{d:+}")),
                    f.path.clone(),
                ]);
            }
            out.table(table);
            if !diff.files.is_empty() {
                out.note(
                    "Comparison is by structural shape, so reformatting and renaming do not \
                     register. A file whose shape changed grew or lost structure.",
                );
            }

            if !diff.errors.is_empty() {
                out.heading(format!("Errors ({})", diff.errors.len()));
                let mut errors = render::Table::new(&["PATH", "ERROR"], "(none)");
                for e in &diff.errors {
                    errors.push(vec![e.path.clone(), e.error.clone()]);
                }
                out.table(errors);
            }

            let text = match cli.format {
                Format::Markdown => out.to_markdown(),
                _ => out.to_text(),
            };
            print!("{}", maybe_redact(&text, cli));
        }
    }

    Ok(())
}

fn cmd_population_drift(input: &str, group_by: &str, cli: &Cli) -> Result<()> {
    // Use unified load_document so git repos are handled.
    let doc = load_document(input, cli)?;
    let raw_value = doc.value().clone();
    let records = match raw_value.as_array() {
        Some(arr) => arr,
        None => anyhow::bail!(
            "population drift requires a JSON array of records, but got {}",
            match &raw_value {
                serde_json::Value::Object(_) => "an object",
                serde_json::Value::String(_) => "a string",
                serde_json::Value::Number(_) => "a number",
                serde_json::Value::Bool(_) => "a boolean",
                serde_json::Value::Null => "null",
                serde_json::Value::Array(_) => "an array",
            }
        ),
    };
    let mut groups: BTreeMap<String, Vec<serde_json::Value>> = BTreeMap::new();
    let mut missing_count: usize = 0;
    for record in records {
        match extract_group_value(record, group_by) {
            Some(key) => groups.entry(key).or_default().push(record.clone()),
            None => missing_count += 1,
        }
    }
    if groups.is_empty() {
        anyhow::bail!(
            "group-by field '{}' not found in any record (checked {} records)",
            group_by,
            records.len()
        );
    }
    if groups.len() == 1 {
        let group_name = groups
            .keys()
            .next()
            .map(String::as_str)
            .unwrap_or("unknown");
        anyhow::bail!(
            "only one group found ('{}' with {} records), nothing to compare",
            group_name,
            groups.values().next().map_or(0, Vec::len)
        );
    }
    let group_count = groups.len();
    let pair_count = group_count * (group_count - 1) / 2;
    if group_count > 10 && !cli.quiet {
        eprintln!(
            "vajra: warning: {} groups produce {} pairwise comparisons",
            group_count, pair_count
        );
    }
    if missing_count > 0 && !cli.quiet {
        eprintln!(
            "vajra: warning: {} records skipped (missing '{}' field)",
            missing_count, group_by
        );
    }
    let group_names: Vec<String> = groups.keys().cloned().collect();
    let group_sizes: BTreeMap<String, usize> =
        groups.iter().map(|(k, v)| (k.clone(), v.len())).collect();
    let mut pairwise_drift = Vec::new();
    for i in 0..group_names.len() {
        for j in (i + 1)..group_names.len() {
            let name_a = &group_names[i];
            let name_b = &group_names[j];
            let records_a = &groups[name_a];
            let records_b = &groups[name_b];
            let json_a = serde_json::to_string(&serde_json::Value::Array(records_a.clone()))
                .context("failed to serialize group A")?;
            let json_b = serde_json::to_string(&serde_json::Value::Array(records_b.clone()))
                .context("failed to serialize group B")?;
            let doc_a = vajra_core::parse_str(&json_a)
                .map_err(|e| anyhow::anyhow!("failed to parse group '{}': {}", name_a, e))?;
            let doc_b = vajra_core::parse_str(&json_b)
                .map_err(|e| anyhow::anyhow!("failed to parse group '{}': {}", name_b, e))?;
            let report = full_drift(&doc_a, &doc_b);
            pairwise_drift.push((name_a.clone(), name_b.clone(), report));
        }
    }
    match cli.format {
        Format::Json => {
            let json_output = serde_json::json!({"groups": group_names, "group_sizes": group_sizes, "pairwise_drift": pairwise_drift.iter().map(|(a, b, report)| drift_pair_to_json(a, b, report)).collect::<Vec<_>>()});
            let json_str =
                serde_json::to_string_pretty(&json_output).context("JSON serialization failed")?;
            println!("{json_str}");
        }
        Format::Text | Format::Markdown | Format::CompactAi => {
            let mut out = render::Report::new();
            out.heading("Population Drift Report");
            out.fields(vec![
                ("Group-by".to_owned(), group_by.to_owned()),
                (
                    "Groups".to_owned(),
                    format!("{group_count} ({pair_count} pairwise comparisons)"),
                ),
            ]);
            let mut sizes = render::Table::new(&["GROUP", "RECORDS"], "(none)");
            for (name, size) in &group_sizes {
                sizes.push(vec![name.clone(), size.to_string()]);
            }
            out.table(sizes);

            for (name_a, name_b, report) in &pairwise_drift {
                out.heading(format!("{name_a} vs {name_b}"));
                out.fields(vec![
                    (
                        "Structural similarity".to_owned(),
                        format!("{:.4} (Jaccard)", report.structural_similarity),
                    ),
                    ("Severity".to_owned(), format!("{:?}", report.severity)),
                ]);
                let mut changes = Vec::new();
                if !report.path_diff.added.is_empty() {
                    changes.push((
                        format!("Added paths ({})", report.path_diff.added.len()),
                        report
                            .path_diff
                            .added
                            .iter()
                            .map(|p| format!("+ {p}"))
                            .collect(),
                    ));
                }
                if !report.path_diff.removed.is_empty() {
                    changes.push((
                        format!("Removed paths ({})", report.path_diff.removed.len()),
                        report
                            .path_diff
                            .removed
                            .iter()
                            .map(|p| format!("- {p}"))
                            .collect(),
                    ));
                }
                if !report.type_changes.is_empty() {
                    changes.push((
                        format!("Type changes ({})", report.type_changes.len()),
                        report
                            .type_changes
                            .iter()
                            .map(|tc| format!("{} : {} -> {}", tc.path, tc.from, tc.to))
                            .collect(),
                    ));
                }
                if !report.distributional_drifts.is_empty() {
                    changes.push((
                        format!(
                            "Distribution shifts ({})",
                            report.distributional_drifts.len()
                        ),
                        report
                            .distributional_drifts
                            .iter()
                            .map(|dd| {
                                format!(
                                    "{} : {:?} = {:.4}  (effect {:.4})",
                                    dd.path, dd.metric, dd.value, dd.effect_size
                                )
                            })
                            .collect(),
                    ));
                }
                if changes.is_empty() {
                    out.note("no structural or distributional differences");
                } else {
                    out.nested(changes);
                }
            }

            let text = match cli.format {
                Format::Markdown => out.to_markdown(),
                _ => out.to_text(),
            };
            print!("{}", maybe_redact(&text, cli));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// cluster
// ---------------------------------------------------------------------------

/// Whether a directory entry should be included when a command walks a
/// directory (`cluster`, `batch`).
///
/// Mirrors the format resolution in `load_document`: `--input-format source`
/// selects source files, anything else keeps the `.json` filter.
#[cfg(feature = "source")]
fn is_selectable_file(path: &Path, cli: &Cli) -> bool {
    if matches!(cli.input_format, Some(InputFormatArg::Source)) {
        return vajra_source::is_source_file(path);
    }
    path.extension().is_some_and(|ext| ext == "json")
}

#[cfg(not(feature = "source"))]
fn is_selectable_file(path: &Path, _cli: &Cli) -> bool {
    path.extension().is_some_and(|ext| ext == "json")
}

fn cmd_cluster(inputs: &[String], similarity_threshold: f64, cli: &Cli) -> Result<()> {
    if !(0.0..=1.0).contains(&similarity_threshold) {
        anyhow::bail!(
            "--similarity-threshold must be between 0.0 and 1.0 (got {similarity_threshold})"
        );
    }

    let mut docs = Vec::new();
    let mut names = Vec::new();

    for input in inputs {
        let path = Path::new(input);
        if path.is_dir() {
            let mut paths: Vec<std::path::PathBuf> = std::fs::read_dir(path)
                .with_context(|| format!("failed to read directory {input}"))?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_file() && is_selectable_file(p, cli))
                .collect();
            paths.sort();
            for p in paths {
                let name = p.display().to_string();
                let doc =
                    load_document(&name, cli).with_context(|| format!("failed to parse {name}"))?;
                names.push(name);
                docs.push(doc);
            }
        } else {
            let doc =
                load_document(input, cli).with_context(|| format!("failed to parse {input}"))?;
            names.push(input.clone());
            docs.push(doc);
        }
    }

    if docs.is_empty() {
        println!("No documents to cluster.");
        return Ok(());
    }

    let doc_refs: Vec<&Document> = docs.iter().collect();
    let result = cluster_documents_with_threshold(&doc_refs, 0, similarity_threshold);

    match cli.format {
        Format::Json => {
            let clusters_json: Vec<serde_json::Value> = result
                .clusters
                .iter()
                .enumerate()
                .map(|(i, members)| {
                    serde_json::json!({
                        "cluster": i,
                        "members": members.iter().map(|&idx| &names[idx]).collect::<Vec<_>>(),
                        "size": members.len(),
                    })
                })
                .collect();
            let json = serde_json::to_string_pretty(&serde_json::json!({
                "total_documents": docs.len(),
                "total_clusters": result.clusters.len(),
                "similarity_threshold": result.similarity_threshold,
                "clusters": clusters_json,
            }))
            .context("JSON serialization failed")?;
            println!("{json}");
        }
        Format::Text | Format::Markdown | Format::CompactAi => {
            let mut report = render::Report::new();
            report.heading("Clustering");
            report.fields(vec![
                ("Documents".to_owned(), docs.len().to_string()),
                (
                    "Similarity threshold".to_owned(),
                    format!("{:.2}", result.similarity_threshold),
                ),
                (
                    "Clusters found".to_owned(),
                    result.clusters.len().to_string(),
                ),
            ]);

            report.nested(
                result
                    .clusters
                    .iter()
                    .enumerate()
                    .map(|(i, members)| {
                        (
                            format!("Cluster {i} ({} members)", members.len()),
                            members.iter().map(|&idx| names[idx].clone()).collect(),
                        )
                    })
                    .collect(),
            );

            if result.clusters.len() == 1 && docs.len() > 1 {
                report.note(
                    "Everything landed in one cluster. Source files of the same language share\ngeneric AST paths, so try a higher --similarity-threshold (0.9-0.95).",
                );
            }

            let rendered = match cli.format {
                Format::Markdown => report.to_markdown(),
                _ => report.to_text(),
            };
            print!("{}", maybe_redact(&rendered, cli));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// invariants
// ---------------------------------------------------------------------------

/// Parse the `--bin` flag into a binning strategy.
fn parse_bin_flag(spec: &str) -> Result<vajra_stats::BinStrategy> {
    use vajra_stats::BinStrategy;
    let spec = spec.trim();
    if spec.eq_ignore_ascii_case("none") {
        return Ok(BinStrategy::None);
    }
    let (kind, count) = spec.split_once(':').ok_or_else(|| {
        anyhow::anyhow!(
            "invalid --bin value '{spec}'. Expected 'quantile:N', 'equal-width:N', or 'none'"
        )
    })?;
    let n: usize = count.trim().parse().map_err(|_| {
        anyhow::anyhow!("invalid bucket count '{count}' in --bin '{spec}': expected an integer")
    })?;
    if n < 2 {
        anyhow::bail!("--bin bucket count must be at least 2 (got {n})");
    }
    match kind.trim().to_lowercase().as_str() {
        "quantile" | "q" => Ok(BinStrategy::Quantile(n)),
        "equal-width" | "equalwidth" | "w" => Ok(BinStrategy::EqualWidth(n)),
        other => anyhow::bail!(
            "unknown binning strategy '{other}'. Available: quantile, equal-width, none"
        ),
    }
}

fn cmd_separation(
    input: &str,
    label_field: &str,
    base_rate: Option<f64>,
    positive_class: Option<&str>,
    top_k: usize,
    cli: &Cli,
) -> Result<()> {
    let doc = load_document(input, cli)?;
    let report = vajra_stats::separation_analysis(&doc, label_field, base_rate, positive_class)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let shown: Vec<&vajra_stats::FeatureSeparation> = if top_k == 0 {
        report.features.iter().collect()
    } else {
        report.features.iter().take(top_k).collect()
    };

    match cli.format {
        Format::Json => {
            let features: Vec<serde_json::Value> = shown
                .iter()
                .map(|f| {
                    serde_json::json!({
                        "path": f.path,
                        "kind": f.kind.as_str(),
                        "count": f.count,
                        "distinct_values": f.distinct_values,
                        "coverage": f.coverage,
                        "binned": f.binned,
                        "mutual_information": f.mutual_information,
                        "relationship_strength": f.relationship_strength,
                        "conditional_entropy": f.conditional_entropy,
                        "auc": f.auc,
                        "separation": f.separation,
                        "operating_point": f.operating_point.as_ref().map(|op| serde_json::json!({
                            "rule": op.rule,
                            "tpr": op.tpr,
                            "fpr": op.fpr,
                            "youden_j": op.youden_j,
                            "precision_at_base_rate": op.precision_at_base_rate,
                        })),
                    })
                })
                .collect();
            let json = serde_json::to_string_pretty(&serde_json::json!({
                "label_field": report.label_field,
                "labelled_records": report.labelled_records,
                "classes": report.classes,
                "baseline_entropy": report.baseline_entropy,
                "binary": report.binary,
                "positive_class": report.positive_class,
                "base_rate": report.base_rate,
                "features": features,
            }))
            .context("JSON serialization failed")?;
            println!("{json}");
        }
        Format::Text | Format::Markdown | Format::CompactAi => {
            let mut out = render::Report::new();

            out.heading(format!("Separation: {}", report.label_field));
            let mut fields = vec![(
                "Labelled records".to_owned(),
                report.labelled_records.to_string(),
            )];
            for (class, count) in &report.classes {
                fields.push((format!("  class {class}"), count.to_string()));
            }
            fields.push((
                "Baseline entropy".to_owned(),
                format!("{:.4} bits", report.baseline_entropy),
            ));
            fields.push((
                "Positive class".to_owned(),
                report.positive_class.clone().unwrap_or_else(|| {
                    format!(
                        "(none — {} classes, so AUC is undefined)",
                        report.classes.len()
                    )
                }),
            ));
            if let Some(rate) = report.base_rate {
                fields.push(("Assumed prevalence".to_owned(), rate.to_string()));
            }
            out.fields(fields);

            out.heading("Feature Separation");
            let mut t = render::Table::new(
                &["FEATURE", "KIND", "MI(bits)", "STRENGTH", "SEP"],
                "no features to evaluate",
            );
            for f in &shown {
                t.push(vec![
                    f.path.clone(),
                    if f.kind == vajra_stats::FieldKind::Numeric {
                        "num"
                    } else {
                        "cat"
                    }
                    .to_owned(),
                    format!("{:.4}", f.mutual_information),
                    format!("{:.4}", f.relationship_strength),
                    f.separation
                        .map_or_else(|| "--".to_owned(), |s| format!("{s:.4}")),
                ]);
            }
            out.table(t);
            out.note(
                "Ranked by MI (symmetric, bits) — the only column comparable across field\ntypes. SEP is |2*AUC-1| and is reported for ordered fields only.",
            );

            if report.base_rate.is_some() && !report.binary {
                out.note(format!(
                    "--base-rate is only meaningful for a two-class label; this one has {} classes, so no single decision rule is defined.",
                    report.classes.len()
                ));
            } else if report.base_rate.is_some() {
                out.heading("Best single rule, priced at the assumed prevalence");
                let mut r = render::Table::new(
                    &["FEATURE", "TPR", "FPR", "PRECISION", "RULE"],
                    "no operating point available",
                );
                for f in &shown {
                    if let Some(op) = &f.operating_point {
                        r.push(vec![
                            f.path.clone(),
                            format!("{:.4}", op.tpr),
                            format!("{:.4}", op.fpr),
                            op.precision_at_base_rate
                                .map_or_else(|| "--".to_owned(), |p| format!("{p:.5}")),
                            op.rule.clone(),
                        ]);
                    }
                }
                out.table(r);
                out.note(
                    "Precision here is what the rule would deliver in a population with the\nassumed prevalence — usually far below its corpus precision.",
                );
            }

            let rendered = match cli.format {
                Format::Markdown => out.to_markdown(),
                _ => out.to_text(),
            };
            print!("{}", maybe_redact(&rendered, cli));
        }
    }

    Ok(())
}

fn cmd_invariants(input: &str, top_k: usize, bin: &str, cli: &Cli) -> Result<()> {
    let bins = parse_bin_flag(bin)?;
    let doc = load_document(input, cli)?;
    let relationships =
        vajra_stats::relationships::discover_relationships_binned(&doc, top_k, bins);

    // Domain hints declare relationships a domain expects. Matching them against
    // what was discovered surfaces both confirmation and, more usefully,
    // expected relationships that are absent. Hints never weight the
    // information theory — they are context laid alongside it.
    let hint_outcomes = {
        let all = collect_hints();
        if all.is_empty() {
            Vec::new()
        } else {
            let document_paths: std::collections::BTreeSet<String> =
                doc.trie().all_paths().iter().map(|p| p.as_str()).collect();
            let pairs: Vec<(String, String, f64)> = relationships
                .iter()
                .map(|r| {
                    (
                        r.field_x.as_str(),
                        r.field_y.as_str(),
                        r.relationship_strength,
                    )
                })
                .collect();
            hints::evaluate_hints(&all, &document_paths, &pairs, 0.25)
        }
    };

    match cli.format {
        Format::Json => {
            let rels_json: Vec<serde_json::Value> = relationships
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "field_x": r.field_x.to_string(),
                        "field_y": r.field_y.to_string(),
                        "conditional_entropy": r.conditional_entropy,
                        "mean_pmi": r.mean_pmi,
                        "relationship_strength": r.relationship_strength,
                        "mutual_information": r.mutual_information,
                        "field_x_binned": r.field_x_binned,
                        "field_y_binned": r.field_y_binned,
                    })
                })
                .collect();
            let json = if hint_outcomes.is_empty() {
                // Preserve the documented flat-array contract when no domain
                // hint applies, which is the common case.
                serde_json::to_string_pretty(&rels_json)
            } else {
                serde_json::to_string_pretty(&serde_json::json!({
                    "relationships": rels_json,
                    "domain_hints": hint_outcomes,
                }))
            }
            .context("JSON serialization failed")?;
            println!("{json}");
        }
        Format::Text | Format::Markdown | Format::CompactAi => {
            let mut report = render::Report::new();
            let mark = |binned: bool| if binned { " [b]" } else { "" };

            report.heading("Cross-Field Relationships");
            let mut t = render::Table::new(
                &["PREDICTOR", "TARGET", "H(Y|X)", "PMI", "STRENGTH", "MI"],
                "no significant relationships discovered — these require repeated objects, e.g. an array of records",
            );
            for r in &relationships {
                t.push(vec![
                    format!("{}{}", r.field_x, mark(r.field_x_binned)),
                    format!("{}{}", r.field_y, mark(r.field_y_binned)),
                    format!("{:.4}", r.conditional_entropy),
                    format!("{:.4}", r.mean_pmi),
                    format!("{:.4}", r.relationship_strength),
                    format!("{:.4}", r.mutual_information),
                ]);
            }
            report.table(t);

            if !relationships.is_empty() {
                let mut note = String::from(
                    "STRENGTH is 1 - H(Y|X)/H(Y) and is direction-dependent; both directions\nof each pair are listed. Compare across pairs using MI (symmetric, bits).",
                );
                if relationships
                    .iter()
                    .any(|r| r.field_x_binned || r.field_y_binned)
                {
                    note.push_str("\n[b] = numeric field discretised before analysis (see --bin).");
                }
                report.note(note);
            }

            if !hint_outcomes.is_empty() {
                report.heading("Domain Expectations");
                let mut h_table = render::Table::new(
                    &["STATUS", "HINT", "RELATIONSHIP", "MAX STRENGTH"],
                    "none applicable",
                );
                for h in &hint_outcomes {
                    h_table.push(vec![
                        if h.observed { "observed" } else { "MISSING" }.to_owned(),
                        h.name.clone(),
                        h.relationship.clone(),
                        format!("{:.4}", h.max_strength),
                    ]);
                }
                report.table(h_table);
                report.note(
                    "MISSING means the domain expects these fields to relate and this\ndocument's data does not show it. Hints are context only — they never\nweight the entropy measures above.",
                );
            }

            let text = match cli.format {
                Format::Markdown => report.to_markdown(),
                _ => report.to_text(),
            };
            print!("{}", maybe_redact(&text, cli));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// query
// ---------------------------------------------------------------------------

fn cmd_query(input: &str, expression: &str, cli: &Cli) -> Result<()> {
    let doc = load_document(input, cli)?;

    // Parse the expression
    let expr = vajra_query::parse_expr(expression).map_err(|e| anyhow::anyhow!("{e}"))?;

    // Build query context with stats if available
    let stats = StatsAnalyzer.analyze(&doc).ok();
    let ctx = vajra_query::QueryContext {
        doc: &doc,
        stats: stats.as_ref(),
    };

    // Evaluate
    let result = vajra_query::evaluate(&expr, &ctx).map_err(|e| anyhow::anyhow!("{e}"))?;

    // Render based on result type
    let _ = &cli.format; // acknowledge format (query output is always plain)
    match result {
        vajra_query::QueryResult::Scalar(v) => println!("{v}"),
        vajra_query::QueryResult::PathSet(paths) => {
            for (path, values) in &paths {
                let vals: Vec<String> = values.iter().map(|v| v.to_string()).collect();
                println!("{path}: [{}]", vals.join(", "));
            }
        }
        vajra_query::QueryResult::FilteredPaths(paths) => {
            for path in &paths {
                println!("{path}");
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// batch
// ---------------------------------------------------------------------------

fn cmd_batch(directory: &str, cli: &Cli) -> Result<()> {
    let dir_path = PathBuf::from(directory);
    let files = batch::collect_batch_files(&dir_path, &|p| is_selectable_file(p, cli))?;

    let kind = match cli.input_format {
        Some(InputFormatArg::Source) => "source",
        _ => "JSON",
    };

    if files.selected.is_empty() {
        anyhow::bail!(
            "no {kind} files found in {directory} ({} file(s) present but not selected)",
            files.skipped.len()
        );
    }

    if !cli.quiet {
        eprintln!(
            "Analyzing {} {kind} files in parallel...",
            files.selected.len()
        );
        if !files.skipped.is_empty() {
            eprintln!("Skipping {} non-{kind} file(s).", files.skipped.len());
        }
    }

    let skipped_names: Vec<String> = files
        .skipped
        .iter()
        .map(|p| {
            p.file_name().map_or_else(
                || p.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            )
        })
        .collect();

    let result = batch::analyze_batch(&files.selected, &|p| load_document_path(p, cli))?;

    match cli.format {
        Format::Json => {
            let per_doc_json: Vec<serde_json::Value> = result
                .per_document
                .iter()
                .map(|d| {
                    serde_json::json!({
                        "file": d.file_name,
                        "total_nodes": d.document.metadata().total_nodes,
                        "max_depth": d.document.metadata().max_depth,
                        "distinct_paths": d.document.metadata().distinct_paths,
                        "anomaly_count": d.anomalies.numeric_outliers.len()
                            + d.anomalies.rare_values.len()
                            + d.anomalies.type_instabilities.len(),
                        "path_set_fingerprint": hex(&d.fingerprint.path_set),
                    })
                })
                .collect();

            let errors_json: Vec<serde_json::Value> = result
                .errors
                .iter()
                .map(|(name, err)| {
                    serde_json::json!({
                        "file": name,
                        "error": err,
                    })
                })
                .collect();

            let json = serde_json::to_string_pretty(&serde_json::json!({
                "total_documents": result.aggregate.total_documents,
                "total_nodes": result.aggregate.total_nodes,
                "common_paths": result.aggregate.common_paths,
                "rare_paths": result.aggregate.rare_paths,
                "per_document": per_doc_json,
                "errors": errors_json,
                "skipped_count": skipped_names.len(),
                "skipped": skipped_names,
            }))
            .context("JSON serialization failed")?;
            println!("{json}");
        }
        Format::Text | Format::Markdown | Format::CompactAi => {
            let mut out = render::Report::new();

            out.heading("Batch Analysis");
            out.fields(vec![
                (
                    "Documents".to_owned(),
                    result.aggregate.total_documents.to_string(),
                ),
                (
                    "Total nodes".to_owned(),
                    result.aggregate.total_nodes.to_string(),
                ),
                ("Skipped".to_owned(), skipped_names.len().to_string()),
                ("Errors".to_owned(), result.errors.len().to_string()),
            ]);

            out.heading("Per-Document Summary");
            let mut per_doc = render::Table::new(
                &["FILE", "NODES", "DEPTH", "PATHS", "ANOMALIES"],
                "no documents analysed",
            );
            for d in &result.per_document {
                let anomaly_count = d.anomalies.numeric_outliers.len()
                    + d.anomalies.rare_values.len()
                    + d.anomalies.type_instabilities.len();
                let meta = d.document.metadata();
                per_doc.push(vec![
                    d.file_name.clone(),
                    meta.total_nodes.to_string(),
                    meta.max_depth.to_string(),
                    meta.distinct_paths.to_string(),
                    anomaly_count.to_string(),
                ]);
            }
            out.table(per_doc);

            for (title, paths) in [
                (
                    "Common Paths (>50% of documents)",
                    &result.aggregate.common_paths,
                ),
                (
                    "Rare Paths (<10% of documents)",
                    &result.aggregate.rare_paths,
                ),
            ] {
                out.heading(title);
                let mut t = render::Table::new(&["PATH", "DOCUMENTS"], "none");
                for path in paths {
                    let count = result
                        .aggregate
                        .path_frequency
                        .get(path)
                        .copied()
                        .unwrap_or(0);
                    t.push(vec![path.clone(), count.to_string()]);
                }
                out.table(t);
            }

            if !result.errors.is_empty() {
                out.heading(format!("Errors ({})", result.errors.len()));
                let mut t = render::Table::new(&["FILE", "ERROR"], "none");
                for (name, err) in &result.errors {
                    t.push(vec![name.clone(), err.clone()]);
                }
                out.table(t);
            }

            if !skipped_names.is_empty() {
                out.heading(format!("Skipped ({})", skipped_names.len()));
                let mut t = render::Table::new(&["FILE"], "none");
                for name in &skipped_names {
                    t.push(vec![name.clone()]);
                }
                out.table(t);
                out.note(
                    "Skipped files were not analysed. Check this count before reading the\nresult as complete.",
                );
            }

            let txt = match cli.format {
                Format::Markdown => out.to_markdown(),
                _ => out.to_text(),
            };
            print!("{}", maybe_redact(&txt, cli));
        }
    }

    Ok(())
}

fn cmd_cascade(
    input: &str,
    entity_field: &str,
    time_field: &str,
    event_field: &str,
    response_values_str: &str,
    cli: &Cli,
) -> Result<()> {
    // Use unified load_document so git repos are handled.
    let doc = load_document(input, cli)?;
    let records = match doc.value() {
        serde_json::Value::Array(arr) => arr.clone(),
        _ => {
            anyhow::bail!("cascade command expects a JSON array of records");
        }
    };

    // When input is git, apply smart defaults for unmapped fields.
    let git_mode = is_git_input(input, cli);
    let effective_entity = if git_mode && entity_field == "file" {
        "author_name"
    } else {
        entity_field
    };
    let effective_time = if git_mode && time_field == "date" {
        "date"
    } else {
        time_field
    };
    let effective_event = if git_mode && event_field == "intent" {
        "subject"
    } else {
        event_field
    };

    let response_values: Vec<String> = response_values_str
        .split(',')
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();
    let config = vajra_cascade::CascadeConfig {
        entity_field: effective_entity.to_owned(),
        time_field: effective_time.to_owned(),
        event_field: effective_event.to_owned(),
        trigger_values: Vec::new(),
        response_values,
    };
    let result =
        vajra_cascade::detect_cascades(&records, &config).map_err(|e| anyhow::anyhow!("{e}"))?;
    match cli.format {
        Format::Json => {
            let j = cascade_json(&result);
            let s = serde_json::to_string_pretty(&j).context("JSON serialization failed")?;
            let s = maybe_redact(&s, cli);
            println!("{s}");
        }
        Format::Text => {
            let t = cascade_text(&result);
            let t = maybe_redact(&t, cli);
            print!("{t}");
        }
        Format::Markdown => {
            let m = cascade_md(&result);
            let m = maybe_redact(&m, cli);
            print!("{m}");
        }
        Format::CompactAi => {
            let j = cascade_json(&result);
            let s = serde_json::to_string(&j).context("JSON serialization failed")?;
            let s = maybe_redact(&s, cli);
            println!("{s}");
        }
    }
    Ok(())
}
fn cascade_json(r: &vajra_cascade::CascadeResult) -> serde_json::Value {
    let cj: Vec<serde_json::Value> = r.cascades.iter().map(|c| serde_json::json!({"entity":c.entity,"trigger":{"value":c.trigger.value,"author":c.trigger.author,"time":c.trigger.time},"response":{"value":c.response.value,"author":c.response.author,"time":c.response.time},"same_author":c.same_author})).collect();
    let hj: Vec<serde_json::Value> = r
        .hot_entities
        .iter()
        .map(|h| {
            serde_json::json!({
                "entity": h.entity,
                "total": h.total,
                "cascades": h.cascades,
                "cascade_ratio": h.cascade_ratio,
                "cascade_ratio_lower_bound": h.cascade_ratio_lower_bound,
            })
        })
        .collect();
    let mut out = serde_json::json!({
        "cascade_rate": r.cascade_rate,
        "self_fix_rate": r.self_fix_rate,
        "total_events": r.total_events,
        "total_cascades": r.cascades.len(),
        "hot_entities": hj,
        "cascades": cj,
    });
    if let (Some(note), Some(map)) = (&r.self_fix_rate_note, out.as_object_mut()) {
        map.insert(
            "self_fix_rate_note".to_owned(),
            serde_json::Value::String(note.clone()),
        );
    }
    out
}
fn cascade_text(r: &vajra_cascade::CascadeResult) -> String {
    use std::fmt::Write;
    let mut o = String::new();
    let _ = writeln!(o, "=== Cascade Analysis ===");
    let _ = writeln!(o, "  Total events:   {}", r.total_events);
    let _ = writeln!(o, "  Total cascades: {}", r.cascades.len());
    let _ = writeln!(o, "  Cascade rate:   {:.3}", r.cascade_rate);
    match r.self_fix_rate {
        Some(v) => {
            let _ = writeln!(o, "  Self-fix rate:  {v:.3}");
        }
        None => {
            let _ = writeln!(o, "  Self-fix rate:  (undefined)");
            if let Some(note) = &r.self_fix_rate_note {
                let _ = writeln!(o, "                  {note}");
            }
        }
    }
    let _ = writeln!(o);
    if !r.hot_entities.is_empty() {
        let _ = writeln!(o, "=== Hot Entities ===");
        let ew = r
            .hot_entities
            .iter()
            .map(|h| h.entity.len())
            .max()
            .unwrap_or(6)
            .max(6);
        let _ = writeln!(o, "  ranked by the 95% lower bound on the ratio");
        let _ = writeln!(
            o,
            "  {:<ew$}  {:>5}  {:>8}  {:>6}  {:>9}",
            "ENTITY",
            "TOTAL",
            "CASCADES",
            "RATIO",
            "LOWER 95%",
            ew = ew
        );
        for h in &r.hot_entities {
            let _ = writeln!(
                o,
                "  {:<ew$}  {:>5}  {:>8}  {:>6.3}  {:>9.3}",
                h.entity,
                h.total,
                h.cascades,
                h.cascade_ratio,
                h.cascade_ratio_lower_bound,
                ew = ew
            );
        }
        let _ = writeln!(o);
    }
    if !r.cascades.is_empty() {
        let _ = writeln!(o, "=== Cascade Chains ===");
        for (i, c) in r.cascades.iter().enumerate() {
            let sf = if c.same_author { " (self-fix)" } else { "" };
            let _ = writeln!(o, "  [{}] {}{}", i + 1, c.entity, sf);
            let _ = writeln!(
                o,
                "    trigger:  \"{}\" by {} at {}",
                c.trigger.value, c.trigger.author, c.trigger.time
            );
            let _ = writeln!(
                o,
                "    response: \"{}\" by {} at {}",
                c.response.value, c.response.author, c.response.time
            );
        }
    }
    o
}
fn cascade_md(r: &vajra_cascade::CascadeResult) -> String {
    use std::fmt::Write;
    let mut o = String::new();
    let _ = writeln!(o, "# Cascade Analysis\n");
    let _ = writeln!(o, "| Metric | Value |");
    let _ = writeln!(o, "|--------|-------|");
    let _ = writeln!(o, "| Total events | {} |", r.total_events);
    let _ = writeln!(o, "| Total cascades | {} |", r.cascades.len());
    let _ = writeln!(o, "| Cascade rate | {:.3} |", r.cascade_rate);
    match r.self_fix_rate {
        Some(v) => {
            let _ = writeln!(o, "| Self-fix rate | {v:.3} |");
        }
        None => {
            let _ = writeln!(o, "| Self-fix rate | (undefined) |");
        }
    }
    let _ = writeln!(o);
    if let Some(note) = &r.self_fix_rate_note {
        let _ = writeln!(o, "> Self-fix rate {note}\n");
    }
    if !r.hot_entities.is_empty() {
        let _ = writeln!(o, "## Hot Entities\n");
        let _ = writeln!(o, "Ranked by the 95% lower bound on the ratio.\n");
        let _ = writeln!(o, "| Entity | Total | Cascades | Ratio | Lower bound |");
        let _ = writeln!(o, "|--------|-------|----------|-------|-------------|");
        for h in &r.hot_entities {
            let _ = writeln!(
                o,
                "| {} | {} | {} | {:.3} | {:.3} |",
                h.entity, h.total, h.cascades, h.cascade_ratio, h.cascade_ratio_lower_bound
            );
        }
        let _ = writeln!(o);
    }
    if !r.cascades.is_empty() {
        let _ = writeln!(o, "## Cascade Chains\n");
        for (i, c) in r.cascades.iter().enumerate() {
            let sf = if c.same_author { " **(self-fix)**" } else { "" };
            let _ = writeln!(o, "### {}. {}{}\n", i + 1, c.entity, sf);
            let _ = writeln!(
                o,
                "- **Trigger**: \"{}\" by {} at {}",
                c.trigger.value, c.trigger.author, c.trigger.time
            );
            let _ = writeln!(
                o,
                "- **Response**: \"{}\" by {} at {}",
                c.response.value, c.response.author, c.response.time
            );
            let _ = writeln!(o);
        }
    }
    o
}

// ---------------------------------------------------------------------------
// governance command
// ---------------------------------------------------------------------------

fn cmd_governance(
    input: &str,
    author_field: Option<&str>,
    time_field: &str,
    resolve_identities: bool,
    email_field: Option<&str>,
    cli: &Cli,
) -> Result<()> {
    let doc = load_document(input, cli)?;
    let records = match doc.value().as_array() {
        Some(arr) => arr.clone(),
        None => {
            anyhow::bail!(
                "governance command expects a JSON array of records (e.g. commit or PR data)"
            );
        }
    };

    if records.is_empty() {
        anyhow::bail!("governance command received an empty array — no records to analyze");
    }

    let author_field = resolve_field(author_field, &fields::AUTHOR, &records, cli);
    let mut records = records;
    if resolve_identities {
        apply_identity_resolution(&mut records, &author_field, email_field, cli);
    }
    let report = governance_analysis(&records, &author_field, time_field)
        .map_err(|e| anyhow::anyhow!("{e}{}", field_hint(&e)))?;

    match cli.format {
        Format::Json => {
            let out = serde_json::to_string_pretty(&report).context("JSON serialization failed")?;
            let out = maybe_redact(&out, cli);
            println!("{out}");
        }
        Format::Markdown => {
            let md = render_governance_markdown(&report);
            let md = maybe_redact(&md, cli);
            print!("{md}");
        }
        Format::Text | Format::CompactAi => {
            let txt = render_governance_text(&report);
            let txt = maybe_redact(&txt, cli);
            print!("{txt}");
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// ingest-github command
// ---------------------------------------------------------------------------

fn cmd_ingest_github(
    repo: &str,
    output: &Path,
    pr_limit: usize,
    issue_limit: usize,
    commit_limit: usize,
    cli: &Cli,
) -> Result<()> {
    let config = vajra_core::GitHubIngestConfig {
        owner_repo: repo.to_string(),
        output_dir: output.to_path_buf(),
        pr_limit,
        issue_limit,
        commit_limit,
    };

    if !cli.quiet {
        eprintln!("vajra: ingesting {} into {}", repo, output.display());
    }

    let result = vajra_core::ingest_github(&config).map_err(|e| anyhow::anyhow!("{e}"))?;

    match cli.format {
        Format::Json | Format::CompactAi => {
            let summary = serde_json::json!({
                "repo": repo,
                "output_dir": result.output_dir.display().to_string(),
                "commits": result.commits,
                "pull_requests": result.pull_requests,
                "issues": result.issues,
                "releases": result.releases,
            });
            let out = if matches!(cli.format, Format::Json) {
                serde_json::to_string_pretty(&summary).context("JSON serialization failed")?
            } else {
                serde_json::to_string(&summary).context("JSON serialization failed")?
            };
            println!("{out}");
        }
        Format::Text | Format::Markdown => {
            let mut out = render::Report::new();
            out.heading("GitHub Ingestion Summary");
            out.fields(vec![
                ("Repository".to_owned(), repo.to_owned()),
                (
                    "Output dir".to_owned(),
                    result.output_dir.display().to_string(),
                ),
                ("Commits".to_owned(), result.commits.to_string()),
                ("Pull requests".to_owned(), result.pull_requests.to_string()),
                ("Issues".to_owned(), result.issues.to_string()),
                ("Releases".to_owned(), result.releases.to_string()),
            ]);
            let text = match cli.format {
                Format::Markdown => out.to_markdown(),
                _ => out.to_text(),
            };
            print!("{}", maybe_redact(&text, cli));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// core-team command
// ---------------------------------------------------------------------------

fn cmd_core_team(input: &str, resolve_identities: bool, cli: &Cli) -> Result<()> {
    let doc = load_document(input, cli)?;
    let mut records = commit_records_from_json(doc.value());

    if records.is_empty() {
        anyhow::bail!(
            "core-team command received no valid commit records — expected JSON array with author_name, author_email, date fields"
        );
    }

    // CommitRecord already carries both halves of the identity, so resolution
    // needs no extra selector here.
    if resolve_identities {
        let resolution = vajra_stats::resolve_identities(
            records
                .iter()
                .map(|r| (r.author_name.as_str(), r.author_email.as_str())),
        );
        for record in &mut records {
            // Both halves: detect_core_team keys on (name, email), so unifying
            // only the name leaves one person counted once per address.
            if let Some(email) = resolution.email_for(&record.author_name) {
                record.author_email = email.to_owned();
            }
            let canonical = resolution.canonical(&record.author_name).to_owned();
            record.author_name = canonical;
        }
        report_identity_merges(&resolution, cli);
    }

    let result = detect_core_team(&records).map_err(|e| anyhow::anyhow!("{e}"))?;

    match cli.format {
        Format::Json | Format::CompactAi => {
            let out = if matches!(cli.format, Format::Json) {
                serde_json::to_string_pretty(&result).context("JSON serialization failed")?
            } else {
                serde_json::to_string(&result).context("JSON serialization failed")?
            };
            let out = maybe_redact(&out, cli);
            println!("{out}");
        }
        Format::Text | Format::Markdown => {
            let mut report = render::Report::new();
            report.heading("Core Team Detection");
            report.fields(vec![("Method".to_owned(), result.detection_method.clone())]);

            for (label, group) in [
                ("Core", &result.core),
                ("Bots", &result.bots),
                ("Community", &result.community),
            ] {
                report.heading(format!("{label} ({})", group.len()));
                let mut t = render::Table::new(
                    &["NAME", "EMAIL", "COMMITS", "CONFIDENCE", "REASON"],
                    "none",
                );
                for a in group {
                    t.push(vec![
                        a.name.clone(),
                        a.email.clone().unwrap_or_else(|| "(no email)".to_owned()),
                        a.commits.to_string(),
                        format!("{:?}", a.confidence),
                        a.reason.clone(),
                    ]);
                }
                report.table(t);
            }

            let txt = match cli.format {
                Format::Markdown => report.to_markdown(),
                _ => report.to_text(),
            };
            print!("{}", maybe_redact(&txt, cli));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// compare command
// ---------------------------------------------------------------------------

/// Per-dataset metrics for cross-repo comparison.
#[derive(Debug, Clone, Serialize)]
struct DatasetMetrics {
    label: String,
    total_records: usize,
    author_entropy: f64,
    author_cardinality: usize,
    fix_ratio: f64,
    one_commit_rate: f64,
}

/// A single pairwise drift observation between two datasets.
#[derive(Debug, Clone, Serialize)]
struct PairwiseDrift {
    a: String,
    b: String,
    severity: String,
    similarity: f64,
}

/// Full comparison result for JSON output.
#[derive(Debug, Clone, Serialize)]
struct CompareResult {
    datasets: Vec<DatasetMetrics>,
    pairwise_drift: Vec<PairwiseDrift>,
}

/// Parse comma-separated labels, or derive labels from file paths.
fn parse_labels_or_filenames(labels: Option<&str>, inputs: &[String]) -> Vec<String> {
    if let Some(label_str) = labels {
        let parsed: Vec<String> = label_str.split(',').map(|s| s.trim().to_owned()).collect();
        if parsed.len() == inputs.len() {
            return parsed;
        }
        // Fall through to filenames if count doesn't match
    }
    inputs
        .iter()
        .map(|input| {
            Path::new(input)
                .file_stem()
                .map_or_else(|| input.clone(), |s| s.to_string_lossy().into_owned())
        })
        .collect()
}

/// Compute per-dataset metrics from a JSON array of records.
fn compute_dataset_metrics(
    records: &[serde_json::Value],
    label: &str,
    author_field: &str,
    message_field: &str,
) -> DatasetMetrics {
    let total_records = records.len();

    // Author distribution
    let mut author_counts: BTreeMap<String, u64> = BTreeMap::new();
    for record in records {
        if let Some(author_val) = extract_json_path(record, author_field) {
            if let Some(name) = author_val.as_str() {
                *author_counts.entry(name.to_owned()).or_insert(0) += 1;
            }
        }
    }
    let author_cardinality = author_counts.len();
    let counts_vec: Vec<u64> = author_counts.values().copied().collect();
    let author_entropy = shannon_entropy_from_counts(&counts_vec);

    // Fix ratio: count messages containing fix-related patterns
    let fix_patterns = ["fix", "bug", "hotfix", "patch", "revert"];
    let mut total_msgs = 0_u64;
    let mut fix_msgs = 0_u64;
    for record in records {
        if let Some(msg_val) = extract_json_path(record, message_field) {
            if let Some(msg) = msg_val.as_str() {
                total_msgs += 1;
                let lower = msg.to_lowercase();
                if fix_patterns.iter().any(|p| lower.contains(p)) {
                    fix_msgs += 1;
                }
            }
        }
    }
    #[allow(clippy::cast_precision_loss)] // counts well within f64 range
    let fix_ratio = if total_msgs > 0 {
        fix_msgs as f64 / total_msgs as f64
    } else {
        0.0
    };

    // One-commit rate: authors with exactly 1 commit / total authors
    #[allow(clippy::cast_precision_loss)] // counts well within f64 range
    let one_commit_rate = if author_cardinality > 0 {
        let one_commit_authors = author_counts.values().filter(|&&c| c == 1).count();
        one_commit_authors as f64 / author_cardinality as f64
    } else {
        0.0
    };

    DatasetMetrics {
        label: label.to_owned(),
        total_records,
        author_entropy,
        author_cardinality,
        fix_ratio,
        one_commit_rate,
    }
}

fn cmd_report(
    input: &str,
    title: &str,
    output: &str,
    repo_name: Option<&str>,
    cli: &Cli,
) -> Result<()> {
    let input_path = Path::new(input);
    let repo = repo_name.unwrap_or("unknown");

    if !cli.quiet {
        eprintln!("vajra: generating report from {}", input_path.display());
    }

    let data = vajra_report::load_report_data(input_path, title, repo)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let html = vajra_report::generate_html(&data);

    std::fs::write(output, &html).with_context(|| format!("Failed to write report to {output}"))?;

    if !cli.quiet {
        let sources_count = data.config.data_sources.len();
        eprintln!(
            "vajra: report written to {} ({} data sources, {} bytes)",
            output,
            sources_count,
            html.len()
        );
    }

    Ok(())
}

#[allow(clippy::too_many_lines)]
fn cmd_compare(
    inputs: &[String],
    labels: Option<&str>,
    author_field: Option<&str>,
    _time_field: &str,
    message_field: Option<&str>,
    cli: &Cli,
) -> Result<()> {
    if inputs.len() < 2 {
        anyhow::bail!(
            "compare requires at least 2 input files, got {}",
            inputs.len()
        );
    }

    let labels = parse_labels_or_filenames(labels, inputs);

    // Load all datasets
    let mut datasets: Vec<Document> = Vec::with_capacity(inputs.len());
    let mut record_sets: Vec<Vec<serde_json::Value>> = Vec::with_capacity(inputs.len());
    for (i, input) in inputs.iter().enumerate() {
        let doc = load_document(input, cli)
            .with_context(|| format!("failed to load dataset '{}' ({})", labels[i], input))?;
        let records = match doc.value().as_array() {
            Some(arr) => arr.clone(),
            None => {
                anyhow::bail!(
                    "compare expects each input to be a JSON array of records, but '{}' ({}) is not an array",
                    labels[i],
                    input
                );
            }
        };
        if records.is_empty() {
            anyhow::bail!(
                "compare received an empty array for '{}' ({})",
                labels[i],
                input
            );
        }
        record_sets.push(records);
        datasets.push(doc);
    }

    // Per-dataset metrics
    let metrics: Vec<DatasetMetrics> = record_sets
        .iter()
        .zip(labels.iter())
        .map(|(records, label)| {
            // Resolved per dataset: comparing a git repository against a GitHub
            // ingest means the two carry different field names, and a single
            // selector cannot read both.
            let author =
                resolve_field_labelled(author_field, &fields::AUTHOR, records, Some(label), cli);
            let message =
                resolve_field_labelled(message_field, &fields::MESSAGE, records, Some(label), cli);
            compute_dataset_metrics(records, label, &author, &message)
        })
        .collect();

    // Pairwise drift
    let mut drift_pairs: Vec<PairwiseDrift> = Vec::new();
    for i in 0..datasets.len() {
        for j in (i + 1)..datasets.len() {
            let report = full_drift(&datasets[i], &datasets[j]);
            drift_pairs.push(PairwiseDrift {
                a: labels[i].clone(),
                b: labels[j].clone(),
                severity: format!("{:?}", report.severity),
                similarity: report.structural_similarity,
            });
        }
    }

    let result = CompareResult {
        datasets: metrics,
        pairwise_drift: drift_pairs,
    };

    match cli.format {
        Format::Json => {
            let out = serde_json::to_string_pretty(&result).context("JSON serialization failed")?;
            let out = maybe_redact(&out, cli);
            println!("{out}");
        }
        Format::CompactAi => {
            let out = serde_json::to_string(&result).context("JSON serialization failed")?;
            let out = maybe_redact(&out, cli);
            println!("{out}");
        }
        Format::Markdown => {
            let md = compare_markdown(&result);
            let md = maybe_redact(&md, cli);
            print!("{md}");
        }
        Format::Text => {
            let txt = compare_text(&result);
            let txt = maybe_redact(&txt, cli);
            print!("{txt}");
        }
    }

    Ok(())
}

fn compare_markdown(result: &CompareResult) -> String {
    use std::fmt::Write;
    let mut out = String::new();

    let _ = writeln!(out, "## Project Comparison\n");

    // Header row
    let _ = write!(out, "| Metric |");
    for ds in &result.datasets {
        let _ = write!(out, " {} |", ds.label);
    }
    let _ = writeln!(out);

    // Separator row
    let _ = write!(out, "|---|");
    for _ in &result.datasets {
        let _ = write!(out, "---|");
    }
    let _ = writeln!(out);

    // Records row
    let _ = write!(out, "| Records |");
    for ds in &result.datasets {
        let _ = write!(out, " {} |", ds.total_records);
    }
    let _ = writeln!(out);

    // Author entropy row
    let _ = write!(out, "| Author entropy |");
    for ds in &result.datasets {
        let _ = write!(out, " {:.2} |", ds.author_entropy);
    }
    let _ = writeln!(out);

    // Author cardinality row
    let _ = write!(out, "| Author cardinality |");
    for ds in &result.datasets {
        let _ = write!(out, " {} |", ds.author_cardinality);
    }
    let _ = writeln!(out);

    // Fix ratio row
    let _ = write!(out, "| Fix ratio |");
    for ds in &result.datasets {
        let _ = write!(out, " {:.1}% |", ds.fix_ratio * 100.0);
    }
    let _ = writeln!(out);

    // One-commit rate row
    let _ = write!(out, "| One-commit rate |");
    for ds in &result.datasets {
        let _ = write!(out, " {:.1}% |", ds.one_commit_rate * 100.0);
    }
    let _ = writeln!(out);

    // Pairwise drift section
    let _ = writeln!(out);
    let _ = writeln!(out, "## Pairwise Drift\n");
    let _ = writeln!(out, "| Pair | Severity | Similarity |");
    let _ = writeln!(out, "|---|---|---|");
    for dp in &result.pairwise_drift {
        let _ = writeln!(
            out,
            "| {} vs {} | {} | {:.1} |",
            dp.a, dp.b, dp.severity, dp.similarity
        );
    }

    out
}

fn compare_text(result: &CompareResult) -> String {
    use std::fmt::Write;
    let mut out = String::new();

    let _ = writeln!(out, "=== Project Comparison ===\n");

    // Find column widths
    let label_width = result
        .datasets
        .iter()
        .map(|ds| ds.label.len())
        .max()
        .unwrap_or(5)
        .max(10);

    // Header
    let _ = write!(out, "  {:<20}", "Metric");
    for ds in &result.datasets {
        let _ = write!(out, "  {:>w$}", ds.label, w = label_width);
    }
    let _ = writeln!(out);

    // Separator
    let _ = write!(out, "  {:<20}", "------");
    for _ in &result.datasets {
        let _ = write!(out, "  {:>w$}", "------", w = label_width);
    }
    let _ = writeln!(out);

    // Records
    let _ = write!(out, "  {:<20}", "Records");
    for ds in &result.datasets {
        let _ = write!(out, "  {:>w$}", ds.total_records, w = label_width);
    }
    let _ = writeln!(out);

    // Author entropy
    let _ = write!(out, "  {:<20}", "Author entropy");
    for ds in &result.datasets {
        let _ = write!(out, "  {:>w$.2}", ds.author_entropy, w = label_width);
    }
    let _ = writeln!(out);

    // Author cardinality
    let _ = write!(out, "  {:<20}", "Author cardinality");
    for ds in &result.datasets {
        let _ = write!(out, "  {:>w$}", ds.author_cardinality, w = label_width);
    }
    let _ = writeln!(out);

    // Fix ratio
    let _ = write!(out, "  {:<20}", "Fix ratio");
    for ds in &result.datasets {
        let _ = write!(
            out,
            "  {:>w$.1}%",
            ds.fix_ratio * 100.0,
            w = label_width - 1
        );
    }
    let _ = writeln!(out);

    // One-commit rate
    let _ = write!(out, "  {:<20}", "One-commit rate");
    for ds in &result.datasets {
        let _ = write!(
            out,
            "  {:>w$.1}%",
            ds.one_commit_rate * 100.0,
            w = label_width - 1
        );
    }
    let _ = writeln!(out);

    // Pairwise drift
    let _ = writeln!(out);
    let _ = writeln!(out, "=== Pairwise Drift ===\n");
    let pair_width = result
        .pairwise_drift
        .iter()
        .map(|dp| dp.a.len() + dp.b.len() + 4)
        .max()
        .unwrap_or(15)
        .max(15);
    let _ = writeln!(
        out,
        "  {:<pw$}  {:>10}  {:>10}",
        "PAIR",
        "SEVERITY",
        "SIMILARITY",
        pw = pair_width
    );
    for dp in &result.pairwise_drift {
        let pair_label = format!("{} vs {}", dp.a, dp.b);
        let _ = writeln!(
            out,
            "  {:<pw$}  {:>10}  {:>10.4}",
            pair_label,
            dp.severity,
            dp.similarity,
            pw = pair_width
        );
    }

    out
}

// ---------------------------------------------------------------------------
// audit command
// ---------------------------------------------------------------------------

/// Parse a GitHub repository URL or shorthand into "owner/repo" form.
///
/// Accepted formats:
///   - `github.com/owner/repo`
///   - `https://github.com/owner/repo`
///   - `http://github.com/owner/repo`
///   - `owner/repo`
///
/// Trailing slashes and `.git` suffixes are stripped.
fn parse_repo_url(input: &str) -> Result<String> {
    let s = input
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("github.com/")
        .trim_end_matches('/')
        .trim_end_matches(".git");
    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
        Ok(s.to_string())
    } else {
        anyhow::bail!(
            "Invalid repository: '{}'. Expected format: owner/repo or github.com/owner/repo",
            input
        )
    }
}

#[allow(clippy::too_many_lines)]
fn cmd_audit(
    repo: &str,
    output: Option<&str>,
    commit_limit: usize,
    pr_limit: usize,
    issue_limit: usize,
    cli: &Cli,
) -> Result<()> {
    // 1. Parse repo URL
    let owner_repo = parse_repo_url(repo)?;

    // 2. Create temp directory for ingested data
    let tmp_dir = tempfile::tempdir().context("failed to create temp directory")?;
    let data_dir = tmp_dir.path().join(owner_repo.replace('/', "_"));
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("failed to create data directory: {}", data_dir.display()))?;

    // 3. Ingest
    if !cli.quiet {
        eprintln!("vajra: [1/3] Ingesting {}...", owner_repo);
    }
    let ingest_config = vajra_core::GitHubIngestConfig {
        owner_repo: owner_repo.clone(),
        output_dir: data_dir.clone(),
        pr_limit,
        issue_limit,
        commit_limit,
    };
    let ingest_result =
        vajra_core::ingest_github(&ingest_config).map_err(|e| anyhow::anyhow!("{e}"))?;

    if !cli.quiet {
        eprintln!(
            "vajra:   ingested {} commits, {} PRs, {} issues, {} releases",
            ingest_result.commits,
            ingest_result.pull_requests,
            ingest_result.issues,
            ingest_result.releases,
        );
    }

    // 4. Run analyses on commits.json
    let commits_path = data_dir.join("commits.json");
    if !commits_path.exists() {
        anyhow::bail!(
            "ingestion did not produce commits.json in {}",
            data_dir.display()
        );
    }

    if !cli.quiet {
        eprintln!("vajra: [2/3] Analyzing...");
    }

    // -- Stats --
    match load_documents_aggregated(
        commits_path.to_string_lossy().as_ref(),
        Some(vajra_core::InputFormat::Json),
    ) {
        Ok(doc) => {
            // stats.json
            match StatsAnalyzer.analyze(&doc) {
                Ok(stats_result) => {
                    let stats_output = build_stats_output(&stats_result);
                    if let Ok(json) = serde_json::to_string_pretty(&stats_output) {
                        let _ = std::fs::write(data_dir.join("stats.json"), json);
                    }
                }
                Err(e) => {
                    if !cli.quiet {
                        eprintln!("vajra:   stats analysis failed: {e} (skipping)");
                    }
                }
            }

            // anomalies.json
            let anomaly_analyzer = AnomalyAnalyzer::default();
            match anomaly_analyzer.analyze(&doc) {
                Ok(anomaly_report) => {
                    let anomaly_output = build_anomaly_output(&anomaly_report);
                    if let Ok(json) = serde_json::to_string_pretty(&anomaly_output) {
                        let _ = std::fs::write(data_dir.join("anomalies.json"), json);
                    }
                }
                Err(e) => {
                    if !cli.quiet {
                        eprintln!("vajra:   anomaly analysis failed: {e} (skipping)");
                    }
                }
            }

            // invariants.json
            let relationships = vajra_stats::relationships::discover_relationships(&doc, 50);
            if !relationships.is_empty() {
                let rels_json: Vec<serde_json::Value> = relationships
                    .iter()
                    .map(|r| {
                        serde_json::json!({
                            "field_x": r.field_x.to_string(),
                            "field_y": r.field_y.to_string(),
                            "conditional_entropy": r.conditional_entropy,
                            "mean_pmi": r.mean_pmi,
                            "relationship_strength": r.relationship_strength,
                            "mutual_information": r.mutual_information,
                            "field_x_binned": r.field_x_binned,
                            "field_y_binned": r.field_y_binned,
                        })
                    })
                    .collect();
                if let Ok(json) = serde_json::to_string_pretty(&rels_json) {
                    let _ = std::fs::write(data_dir.join("invariants.json"), json);
                }
            }

            // Extract records for governance, score, and temporal
            if let Some(records) = doc.value().as_array() {
                let records = records.clone();

                // governance.json
                match governance_analysis(&records, "$.author_name", "$.date") {
                    Ok(gov_report) => {
                        if let Ok(json) = serde_json::to_string_pretty(&gov_report) {
                            let _ = std::fs::write(data_dir.join("governance.json"), json);
                        }
                    }
                    Err(e) => {
                        if !cli.quiet {
                            eprintln!("vajra:   governance analysis failed: {e} (skipping)");
                        }
                    }
                }

                // score.json
                let metrics =
                    extract_health_metrics(&records, "$.author_name", "$.date", "$.subject", None);
                let weights = HealthWeights::default();
                if let Some(score) = compute_health_score(&metrics, &weights) {
                    let score_json = score_to_json(&score);
                    if let Ok(json) = serde_json::to_string_pretty(&score_json) {
                        let _ = std::fs::write(data_dir.join("score.json"), json);
                    }
                }

                // temporal.json (windowed analysis with month granularity)
                if !records.is_empty() {
                    use vajra_stats::temporal::{windowed_analysis, WindowGranularity};
                    match windowed_analysis(&records, "$.date", WindowGranularity::Month) {
                        Ok(temporal_result) => {
                            if let Ok(json) = serde_json::to_string_pretty(&temporal_result) {
                                let _ = std::fs::write(data_dir.join("temporal.json"), json);
                            }
                        }
                        Err(e) => {
                            if !cli.quiet {
                                eprintln!("vajra:   temporal analysis failed: {e} (skipping)");
                            }
                        }
                    }
                }
            }
        }
        Err(e) => {
            if !cli.quiet {
                eprintln!("vajra:   failed to load commits.json: {e}");
                eprintln!("vajra:   report will have limited data");
            }
        }
    }

    // 5. Generate report
    if !cli.quiet {
        eprintln!("vajra: [3/3] Generating report...");
    }

    let title = format!("{} Audit Report", owner_repo);
    let data = vajra_report::load_report_data(&data_dir, &title, &owner_repo)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let html = vajra_report::generate_html(&data);

    let default_output = format!("{}-report.html", owner_repo.replace('/', "-"));
    let output_path = output.unwrap_or(&default_output);
    std::fs::write(output_path, &html)
        .with_context(|| format!("failed to write report to {output_path}"))?;

    if !cli.quiet {
        eprintln!(
            "vajra: report written to {} ({} bytes)",
            output_path,
            html.len()
        );
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// audit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod audit_tests {
    use super::parse_repo_url;

    #[test]
    fn parse_owner_repo() {
        assert_eq!(
            parse_repo_url("facebook/react").unwrap_or_default(),
            "facebook/react"
        );
    }

    #[test]
    fn parse_github_url() {
        assert_eq!(
            parse_repo_url("github.com/facebook/react").unwrap_or_default(),
            "facebook/react"
        );
    }

    #[test]
    fn parse_https_url() {
        assert_eq!(
            parse_repo_url("https://github.com/facebook/react").unwrap_or_default(),
            "facebook/react"
        );
    }

    #[test]
    fn parse_http_url() {
        assert_eq!(
            parse_repo_url("http://github.com/facebook/react").unwrap_or_default(),
            "facebook/react"
        );
    }

    #[test]
    fn parse_trailing_slash() {
        assert_eq!(
            parse_repo_url("github.com/facebook/react/").unwrap_or_default(),
            "facebook/react"
        );
    }

    #[test]
    fn parse_git_suffix() {
        assert_eq!(
            parse_repo_url("https://github.com/facebook/react.git").unwrap_or_default(),
            "facebook/react"
        );
    }

    #[test]
    fn parse_invalid_bare_name() {
        assert!(parse_repo_url("just-a-name").is_err());
    }

    #[test]
    fn parse_invalid_too_many_segments() {
        assert!(parse_repo_url("a/b/c").is_err());
    }

    #[test]
    fn parse_invalid_empty_parts() {
        assert!(parse_repo_url("/repo").is_err());
    }
}

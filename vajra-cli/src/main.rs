mod batch;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;

use vajra_anomaly::{AnomalyAnalyzer, AnomalyReport};
use vajra_core::input::load_documents_aggregated;
use vajra_core::{parse_file, InputFormat};
use vajra_drift::full_drift;
use vajra_essence::{
    AiProfile, AuditorProfile, CustomProfile, EngineerProfile, EssenceBuilder, FraudProfile,
    HealthProfile, StaffProfile,
};
use vajra_fingerprint::{
    cluster_documents, FingerprintAnalyzer, FingerprintResult, StreamingFingerprintAccumulator,
};
use vajra_stats::{StatsAnalyzer, StatsResult, StreamingStatsAccumulator};
use vajra_types::traits::{ConcernProfile, OutputFormat};
use vajra_types::{Analyzer, Document};

// ---------------------------------------------------------------------------
// CLI argument definitions
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(name = "vajra", about = "Structural analysis toolkit for JSON data")]
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

    /// Use streaming analysis pipeline (bounded memory, sketch-based stats)
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
    },
    /// Cluster similar documents in a batch
    Cluster {
        /// JSON files to cluster
        #[arg(required = true, num_args = 1..)]
        inputs: Vec<String>,
    },
    /// Discover cross-field relationships
    Invariants {
        /// Path to JSON file, or `-` for stdin
        input: String,
        /// Maximum number of field pairs to consider
        #[arg(long, default_value = "50")]
        top_k: usize,
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
    /// List all available profiles (built-in and custom)
    Profiles,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    let cli = Cli::parse();

    let result = match &cli.command {
        Command::Inspect { input } => cmd_inspect(input, &cli),
        Command::Stats {
            input,
            window,
            time_field,
        } => cmd_stats(input, *window, time_field.as_deref(), &cli),
        Command::Anomalies { input } => cmd_anomalies(input, &cli),
        Command::Fingerprint { input } => cmd_fingerprint(input, &cli),
        Command::Essence { input } => cmd_essence(input, &cli),
        Command::Drift {
            baseline,
            candidate,
            group_by,
        } => {
            if let Some(field) = group_by {
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
        Command::Cluster { inputs } => cmd_cluster(inputs, &cli),
        Command::Invariants { input, top_k } => cmd_invariants(input, *top_k, &cli),
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
        Command::Profiles => cmd_profiles(&cli),
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
            println!("=== Built-in Profiles ===");
            for (name, desc) in &builtin_profiles {
                println!("  {:<12} {}", name, desc);
            }

            if !custom_profiles.is_empty() {
                println!();
                println!("=== Custom Profiles ===");
                for p in &custom_profiles {
                    println!("  {:<12} {}", p.name(), p.description());
                }
            }
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

    // Encoding plugin registered LAST — other plugins claim specific types
    // (JWT, SHA hashes) first; encoding detects the general encoding pattern.
    #[cfg(feature = "encoding")]
    {
        let plugin = vajra_domain_encoding::EncodingPlugin;
        recognizers.extend(vajra_types::traits::VajraPlugin::type_recognizers(&plugin));
    }

    recognizers
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

    let output = InspectOutput {
        metadata: metadata_view,
        paths: path_views,
        fingerprints: fp_view,
        domain_hints,
    };

    match cli.format {
        Format::Json => {
            let json =
                serde_json::to_string_pretty(&output).context("JSON serialization failed")?;
            let json = maybe_redact(&json, cli);
            println!("{json}");
        }
        Format::Text | Format::Markdown | Format::CompactAi => {
            println!("=== Document Metadata ===");
            println!("  Total nodes:    {}", output.metadata.total_nodes);
            println!("  Max depth:      {}", output.metadata.max_depth);
            println!("  Distinct paths: {}", output.metadata.distinct_paths);
            println!("  Raw size:       {} bytes", output.metadata.raw_size_bytes);
            println!();

            println!("=== Wildcard Paths ===");
            // Compute column widths
            let max_path = output.paths.iter().map(|p| p.path.len()).max().unwrap_or(4);
            let max_type = output
                .paths
                .iter()
                .map(|p| p.dominant_type.len())
                .max()
                .unwrap_or(4);

            println!(
                "  {:<path_w$}  {:<type_w$}  {:>6}  {:>12}  {:>10}",
                "PATH",
                "TYPE",
                "COUNT",
                "INSTABILITY",
                "NULL_RATE",
                path_w = max_path,
                type_w = max_type,
            );
            for p in &output.paths {
                println!(
                    "  {:<path_w$}  {:<type_w$}  {:>6}  {:>12.4}  {:>10.4}",
                    p.path,
                    p.dominant_type,
                    p.count,
                    p.type_instability,
                    p.null_rate,
                    path_w = max_path,
                    type_w = max_type,
                );
            }
            println!();

            println!("=== Fingerprints ===");
            println!("  Path set:    {}", output.fingerprints.path_set);
            println!("  Typed path:  {}", output.fingerprints.typed_path);
            println!("  Shape:       {}", output.fingerprints.shape);

            if !output.domain_hints.is_empty() {
                println!();
                println!("=== Domain Type Recognition ===");
                for hint in &output.domain_hints {
                    println!(
                        "  {} : \"{}\" -> {}",
                        hint.path, hint.value, hint.recognized_type
                    );
                }
            }
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
            if output.paths.is_empty() {
                println!("No per-path statistics (document has no scalar or array paths).");
                return Ok(());
            }

            let mut text = String::new();
            for sp in &output.paths {
                use std::fmt::Write;
                let _ = writeln!(text, "--- {} ---", sp.path);
                let _ = writeln!(
                    text,
                    "  Entropy: {:.4}  Normalized: {:.4}  Cardinality: {}  Count: {}",
                    sp.entropy, sp.normalized_entropy, sp.cardinality, sp.total_count
                );
                let _ = writeln!(text, "  Max rarity: {:.4} bits", sp.max_rarity);

                if let Some(ref ns) = sp.numeric {
                    let _ = writeln!(
                        text,
                        "  Numeric: min={:.4} max={:.4} mean={:.4} median={:.4} mad={:.4}",
                        ns.min, ns.max, ns.mean, ns.median, ns.mad
                    );
                    let _ = writeln!(
                        text,
                        "           p05={:.4} p25={:.4} p75={:.4} p95={:.4}",
                        ns.p05, ns.p25, ns.p75, ns.p95
                    );
                }

                if !sp.top_values.is_empty() {
                    let _ = writeln!(text, "  Top values:");
                    for tv in &sp.top_values {
                        let _ = writeln!(text, "    {:>6}x  {}", tv.count, tv.value);
                    }
                }
                text.push('\n');
            }
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

    let fmt = to_input_format(cli.input_format);
    let docs = vajra_core::input::load_documents(input, fmt).map_err(|e| anyhow::anyhow!("{e}"))?;

    let mut records: Vec<serde_json::Value> = Vec::new();
    for doc in &docs {
        match doc.value() {
            serde_json::Value::Array(arr) => {
                records.extend(arr.iter().cloned());
            }
            obj @ serde_json::Value::Object(_) => {
                records.push(obj.clone());
            }
            _ => {}
        }
    }

    if records.is_empty() {
        anyhow::bail!("no records found in input");
    }

    let resolved_time_field = match time_field {
        Some(tf) => tf.to_owned(),
        None => auto_detect_time_field(&records).ok_or_else(|| {
            anyhow::anyhow!("could not auto-detect time field; use --time-field to specify")
        })?,
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
            let mut text = String::new();
            use std::fmt::Write;

            let _ = writeln!(text, "=== Type Instabilities ===");
            if output.type_instabilities.is_empty() {
                let _ = writeln!(text, "  (none detected)");
            } else {
                for ti in &output.type_instabilities {
                    let _ = writeln!(
                        text,
                        "  {}: instability={:.4}, dominant={}",
                        ti.path, ti.instability, ti.dominant_type
                    );
                    let dist: Vec<String> = ti
                        .type_distribution
                        .iter()
                        .map(|(t, c)| format!("{t}={c}"))
                        .collect();
                    let _ = writeln!(text, "    types: {}", dist.join(", "));
                }
            }
            text.push('\n');

            let _ = writeln!(text, "=== Numeric Outliers ===");
            if output.numeric_outliers.is_empty() {
                let _ = writeln!(text, "  (none detected)");
            } else {
                for no in &output.numeric_outliers {
                    let _ = writeln!(
                        text,
                        "  {}: value={}, z_mad={:.4}, median={:.4}, mad={:.4}",
                        no.path, no.value, no.z_mad, no.median, no.mad
                    );
                }
            }
            text.push('\n');

            let _ = writeln!(text, "=== Rare Values ===");
            if output.rare_values.is_empty() {
                let _ = writeln!(text, "  (none detected)");
            } else {
                for rv in &output.rare_values {
                    let _ = writeln!(
                        text,
                        "  {} \"{}\": count={}, rarity={:.4} bits",
                        rv.path, rv.value, rv.count, rv.rarity_bits
                    );
                }
            }
            let text = maybe_redact(&text, cli);
            print!("{text}");
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// fingerprint
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct FingerprintOutput {
    path_set: String,
    typed_path: String,
    shape: String,
    repeated_motifs: Vec<MotifView>,
}

#[derive(Serialize)]
struct MotifView {
    hash: String,
    count: u64,
}

fn build_fingerprint_output(result: &FingerprintResult) -> FingerprintOutput {
    let mut repeated_motifs: Vec<MotifView> = result
        .subtree_frequencies
        .iter()
        .filter(|(_, &count)| count > 1)
        .map(|(h, &count)| MotifView {
            hash: hex(h),
            count,
        })
        .collect();
    // Sort by count descending for readability
    repeated_motifs.sort_by(|a, b| b.count.cmp(&a.count));

    FingerprintOutput {
        path_set: hex(&result.path_set),
        typed_path: hex(&result.typed_path),
        shape: hex(&result.shape),
        repeated_motifs,
    }
}

fn hex_slice(bytes: &[u8; 32]) -> String {
    hex(bytes)
}

fn cmd_fingerprint(input: &str, cli: &Cli) -> Result<()> {
    if cli.streaming {
        let doc = load_document(input, cli)?;
        let events = vajra_core::emit_events(doc.value());
        let mut acc = StreamingFingerprintAccumulator::new();
        acc.process_events(&events);
        let result = acc.finalize();

        match cli.format {
            Format::Json => {
                let json = serde_json::json!({
                    "path_set": hex_slice(&result.path_set),
                    "typed_path": hex_slice(&result.typed_path),
                    "shape": "(not available in streaming mode)",
                    "repeated_motifs": []
                });
                let json =
                    serde_json::to_string_pretty(&json).context("JSON serialization failed")?;
                println!("{json}");
            }
            Format::Text | Format::Markdown | Format::CompactAi => {
                println!("=== Structural Fingerprints (streaming) ===");
                println!("  Path set:    {}", hex_slice(&result.path_set));
                println!("  Typed path:  {}", hex_slice(&result.typed_path));
                println!("  Shape:       (not available in streaming mode)");
            }
        }
    } else {
        let doc = load_document(input, cli)?;
        let result = FingerprintAnalyzer
            .analyze(&doc)
            .context("fingerprint analysis failed")?;
        let output = build_fingerprint_output(&result);

        match cli.format {
            Format::Json => {
                let json =
                    serde_json::to_string_pretty(&output).context("JSON serialization failed")?;
                println!("{json}");
            }
            Format::Text | Format::Markdown | Format::CompactAi => {
                println!("=== Structural Fingerprints ===");
                println!("  Path set:    {}", output.path_set);
                println!("  Typed path:  {}", output.typed_path);
                println!("  Shape:       {}", output.shape);
                println!();

                println!("=== Repeated Motifs ===");
                if output.repeated_motifs.is_empty() {
                    println!("  (no repeated subtree shapes found)");
                } else {
                    println!("  {:<66}  {:>5}", "HASH", "COUNT");
                    for m in &output.repeated_motifs {
                        println!("  {:<66}  {:>5}", m.hash, m.count);
                    }
                }
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
                    }))
                    .collect::<Vec<_>>()),
            );
            let json_str =
                serde_json::to_string_pretty(&json).context("JSON serialization failed")?;
            println!("{json_str}");
        }
        Format::Text | Format::Markdown | Format::CompactAi => {
            println!("Drift Report: {baseline} -> {candidate}");
            println!(
                "Structural similarity: {:.4} (Jaccard)",
                report.structural_similarity
            );
            println!("Severity: {:?}", report.severity);
            println!();

            println!("Added paths ({}):", report.path_diff.added.len());
            if report.path_diff.added.is_empty() {
                println!("  (none)");
            } else {
                for p in &report.path_diff.added {
                    println!("  + {p}");
                }
            }
            println!();

            println!("Removed paths ({}):", report.path_diff.removed.len());
            if report.path_diff.removed.is_empty() {
                println!("  (none)");
            } else {
                for p in &report.path_diff.removed {
                    println!("  - {p}");
                }
            }
            println!();

            if !report.type_changes.is_empty() {
                println!("Type changes ({}):", report.type_changes.len());
                for tc in &report.type_changes {
                    println!("  {} : {} -> {}", tc.path, tc.from, tc.to);
                }
                println!();
            }

            if !report.distributional_drifts.is_empty() {
                println!(
                    "Distribution shifts ({}):",
                    report.distributional_drifts.len()
                );
                for dd in &report.distributional_drifts {
                    println!("  {} : {:?} = {:.4}", dd.path, dd.metric, dd.value);
                }
            }
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
        })).collect::<Vec<serde_json::Value>>(),
    })
}

#[allow(clippy::too_many_lines)]
fn cmd_population_drift(input: &str, group_by: &str, cli: &Cli) -> Result<()> {
    let raw_content = if input == "-" {
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)
            .context("failed to read from stdin")?;
        buf
    } else {
        std::fs::read_to_string(input).with_context(|| format!("failed to read file: {input}"))?
    };
    let raw_value: serde_json::Value = serde_json::from_str(&raw_content)
        .with_context(|| format!("failed to parse JSON from: {input}"))?;
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
            println!("Population Drift Report");
            println!("Group-by: {group_by}");
            println!("Groups: {group_count} ({pair_count} pairwise comparisons)");
            println!();
            println!("Group sizes:");
            for (name, size) in &group_sizes {
                println!("  {name}: {size} records");
            }
            println!();
            for (name_a, name_b, report) in &pairwise_drift {
                println!("--- {name_a} vs {name_b} ---");
                println!(
                    "  Structural similarity: {:.4} (Jaccard)",
                    report.structural_similarity
                );
                println!("  Severity: {:?}", report.severity);
                if !report.path_diff.added.is_empty() {
                    println!("  Added paths ({}):", report.path_diff.added.len());
                    for p in &report.path_diff.added {
                        println!("    + {p}");
                    }
                }
                if !report.path_diff.removed.is_empty() {
                    println!("  Removed paths ({}):", report.path_diff.removed.len());
                    for p in &report.path_diff.removed {
                        println!("    - {p}");
                    }
                }
                if !report.type_changes.is_empty() {
                    println!("  Type changes ({}):", report.type_changes.len());
                    for tc in &report.type_changes {
                        println!("    {} : {} -> {}", tc.path, tc.from, tc.to);
                    }
                }
                if !report.distributional_drifts.is_empty() {
                    println!(
                        "  Distribution shifts ({}):",
                        report.distributional_drifts.len()
                    );
                    for dd in &report.distributional_drifts {
                        println!("    {} : {:?} = {:.4}", dd.path, dd.metric, dd.value);
                    }
                }
                println!();
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// cluster
// ---------------------------------------------------------------------------

fn cmd_cluster(inputs: &[String], cli: &Cli) -> Result<()> {
    let mut docs = Vec::new();
    let mut names = Vec::new();

    for input in inputs {
        let path = Path::new(input);
        if path.is_dir() {
            let mut entries: Vec<_> = std::fs::read_dir(path)
                .with_context(|| format!("failed to read directory {input}"))?
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
                .collect();
            entries.sort_by_key(|e| e.path());
            for entry in entries {
                let p = entry.path();
                let doc =
                    parse_file(&p).with_context(|| format!("failed to parse {}", p.display()))?;
                names.push(p.display().to_string());
                docs.push(doc);
            }
        } else {
            let doc = parse_file(path).with_context(|| format!("failed to parse {input}"))?;
            names.push(input.clone());
            docs.push(doc);
        }
    }

    if docs.is_empty() {
        println!("No documents to cluster.");
        return Ok(());
    }

    let doc_refs: Vec<&Document> = docs.iter().collect();
    let result = cluster_documents(&doc_refs, 0);

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
            println!("=== Clustering {} documents ===", docs.len());
            println!("Similarity threshold: {:.2}", result.similarity_threshold);
            println!("Clusters found: {}", result.clusters.len());
            println!();

            for (i, members) in result.clusters.iter().enumerate() {
                println!("Cluster {} ({} members):", i, members.len());
                for &idx in members {
                    println!("  {}", names[idx]);
                }
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// invariants
// ---------------------------------------------------------------------------

fn cmd_invariants(input: &str, top_k: usize, cli: &Cli) -> Result<()> {
    let doc = load_document(input, cli)?;
    let relationships = vajra_stats::relationships::discover_relationships(&doc, top_k);

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
                    })
                })
                .collect();
            let json =
                serde_json::to_string_pretty(&rels_json).context("JSON serialization failed")?;
            println!("{json}");
        }
        Format::Text | Format::Markdown | Format::CompactAi => {
            println!("=== Cross-Field Relationships ===");
            if relationships.is_empty() {
                println!("  (no significant relationships discovered)");
                println!(
                    "  Hint: relationships require repeated objects (e.g., arrays of records)."
                );
            } else {
                println!(
                    "  {:<40}  {:<40}  {:>8}  {:>8}  {:>10}",
                    "FIELD X", "FIELD Y", "H(Y|X)", "PMI", "STRENGTH"
                );
                for r in &relationships {
                    println!(
                        "  {:<40}  {:<40}  {:>8.4}  {:>8.4}  {:>10.4}",
                        r.field_x,
                        r.field_y,
                        r.conditional_entropy,
                        r.mean_pmi,
                        r.relationship_strength,
                    );
                }
            }
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
    let files = batch::collect_json_files(&dir_path)?;

    if files.is_empty() {
        anyhow::bail!("no JSON files found in {directory}");
    }

    if !cli.quiet {
        eprintln!("Analyzing {} JSON files in parallel...", files.len());
    }

    let result = batch::analyze_batch(&files)?;

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
            }))
            .context("JSON serialization failed")?;
            println!("{json}");
        }
        Format::Text | Format::Markdown | Format::CompactAi => {
            println!(
                "=== Batch Analysis: {} documents ===",
                result.aggregate.total_documents
            );
            println!(
                "Total nodes across all documents: {}",
                result.aggregate.total_nodes
            );
            println!();

            // Per-document summary table
            println!("=== Per-Document Summary ===");
            let max_name = result
                .per_document
                .iter()
                .map(|d| d.file_name.len())
                .max()
                .unwrap_or(4)
                .max(4);
            println!(
                "  {:<name_w$}  {:>7}  {:>5}  {:>7}  {:>9}",
                "FILE",
                "NODES",
                "DEPTH",
                "PATHS",
                "ANOMALIES",
                name_w = max_name,
            );
            for d in &result.per_document {
                let anomaly_count = d.anomalies.numeric_outliers.len()
                    + d.anomalies.rare_values.len()
                    + d.anomalies.type_instabilities.len();
                let meta = d.document.metadata();
                println!(
                    "  {:<name_w$}  {:>7}  {:>5}  {:>7}  {:>9}",
                    d.file_name,
                    meta.total_nodes,
                    meta.max_depth,
                    meta.distinct_paths,
                    anomaly_count,
                    name_w = max_name,
                );
            }
            println!();

            // Common paths
            println!("=== Common Paths (>50% of documents) ===",);
            if result.aggregate.common_paths.is_empty() {
                println!("  (none)");
            } else {
                for path in &result.aggregate.common_paths {
                    let count = result
                        .aggregate
                        .path_frequency
                        .get(path)
                        .copied()
                        .unwrap_or(0);
                    println!("  {path}  ({count} docs)");
                }
            }
            println!();

            // Rare paths
            println!("=== Rare Paths (<10% of documents) ===",);
            if result.aggregate.rare_paths.is_empty() {
                println!("  (none)");
            } else {
                for path in &result.aggregate.rare_paths {
                    let count = result
                        .aggregate
                        .path_frequency
                        .get(path)
                        .copied()
                        .unwrap_or(0);
                    println!("  {path}  ({count} docs)");
                }
            }

            // Errors
            if !result.errors.is_empty() {
                println!();
                println!("=== Errors ({}) ===", result.errors.len());
                for (name, err) in &result.errors {
                    println!("  {name}: {err}");
                }
            }
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
    let raw = if input == "-" {
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)
            .context("failed to read stdin")?;
        buf
    } else {
        std::fs::read_to_string(input).with_context(|| format!("failed to read file: {input}"))?
    };
    let parsed: serde_json::Value =
        serde_json::from_str(&raw).with_context(|| format!("failed to parse JSON from {input}"))?;
    let records = match parsed.as_array() {
        Some(arr) => arr.clone(),
        None => {
            anyhow::bail!("cascade command expects a JSON array of records");
        }
    };
    let response_values: Vec<String> = response_values_str
        .split(',')
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();
    let config = vajra_cascade::CascadeConfig {
        entity_field: entity_field.to_owned(),
        time_field: time_field.to_owned(),
        event_field: event_field.to_owned(),
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
    let hj: Vec<serde_json::Value> = r.hot_entities.iter().map(|h| serde_json::json!({"entity":h.entity,"total":h.total,"cascades":h.cascades,"cascade_ratio":h.cascade_ratio})).collect();
    serde_json::json!({"cascade_rate":r.cascade_rate,"self_fix_rate":r.self_fix_rate,"total_events":r.total_events,"total_cascades":r.cascades.len(),"hot_entities":hj,"cascades":cj})
}
fn cascade_text(r: &vajra_cascade::CascadeResult) -> String {
    use std::fmt::Write;
    let mut o = String::new();
    let _ = writeln!(o, "=== Cascade Analysis ===");
    let _ = writeln!(o, "  Total events:   {}", r.total_events);
    let _ = writeln!(o, "  Total cascades: {}", r.cascades.len());
    let _ = writeln!(o, "  Cascade rate:   {:.3}", r.cascade_rate);
    let _ = writeln!(o, "  Self-fix rate:  {:.3}", r.self_fix_rate);
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
        let _ = writeln!(
            o,
            "  {:<ew$}  {:>5}  {:>8}  {:>6}",
            "ENTITY",
            "TOTAL",
            "CASCADES",
            "RATIO",
            ew = ew
        );
        for h in &r.hot_entities {
            let _ = writeln!(
                o,
                "  {:<ew$}  {:>5}  {:>8}  {:>6.3}",
                h.entity,
                h.total,
                h.cascades,
                h.cascade_ratio,
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
    let _ = writeln!(o, "| Self-fix rate | {:.3} |", r.self_fix_rate);
    let _ = writeln!(o);
    if !r.hot_entities.is_empty() {
        let _ = writeln!(o, "## Hot Entities\n");
        let _ = writeln!(o, "| Entity | Total | Cascades | Ratio |");
        let _ = writeln!(o, "|--------|-------|----------|-------|");
        for h in &r.hot_entities {
            let _ = writeln!(
                o,
                "| {} | {} | {} | {:.3} |",
                h.entity, h.total, h.cascades, h.cascade_ratio
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

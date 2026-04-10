<p align="center">
  <img src="logo.svg" alt="Vajra" width="200">
</p>

<h1 align="center">Vajra</h1>

<p align="center">
  <strong>Break noise. Preserve truth.</strong>
</p>

<p align="center">
  <a href="https://github.com/copyleftdev/vajra/actions/workflows/ci.yml"><img src="https://github.com/copyleftdev/vajra/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://copyleftdev.github.io/vajra"><img src="https://img.shields.io/badge/docs-live-gold" alt="Docs"></a>
  <a href="https://github.com/copyleftdev/vajra/actions"><img src="https://img.shields.io/badge/tests-1075%20passed-brightgreen" alt="Tests"></a>
  <a href="https://github.com/copyleftdev/vajra"><img src="https://img.shields.io/badge/crates-17-blue" alt="Crates"></a>
  <a href="LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT%2FApache--2.0-orange" alt="License"></a>
</p>

---

A high-performance Rust CLI and library that analyzes arbitrary structured data — JSON, YAML, CSV, NDJSON, Markdown, PDF, git repos, CPU profiles, strace logs — extracts structural and semantic signal, detects anomalies and drift, discovers temporal cause-effect chains, and emits compact deterministic essences optimized for humans, auditors, and AI pipelines.

## Install

```bash
cargo install vajra-cli
```

Or from source:

```bash
git clone https://github.com/copyleftdev/vajra
cd vajra
cargo build --release
```

## 30 Seconds to Value

```bash
# Structural analysis
vajra inspect data.json

# Concern-oriented essence for non-technical staff
vajra essence data.json --profile staff

# Anomaly detection
vajra anomalies data.json

# Schema drift between versions
vajra drift v1.json v2.json

# Compact output for LLM consumption
vajra essence data.json --profile ai --format compact-ai --budget 500

# Query with analysis functions
vajra query data.json 'entropy($.claims[*].status) > 0.5'

# Batch analysis with parallel processing
vajra batch data_directory/

# Cluster similar documents
vajra cluster batch/*.json

# Analyze a git repo directly
vajra stats /path/to/repo --input-format git

# Temporal cause-effect chains
vajra cascade commits.json --entity-field '$.file' --time-field '$.date' --event-field '$.intent'

# Time-series windowed stats with trend detection
vajra stats data.json --window month --time-field '$.date'

# Population-level drift comparison
vajra drift data.json --group-by '$.team'

# Semantic path labels for source code
vajra inspect src/main.rs --input-format source --lang rust --semantic-paths
```

## What It Does

Feed Vajra any structured data. It returns **shape, signal, anomalies, and truth**.

| Command | Purpose |
|---------|---------|
| `inspect` | Full structural analysis — paths, types, fingerprints, domain recognition |
| `stats` | Statistical summary — entropy, frequency, numeric distributions |
| `anomalies` | MAD-based outliers, rarity scoring, type instability |
| `fingerprint` | BLAKE3 structural hashes, Merkle motifs |
| `essence` | Concern-oriented reduction — 7 profiles, token budgets, compact-AI |
| `drift` | Schema drift — JSD, Wasserstein, severity classification |
| `cluster` | MinHash + LSH similarity clustering |
| `invariants` | Cross-field relationships — conditional entropy, PMI |
| `query` | Path expressions with analysis functions |
| `cascade` | Temporal cause-effect chain detection across events |
| `batch` | Parallel batch analysis across directories |
| `profiles` | List available profiles |

## Input Formats

| Format | Extensions | Auto-Detected |
|--------|-----------|---------------|
| JSON | `.json` | Yes |
| NDJSON | `.ndjson`, `.jsonl` | Yes |
| YAML | `.yaml`, `.yml` | Yes |
| CSV | `.csv` | Yes |
| TSV | `.tsv` | Yes |
| Markdown | `.md`, `.markdown` | Yes |
| PDF | `.pdf` | Yes |
| Gzip | `.gz`, `.json.gz` | Yes (magic bytes) |
| Zstd | `.zst`, `.zstd` | Yes (magic bytes) |
| Git | `--input-format git` | No (explicit) |
| V8 CPU Profile | `.cpuprofile` | Yes |
| strace | `--input-format strace` | No (explicit) |
| HTTP | `http://`, `https://` | Yes |
| Stdin | `-` | Yes |

## Profiles

| Profile | Emphasizes | Audience |
|---------|-----------|----------|
| `staff` | Anomalies, structural coverage | Non-technical operations |
| `engineer` | Type instability, balanced | Developers |
| `auditor` | Completeness, traceability | Compliance |
| `ai` | Entropy, coverage, compact output | LLM pipelines |
| `fraud` | Outliers, rarity, suspicious patterns | Investigation |
| `health` | Velocity, staleness, bus factor | Project health assessment |
| Custom | Your weights, your rules | TOML configuration |

## Domain Plugins

| Plugin | Recognizers | Covers |
|--------|-------------|--------|
| `vajra-domain-med` | ICD-10, CPT, NPI, NDC | Medical/EDI |
| `vajra-domain-sec` | CVE, MITRE ATT&CK, IPs, hashes, JWT | Security |
| `vajra-domain-devops` | K8s, Docker, Terraform, ARN, semver | DevOps/Infra |
| `vajra-domain-github` | PRs, issues, reviews, releases, commits, labels, milestones, deployments, check runs, actions | GitHub |
| `vajra-domain-source` | Naming conventions, file paths | Source code |
| `vajra-domain-encoding` | Base64, hex, URL, PEM, layers | Encoding |

## Global Options

| Flag | Applies To | Purpose |
|------|-----------|---------|
| `--window month/week/day` | `stats` | Temporal windowing with trend detection |
| `--time-field '$.path'` | `stats`, `cascade` | JSONPath to the timestamp field |
| `--group-by '$.path'` | `drift` | Partition records by field for population-level comparison |
| `--git-limit N` | `--input-format git` | Cap number of git commits ingested |
| `--git-branch name` | `--input-format git` | Read from a specific branch |
| `--semantic-paths` | `inspect` | Map tree-sitter AST paths to human-readable labels (9 languages) |

## The Engine

Every algorithm was chosen against three gates: **works at any scale** (O(n) or O(n log n)), **battle-tested** (published, peer-reviewed), and **deterministic** (same input = same output, always).

| Algorithm | Purpose | Provenance |
|-----------|---------|-----------|
| BLAKE3 | All hashing and fingerprinting | O'Connor et al. 2020 |
| Merkle subtree hashing | Structural identity + motif detection | O(n), motifs for free |
| Shannon entropy | Value diversity measurement | Universal signal primitive |
| MAD | Robust outlier detection | 50% breakdown point |
| DDSketch | Streaming quantile estimation | Masson et al. 2019 (Datadog) |
| Count-Min Sketch | Streaming frequency estimation | Cormode & Muthukrishnan 2005 |
| Jensen-Shannon Divergence | Distribution drift measurement | Endres & Schindelin 2003 |
| MinHash + LSH | Scalable similarity clustering | Broder 1997, Indyk & Motwani 1998 |

## Testing

```
1075 tests, 0 failures

43 property tests — every mathematical invariant encoded
18 chaos tests — pathological inputs, no panics
11 differential tests — exact vs streaming equivalence
10 determinism tests — 10-run byte-identical verification
 6 golden tests — regression gate against 31-file corpus
 8 criterion benchmark suites
```

```bash
cargo test --workspace                    # all tests
cargo test -- prop_                       # property tests only
cargo test -- chaos                       # chaos tests only
cargo test -- determinism                 # determinism verification
cargo test -- golden                      # golden regression tests
cargo bench --workspace                   # benchmarks
```

## Architecture

```
vajra/
├── vajra-types        # Shared types, traits, scoring
├── vajra-core         # Parsing, paths, canonicalization, streaming, formats, redaction
├── vajra-fingerprint  # BLAKE3, Merkle, MinHash, LSH, clustering
├── vajra-stats        # Entropy, MAD, DDSketch, CMS, Benford, temporal, relationships
├── vajra-anomaly      # Outlier detection, rarity, type instability
├── vajra-drift        # JSD, Wasserstein, path diff, severity
├── vajra-essence      # Profiles, scoring, rendering, TOML config, chunking
├── vajra-query        # Expression parser, analysis functions
├── vajra-cascade      # Temporal cause-effect chain detection
├── vajra-domain-med   # Medical/EDI plugin (ICD-10, CPT, NPI, NDC)
├── vajra-domain-sec   # Security plugin (CVE, MITRE ATT&CK, IPs, hashes, JWT)
├── vajra-domain-devops # DevOps plugin (K8s, Docker, Terraform, ARN, semver)
├── vajra-domain-github # GitHub plugin (PRs, issues, reviews, releases, actions)
├── vajra-source       # Source code parsing via tree-sitter (9 languages)
├── vajra-domain-source # Source code recognizers (naming conventions, paths)
├── vajra-domain-encoding # Encoding detection (Base64, hex, URL, PEM, layers)
├── vajra-motif        # (reserved)
├── vajra-cli          # CLI commands, batch processing
└── docs/              # mdbook documentation site
```

## Documentation

Full documentation with GSAP-powered kinetic showcase:

```bash
cd docs && mdbook serve --open
```

## License

Licensed under either of:

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.

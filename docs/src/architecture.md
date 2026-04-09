# Architecture

Vajra is a Rust workspace of 16 crates. Each crate has a single responsibility. Dependencies flow downward. Nothing cycles.

---

## The 16-Crate Workspace

```text
vajra/
├── vajra-types/          Shared types, traits, contracts
├── vajra-core/           Parsing, traversal, canonicalization, path extraction
├── vajra-fingerprint/    BLAKE3 hashing, Merkle trees, MinHash, SimHash, LSH
├── vajra-stats/          CMS, Space-Saving, DDSketch, MAD, entropy, frequency
├── vajra-anomaly/        Outlier scoring, instability, rarity, structural anomaly
├── vajra-drift/          JSD, Wasserstein, path diff, drift classification
├── vajra-motif/          Motif counting, near-motif grouping, motif compression
├── vajra-essence/        Profiles, scoring, ranking, rendering, templates
├── vajra-query/          Expression parsing, path filtering, analysis functions
├── vajra-source/         Source code parsing via tree-sitter (Rust, Python, Go, JS, +5)
├── vajra-cli/            CLI argument parsing, command dispatch, output formatting
├── vajra-domain-med/     Medical/EDI type recognizers (ICD-10, CPT, NPI, NDC, HCPCS)
├── vajra-domain-sec/     Security type recognizers (CVE, MITRE ATT&CK, IPs, hashes, JWT)
├── vajra-domain-devops/  DevOps type recognizers (K8s, Docker, Terraform, ARN, semver)
├── vajra-domain-source/  Source code recognizers (naming conventions, import paths)
└── Cargo.toml            Workspace root
```

---

## Dependency Graph

```text
                    vajra-types
                   /     |     \
                  /      |      \
           vajra-core    |    vajra-domain-{med,sec,devops}
            /    \       |       /
           /      \      |      /
  vajra-fingerprint  vajra-stats
       |          \   /   |
       |           \ /    |
       |      vajra-anomaly
       |           |
       |      vajra-drift
       |           |
       |      vajra-motif
       |         / |
       |        /  |
       vajra-essence
            |
       vajra-query
            |
       vajra-cli
```

**Root crates (no internal dependencies):**
- `vajra-types` — shared types, trait definitions, result contracts
- `vajra-core` depends only on `vajra-types`

**Leaf crate (depends on everything):**
- `vajra-cli` — the binary. It orchestrates all other crates.

---

## Crate Responsibilities

### vajra-types

The foundation. Shared types that every crate depends on.

- `Document` — the parsed document model (value tree + path trie + metadata)
- `WildcardPath` — normalized path representation with `[*]` array indices
- `PathTrie` — trie data structure for efficient path storage and lookup
- `FeatureStore` — per-path feature vectors
- `JsonType` — enum of JSON types (object, array, string, number, boolean, null)
- Core traits: `Analyzer`, `StreamAnalyzer`, `FeatureExtractor`, `ConcernProfile`, `Fingerprinter`, `DriftDetector`

```rust
pub trait Analyzer {
    type Output;
    fn analyze(&self, doc: &Document) -> Result<Self::Output>;
}

pub trait StreamAnalyzer {
    type Accumulator: Default;
    type Output;
    fn on_event(&self, event: &JsonEvent, acc: &mut Self::Accumulator) -> Result<()>;
    fn finalize(&self, acc: Self::Accumulator) -> Result<Self::Output>;
}
```

### vajra-core

Parsing, traversal, and the foundational index.

- `simd-json` integration for DOM-mode parsing
- Multi-format input support (JSON, NDJSON, YAML, CSV, TSV, Markdown, PDF)
- Compression handling (gzip, zstd)
- HTTP URL fetching
- RFC 8785 canonicalization
- DFS path extraction and path trie construction
- Unicode NFC normalization
- Redaction engine (`vajra_core::redact`)
- Input hardening (depth limits, string length limits, size limits)

### vajra-fingerprint

Structural identity.

- BLAKE3 path set fingerprint
- BLAKE3 typed path fingerprint
- Merkle subtree hashing (shape fingerprint)
- MinHash signature computation (k = 128)
- SimHash for near-motif detection
- LSH bucketing for scalable similarity search
- Cluster computation from LSH candidates
- `StreamingFingerprintAccumulator` for streaming mode

### vajra-stats

The statistical engine.

- Shannon entropy (exact and CMS-approximate)
- Normalized entropy
- Count-Min Sketch with conservative update
- Space-Saving top-k
- DDSketch for streaming quantiles
- MAD and modified z-scores
- Frequency analysis (key, path, value)
- Missingness profiling (null rate, absent rate, empty rate)
- Numeric distribution summary (min, max, mean, median, percentiles)
- Co-occurrence and PMI computation
- Benford's Law leading digit analysis
- `StreamingStatsAccumulator` for streaming mode

### vajra-anomaly

Deviation detection.

- Numeric outlier detection (MAD-based z-scores)
- Rarity scoring (self-information)
- Structural deviation detection (Jaccard distance from mode)
- Type instability detection
- Composite anomaly scoring
- Anomaly report generation

### vajra-drift

Change detection between documents.

- Path set symmetric difference (structural drift)
- Type drift detection
- Jensen-Shannon Divergence for distributional drift
- 1D Wasserstein distance for numeric drift magnitude
- Drift classification (additive, subtractive, type-mutative, distributional, cardinality-shift, null-rate-shift)
- Severity scoring with profile-dependent weights

### vajra-motif

Repeated structure analysis.

- Motif counting from Merkle subtree hash frequencies
- Near-motif grouping via SimHash Hamming distance
- Motif ranking (frequency x subtree size)
- Motif compression for essence generation
- Array morphology analysis (homogeneity, uniqueness, shape diversity)

### vajra-essence

The rendering engine.

- Built-in profiles: `StaffProfile`, `EngineerProfile`, `AuditorProfile`, `AiProfile`, `FraudProfile`
- Custom profile loading from TOML
- Six-dimensional scoring model
- Candidate collection and ranking
- Token budget enforcement (greedy knapsack)
- Text, JSON, Markdown, and compact-AI renderers
- Motif collapsing
- `--explain` score decomposition
- Provenance metadata attachment

### vajra-query

Path-based query engine.

- Expression parser for path filters and analysis functions
- `entropy(path)`, `rarity(path, value)`, `instability(path)`, `null_rate(path)`, `stats(path)`, `anomaly_score(path)`, `motif(path)`
- Conditional expression evaluation (e.g., `entropy($.status) > 0.5`)
- Integration with stats, anomaly, and motif analyzers

### vajra-cli

The command-line interface.

- Clap-based argument parsing
- Command dispatch (`inspect`, `stats`, `anomalies`, `fingerprint`, `essence`, `drift`, `cluster`, `invariants`, `query`, `batch`, `profiles`)
- Output format rendering (text, JSON, Markdown, compact-AI)
- Redaction integration
- Streaming mode selection
- Custom profile loading
- Batch processing with Rayon parallelism

### vajra-domain-med

The medical/EDI domain plugin.

- ICD-10-CM and ICD-10-PCS pattern recognizers
- CPT and HCPCS code recognizers
- NDC (National Drug Code) recognizer
- NPI (National Provider Identifier) recognizer with Luhn check
- Denial reason code recognizer (CO, PR, OA, PI, CR)
- Claim, service line, patient, provider, and adjudication relationship hints
- Implements `VajraPlugin` trait

---

## Core Traits

The trait system is the architectural backbone. Each trait is small, composable, and independently testable.

| Trait | Defined In | Purpose |
|---|---|---|
| `Analyzer` | vajra-types | DOM-mode analysis: document in, typed output out |
| `StreamAnalyzer` | vajra-types | Streaming analysis: events in, accumulator maintained, output finalized |
| `FeatureExtractor` | vajra-types | Extract features into the shared feature store |
| `ConcernProfile` | vajra-types | Define scoring weights and rendering behavior |
| `Fingerprinter` | vajra-types | Compute structural fingerprints |
| `DriftDetector` | vajra-types | Compare two analyzed documents for drift |
| `VajraPlugin` | vajra-types | Plugin extension point |
| `TypeRecognizer` | vajra-types | Domain-specific value type recognition |

---

## Navigating the Codebase

**"I want to understand how parsing works."**  
Start at `vajra-core/src/`. The `input` module handles multi-format loading. The `parse` module handles JSON parsing. The `canon` module handles canonicalization.

**"I want to understand the statistical engine."**  
Start at `vajra-stats/src/`. Each statistical primitive has its own module. `StatsAnalyzer` composes them.

**"I want to add a new profile."**  
Look at `vajra-essence/src/`. The built-in profiles (`StaffProfile`, `EngineerProfile`, etc.) implement `ConcernProfile`. Follow the pattern.

**"I want to add a domain plugin."**  
Look at `vajra-domain-med/` as the reference implementation. Implement `VajraPlugin` in a new crate.

**"I want to add a new command."**  
Start at `vajra-cli/src/main.rs`. Each command is a function (`cmd_inspect`, `cmd_stats`, etc.). Add a new variant to the `Command` enum and implement the handler.

**"I want to understand how essences are built."**  
Start at `vajra-essence/src/`. The `EssenceBuilder` collects observations from stats, anomaly, and motif analyzers, scores them, and renders the result.

---

## Build and Run

```bash
# Build the entire workspace
cargo build --release

# Run tests across all crates
cargo test --workspace

# Run the CLI
./target/release/vajra inspect claim.json

# Run benchmarks
cargo bench --workspace
```

---

## External Dependencies

| Dependency | Version | Purpose |
|---|---|---|
| `serde` / `serde_json` | 1.x | Serialization |
| `serde_yaml` | 0.9 | YAML input format |
| `csv` | 1.x | CSV/TSV input format |
| `blake3` | 1.x | All hashing |
| `clap` | 4.x | CLI argument parsing |
| `ryu` | 1.x | Deterministic float formatting |
| `unicode-normalization` | 0.1 | Unicode NFC normalization |
| `toml` | 0.8 | Config and profile loading |
| `regex` | 1.x | Pattern matching (redaction, type recognition) |
| `rayon` | 1.x | Parallel batch processing |
| `thiserror` / `anyhow` | 2.x / 1.x | Error handling |
| `flate2` | 1.x | Gzip decompression |
| `zstd` | 0.13 | Zstd decompression |
| `pulldown-cmark` | 0.12 | Markdown input parsing |
| `pdf-extract` | 0.10 | PDF text extraction |
| `ureq` | 2.x | HTTP URL fetching |
| `proptest` | 1.x | Property-based testing |
| `criterion` | 0.5 | Benchmarks |

All dependencies are Rust-native. No C bindings, no FFI, no system library requirements beyond a standard Rust toolchain.

---

## Lints

The workspace enforces strict Clippy lints:

```toml
[workspace.lints.clippy]
pedantic = { level = "warn", priority = -1 }
nursery = { level = "warn", priority = -1 }
unwrap_used = "deny"    # No .unwrap() — use Result
expect_used = "deny"    # No .expect() — use Result
panic = "deny"          # No panic!() — ever
```

No panics on any input. No unwraps. No expects. Every error path returns a `Result`.

# Vajra — Development Guide

**Break noise. Preserve truth.**

Vajra is a deterministic semantic reduction and anomaly analysis engine for JSON, built in Rust. It works on any JSON regardless of size or complexity. Read `prd.md` for the full specification.

---

## The Rules

1. **Every algorithm must work at any scale.** 1KB, 1GB, 100GB. If it doesn't stream, it doesn't ship.
2. **Determinism is sacred.** Same input + same config = same output. Always. Use `BTreeMap` for any externally-visible ordering. Seed all randomized algorithms. Fixed traversal order everywhere.
3. **Strong primitives only.** No speculative ML, no O(n^2) algorithms, no techniques requiring tuning. Published, peer-reviewed, deployed at scale — or it doesn't belong.
4. **Honest inference.** Every heuristic is labeled. Every score is decomposable. Never silently assert a guess as truth.
5. **Find bugs, fix bugs.** Property tests, fuzz tests, chaos, mutation testing. Finding an issue is a win, not a problem. You are rewarded for breaking things and fixing them.
6. **No dead code, no speculative abstractions.** Three similar lines beat a premature abstraction.
7. **Zero warnings.** `cargo fmt` runs automatically via hook. Clippy at pedantic level with `-D warnings`.

---

## Workspace Structure

```
vajra/
├── vajra-core/          # parsing, traversal, canonicalization, path extraction
├── vajra-types/         # shared types, feature vectors, result contracts
├── vajra-fingerprint/   # BLAKE3, Merkle subtree hashing, MinHash, SimHash, LSH
├── vajra-stats/         # CMS, Space-Saving, DDSketch, MAD, entropy, frequency, PMI
├── vajra-anomaly/       # MAD outliers, rarity scoring, type instability, structural anomaly
├── vajra-drift/         # JSD, 1D Wasserstein, path diff, drift classification
├── vajra-motif/         # Merkle motif counting, SimHash near-motif grouping
├── vajra-essence/       # concern profiles, scoring, ranking, rendering, templates
├── vajra-query/         # path expressions, analysis functions
├── vajra-cli/           # CLI commands, argument parsing, output formatting
├── vajra-domain-med/    # optional medical/EDI plugin
├── vajra-domain-sec/    # optional security plugin (CVE, MITRE, IPs, hashes, JWT)
├── vajra-domain-devops/ # optional DevOps plugin (K8s, Docker, Terraform, ARN, semver)
├── vajra-source/        # source code parsing via tree-sitter (9 languages)
└── vajra-domain-source/ # source code recognizers (naming conventions, paths)
```

Dependencies flow downward. `vajra-core` and `vajra-types` depend on nothing internal. `vajra-cli` depends on everything.

---

## Commands

| Command | Purpose |
|---------|---------|
| `/quality-gate` | Full quality gate: fmt, clippy, test, property tests, benchmarks |
| `/chaos [crate]` | Adversarial testing — try to break a crate |
| `/review [module]` | Dream team multi-lens code review |
| `/bench [target]` | Run benchmarks, check for regressions |
| `/design [crate]` | Architect a crate before implementation |
| `/dreamteam <skill>` | Invoke a specific dream team expert |

## Agents

| Agent | Trigger | Purpose |
|-------|---------|---------|
| **chaos** (red) | "break it", "fuzz", "chaos" | Property tests + fuzz targets + adversarial inputs |
| **review** (blue) | "review", "dream team" | 10-lens expert code review |
| **test-runner** (green) | "run tests", "quality gate" | Full quality gate execution |
| **architect** (cyan) | "design", "architecture" | Crate design and trait decisions |

## Skills (Auto-Loaded by Context)

Skills activate automatically when the work matches. No need to invoke them — Claude loads the relevant skill based on what you're doing.

| Skill | Triggers On |
|-------|-------------|
| **vajra-analyzer** | "add analyzer", "implement analyzer", "wire into pipeline" — how to build Analyzer/StreamAnalyzer |
| **vajra-testing** | "write tests", "property tests", "fuzz this" — full testing methodology with invariant catalog |
| **vajra-algorithm** | "add algorithm", "implement primitive", "implement DDSketch" — the three gates + approved primitives |
| **vajra-streaming** | "handle large files", "streaming mode", "bounded memory" — two-pass architecture, accumulator design |
| **vajra-essence-profile** | "create profile", "customize essence", "tune weights" — scoring, rendering, TOML config |
| **vajra-plugin-dev** | "create plugin", "add domain plugin", "extend Vajra" — VajraPlugin trait, isolation, testing |
| **vajra-domain-sec** | "security plugin", "CVE detection", "MITRE ATT&CK types", "detect JWT" — security type recognizers, hints, profiles |
| **vajra-domain-devops** | "devops plugin", "K8s recognizers", "detect container IDs", "Terraform" — infrastructure type recognizers, hints, profiles |
| **vajra-cli-command** | "add CLI command", "implement inspect", "add subcommand" — clap patterns, output modes, error style |

## Hooks

| Event | Trigger | Action |
|-------|---------|--------|
| PostToolUse | Edit/Write of .rs files | `cargo fmt` on the edited file |
| Stop | Agent stopping | `cargo check` + `cargo clippy` — blocks on failure |

---

## The Dream Team

16 skills, each channeling a master. Invoke via `/dreamteam <name>` or use the skill directly.

### Core Rust
- **`/tolnay`** — serde, parsing, error types. *Owns: vajra-core parsing, error handling.*
- **`/gjengset`** — API design, type-state, testing. *Owns: crate APIs, test architecture.*
- **`/matsakis`** — ownership, lifetimes, borrow checker. *Owns: Document type, path trie, zero-copy.*
- **`/turon`** — public API, async, streaming. *Owns: StreamAnalyzer trait, library ergonomics.*
- **`/bos`** — concurrency, atomics. *Owns: parallel batch, sketch merging.*
- **`/lerche`** — streaming, backpressure, Tokio. *Owns: streaming parser architecture.*

### Systems
- **`/cantrill`** — observability, debugging. *Owns: pipeline instrumentation, --explain output.*
- **`/muratori`** — performance, measurement. *Owns: latency targets, hot path optimization.*
- **`/lampson`** — abstractions, interfaces. *Owns: trait design, plugin interface, crate boundaries.*

### Data & Math
- **`/kleppmann`** — data systems, streaming, sketches. *Owns: CMS, DDSketch, streaming architecture.*
- **`/rodriguez`** — graph data, traversal. *Owns: JSON-as-graph, motif mining, structural similarity.*
- **`/pavlo`** — data layout, memory, cache. *Owns: feature store layout, path trie memory.*

### CLI
- **`/hashimoto`** — CLI design, error messages. *Owns: vajra-cli command grammar, output formatting.*

### Testing & Verification
- **`/property-based`** — property tests, invariants, shrinking. *Owns: property tests for every module.*
- **`/deterministic-simulation`** — DST, nondeterminism hunting. *Owns: determinism test suite.*
- **`/beck-tdd`** — red-green-refactor. *Owns: development methodology.*

---

## Agent Workflows

### Parallel Crate Development
When building independent crates, spawn agents in worktree isolation:
```
Agent(isolation: worktree) → vajra-stats
Agent(isolation: worktree) → vajra-fingerprint
Agent(isolation: worktree) → vajra-anomaly
```

### Implementation Cycle
1. `/design <crate>` — architect the crate
2. `/dreamteam tdd` — start with tests
3. Implement (hooks auto-format, stop hook checks clippy)
4. `/chaos <crate>` — try to break it
5. `/review <crate>` — dream team review
6. `/quality-gate` — full gate before merge

---

## Algorithm Quick Reference

| Primitive | Crate | Why This One |
|---|---|---|
| BLAKE3 | fingerprint | fastest crypto hash, Rust-native (O'Connor 2020) |
| Merkle subtree hash | fingerprint | O(n), motifs free |
| MinHash (b-bit) | fingerprint | 32x memory savings (Li & Konig 2011) |
| LSH (banded) | fingerprint | O(n) indexing, sublinear queries |
| SimHash | motif | Hamming ~ cosine (Charikar 2002) |
| Shannon entropy | stats | universal signal, O(n) |
| CMS (conservative) | stats | O(1) update, bounded memory (Estan & Varghese 2002) |
| Space-Saving | stats | O(k) memory top-k (Metwally 2005) |
| DDSketch | stats | relative error quantiles, mergeable (Masson 2019) |
| MAD | stats, anomaly | 50% breakdown point (Iglewicz & Hoaglin 1993) |
| JSD (metric) | drift | symmetric, bounded, proper metric (Endres & Schindelin 2003) |
| 1D Wasserstein | drift | interpretable earth mover's, O(n log n) |
| PMI | stats | information-theoretic association |
| Conditional entropy | stats | functional dependency detection |
| Benford's Law | stats | leading digit forensics (Nigrini 1996) |
| RFC 8785 (JCS) | core | IETF canonical JSON |
| DFA bank | core | O(m) type inference, no backtracking |

## Dependency Stack

| Need | Crate |
|------|-------|
| JSON (DOM) | `simd-json` |
| JSON (stream) | `serde_json` streaming / `json-event-parser` |
| Hashing | `blake3` |
| Serialization | `serde` + `serde_json` |
| CLI | `clap` (derive) |
| Errors (lib) | `thiserror` |
| Errors (CLI) | `anyhow` |
| Parallelism | `rayon` |
| Property tests | `proptest` |
| Fuzzing | `cargo-fuzz` |
| Benchmarks | `criterion` |
| Float formatting | `ryu` |
| Unicode | `unicode-normalization` |
| Mutation tests | `cargo-mutants` |

---

## Hard Constraints

- No `unsafe` blocks.
- No `HashMap` where output order matters. Use `BTreeMap`.
- No `f64` equality. Use relative tolerance.
- No `println!` for output. All user-facing output through the rendering system.
- No `#[allow(clippy::...)]` without a comment explaining why the lint is wrong here.
- No dependencies without checking: maintained? reasonable dep tree? `no_std` compatible where needed?
- No `unwrap()`, `expect()`, or `panic!()`. Ever. The clippy deny lints enforce this.
- All errors must include enough context to diagnose without a debugger.
- All tests must be order-independent.

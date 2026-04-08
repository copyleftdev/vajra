# The Engine

Vajra processes structured data through a six-layer pipeline. Each layer depends on the one before it. Each layer's outputs are independently useful. The pipeline can exit early at any layer depending on the command.

---

## The Six Layers

```text
Raw Input
  -> [1] Parse + Normalize
  -> [2] Structural Analysis
  -> [3] Statistical Analysis
  -> [4] Semantic Lifting
  -> [5] Concern-Oriented Scoring
  -> [6] Deterministic Essence Rendering
```

---

## Layer 1: Parse + Normalize

**Responsibility:** Take raw bytes and produce a traversable document model.

**What happens:**

1. **Format detection.** Auto-detect or apply `--input-format` override. See [Input Formats](./formats.md).
2. **Decompression.** Gzip and Zstd payloads are decompressed transparently.
3. **Parsing.** JSON via `simd-json` (DOM mode) or SAX-style streaming. YAML, CSV, TSV, Markdown, PDF converted to JSON-equivalent internal representation.
4. **Canonicalization.** RFC 8785 (JSON Canonicalization Scheme) applied: lexicographic key ordering, deterministic number formatting, Unicode NFC normalization.
5. **Input hardening.** Maximum nesting depth enforced (default 256). Maximum string length enforced. Malformed input produces clean errors with byte offset locations.

**Output:** A `Document` — the parsed value tree plus metadata (node count, depth, raw size, content hash).

**Complexity:** O(n) time. O(n) memory in DOM mode, O(1) in streaming.

**Commands that stop here:** None. Every command needs at least a parsed document.

---

## Layer 2: Structural Analysis

**Responsibility:** Extract the structural skeleton — every path, every type, every parent-child relationship.

**What happens:**

1. **Path extraction.** DFS traversal computes full JSONPath for every node. Array indices normalized to `[*]` for wildcard paths.
2. **Path trie construction.** Wildcard paths stored in a trie. Each trie node holds aggregated metadata: count, type distribution, depth, parent type, sibling count.
3. **Fingerprinting.** BLAKE3 path set hash, typed path hash, and Merkle subtree hashes computed in a single bottom-up traversal.
4. **Motif detection.** Subtree hashes that appear more than once identify repeated structural patterns. Ranked by frequency times subtree size.
5. **Array morphology.** Per-array cardinality distribution, type homogeneity, element uniqueness, nested shape diversity.

**Output:** Path trie, fingerprints, motif index, array morphology profiles.

**Complexity:** O(n) time, O(p) memory where p = distinct wildcard paths.

**Commands that exit here:** `inspect`, `fingerprint`.

---

## Layer 3: Statistical Analysis

**Responsibility:** Quantify the distribution of every observable quantity in the document.

**What happens:**

1. **Frequency analysis.** Per-path value frequencies via exact counting (or Count-Min Sketch in streaming mode). Top-k values via Space-Saving.
2. **Entropy computation.** Shannon entropy and normalized entropy per path. The most informative universal signal in the system.
3. **Missingness profiling.** Null rate, absent rate, empty rate, type instability rate per path. Identifies quasi-required fields and suspicious omissions.
4. **Numeric distributions.** Min, max, mean, median, MAD, percentiles via DDSketch. Skewness proxy. Heavy-tail indicator.
5. **Co-occurrence.** Pointwise Mutual Information (PMI) between field pairs for the top-k most frequent paths.

**Output:** Per-path feature vectors stored in the feature store. The statistical backbone of everything downstream.

**Complexity:** O(n) time, O(p + v) memory where v = distinct values per path (bounded by sketches in streaming mode).

**Commands that exit here:** `stats`, `anomalies`.

---

## Layer 4: Semantic Lifting

**Responsibility:** Infer likely semantic types from raw JSON scalar types and discover cross-field relationships.

**What happens:**

1. **Type inference.** DFA bank runs against values: dates, currency-like values, identifiers, enum-like fields, code tokens, phone numbers, free text. Each inference carries a confidence label (definite, dominant, heuristic, unclassified).
2. **Relationship discovery.** Conditional entropy between field pairs identifies functional dependencies. PMI identifies co-occurrence patterns.
3. **Domain plugin integration.** Registered plugins contribute additional type recognizers and relationship hints. The medical plugin recognizes ICD-10, CPT, NPI, HCPCS patterns.
4. **Temporal analysis.** When date/datetime fields are detected, inter-event intervals, monotonicity, gaps, and chronology violations are analyzed.

**Output:** Semantic type annotations, relationship graph, temporal observations, domain hints.

**Complexity:** O(n) for type inference, O(k^2 * n) for relationship discovery where k = top-k field screening threshold (default 50).

**Commands that exit here:** `invariants`, `query`.

---

## Layer 5: Concern-Oriented Scoring

**Responsibility:** Score every observation against the active concern profile's weight vector and select what matters.

**What happens:**

1. **Candidate collection.** Every notable observation from layers 2-4 becomes a candidate: high-entropy fields, anomalies, motifs, relationship discoveries, drift observations.
2. **Signal normalization.** Each of the six scoring dimensions normalized to [0, 1].
3. **Composite scoring.** Weighted sum using the profile's weight vector.
4. **Ranking.** Candidates sorted by composite score with deterministic tie-breaking (path depth, then lexicographic).
5. **Token budget enforcement.** If `--budget N` is set, greedy knapsack selection by score-per-token.

**Output:** Ranked, budgeted list of observations ready for rendering.

**Complexity:** O(c log c) where c = number of candidates (typically a few dozen to a few hundred).

**Commands that exit here:** None directly — this feeds rendering.

---

## Layer 6: Deterministic Essence Rendering

**Responsibility:** Transform the scored, ranked observations into the final output.

**What happens:**

1. **Motif collapsing.** Repeated structures represented once with count and variation notes.
2. **Template application.** The profile's rendering configuration (vocabulary level, section headers, formatting rules) is applied.
3. **Format rendering.** Output produced in the requested format: text, JSON, Markdown, or compact-AI.
4. **Redaction.** If `--redact` is enabled, pattern-based redaction applied before final emission.
5. **Provenance attachment.** Every essence includes: Vajra version, profile used, input hash, config hash, timestamp.

**Output:** The essence — a compressed, prioritized, faithful representation of the input data.

**Complexity:** O(c) where c = number of included observations.

**Commands that exit here:** `essence`, `drift`, `cluster`, `batch`.

---

## Data Flow Diagram

```text
                    +-----------+
                    | Raw Input |
                    +-----+-----+
                          |
                    [1] Parse + Normalize
                          |
                   +------v------+
                   |  Document   |
                   | (value tree |
                   |  + metadata)|
                   +------+------+
                          |
                    [2] Structural Analysis
                          |
         +-------+--------+--------+--------+
         |       |        |        |        |
      Path    Finger-   Motif   Array    Domain
      Trie    prints    Index   Morph.   Hints
         |       |        |        |        |
         +-------+--------+--------+--------+
                          |
                    [3] Statistical Analysis
                          |
                   +------v------+
                   | Feature     |
                   | Store       |
                   | (per-path   |
                   |  vectors)   |
                   +------+------+
                          |
                    [4] Semantic Lifting
                          |
         +-------+--------+--------+
         |       |        |        |
      Type    Relation-  Temporal  Plugin
      Infer.  ships      Patterns  Hints
         |       |        |        |
         +-------+--------+--------+
                          |
                    [5] Scoring + Selection
                          |
                   +------v------+
                   | Ranked      |
                   | Observations|
                   +------+------+
                          |
                    [6] Rendering
                          |
                   +------v------+
                   |   Essence   |
                   +-------------+
```

---

## Early Exit Points

Not every command runs all six layers. The engine exits as early as possible:

| Command | Layers Used |
|---|---|
| `inspect` | 1, 2 |
| `fingerprint` | 1, 2 |
| `stats` | 1, 2, 3 |
| `anomalies` | 1, 2, 3 |
| `invariants` | 1, 2, 3, 4 |
| `query` | 1, 2, 3, 4 |
| `essence` | 1, 2, 3, 4, 5, 6 |
| `drift` | 1, 2, 3 (both docs), then comparison |
| `cluster` | 1, 2 (all docs), then similarity |
| `batch` | 1, 2, 3 (all docs), then aggregation |

This is why `inspect` is fast and `essence` is slower — `inspect` exits after structural analysis while `essence` runs the full pipeline.

---

## Deep Dives

- [Algorithms](./algorithms.md) — every algorithm with provenance, complexity, and what it replaced
- [Streaming](./streaming.md) — how the engine handles documents that exceed memory
- [Determinism](./determinism.md) — how every source of nondeterminism is eliminated

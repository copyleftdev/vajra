# Vajra — Product Requirements Document v1

**Mantra: Break noise. Preserve truth.**

A high-performance Rust CLI and library that analyzes arbitrary JSON — regardless of size or complexity — extracts structural and semantic signal, detects anomalies and drift, and emits compact deterministic essences optimized for humans, auditors, and downstream LLM workflows.

---

# 1. Problem Statement

Organizations routinely handle JSON that is large, nested, inconsistent, semi-structured, operationally important, and cognitively hostile to the people who depend on it.

Examples:

* EDI-derived JSON (835, 837, 270/271 translations)
* medical claims and eligibility responses
* benefits payloads and policy rules outputs
* event streams and telemetry
* vendor APIs with undocumented schema drift

Current failure modes:

* people read raw JSON directly, burning time and missing signal
* critical anomalies hide in deep nesting
* schema drift goes unnoticed until production breaks
* repeated structures waste attention and LLM tokens
* non-technical staff cannot distinguish signal from representational noise
* LLMs receive bloated context with no prioritization

Vajra exists to solve this by creating a **universal analysis and essence layer** for JSON.

---

# 2. Users and Stakeholders

## 2.1 Operations Staff

Non-technical. They handle claims, eligibility, enrollment. Today they open JSON in a text editor or ask an engineer. They need plain language: what is this, what stands out, what might be wrong.

Vajra gives them: `vajra essence claim.json --profile staff`

## 2.2 Auditors and Compliance

They need completeness, traceability, and consistency evidence. Today they write ad hoc scripts or rely on downstream reports that obscure the raw data. They need: what fields are missing, what changed between versions, what patterns break expectations.

Vajra gives them: `vajra essence batch/ --profile auditor` and `vajra drift baseline.json current.json`

## 2.3 Engineers

They need schema details, type instability, structural regressions, and path-level analysis. Today they use jq, grep, and intuition. They need: what is the shape of this data, what is unstable, what drifted.

Vajra gives them: `vajra inspect payload.json` and `vajra fingerprint payload.json`

## 2.4 AI Pipelines

LLMs need compact, structured, deterministic context. Today raw JSON is pasted into prompts, wasting tokens on boilerplate and repeated structure. AI pipelines need: the smallest faithful representation that preserves operational meaning.

Vajra gives them: `vajra essence data.json --profile ai --format compact-ai`

## 2.5 Fraud and Risk Analysts

They need anomaly surfacing, suspicious pattern detection, and outlier identification. Today they rely on rules engines that cannot see structural anomalies. They need: what is unusual, what combinations are suspicious, what deviates from the population.

Vajra gives them: `vajra anomalies batch/ --profile fraud`

---

# 3. Design Principles

These are not aspirations. They are constraints. Every design decision must satisfy all six.

1. **Universal.** Any JSON. Any size. Any schema. Any nesting depth. No required schema, no required domain knowledge, no assumption about structure. If it parses as JSON, Vajra handles it.

2. **Deterministic.** Same input + same config + same version = same output. Always. Fingerprints, scores, orderings, essence text, anomaly rankings — all reproducible. This is non-negotiable.

3. **Honest.** Every inference is labeled as inference. Every score is decomposable. Every anomaly is explainable. Vajra never silently asserts heuristic conclusions as truth.

4. **Fast.** Operational speed. Seconds on typical payloads, minutes on gigabyte-scale batches. Not overnight batch processing. Fast enough to use interactively and in CI pipelines.

5. **Composable.** CLI, Rust library, and plugin system are each independently useful. Analyzers compose. Outputs chain. Profiles are combinable.

6. **Minimal assumption.** The core engine assumes nothing about the domain, the schema, or the purpose of the data. Domain intelligence enters only through plugins and concern profiles, never through hardcoded logic.

---

# 4. Non-Goals and Boundaries

Vajra is not:

* a general-purpose database or data store
* a replacement for jq (jq transforms; Vajra analyzes and reduces)
* a probabilistic summarizer (all reduction is deterministic and explainable)
* a GUI or BI platform
* a schema registry (though it can infer schema characteristics)
* a domain-specific medical coding engine (though domain plugins can add that intelligence)
* a query engine in its initial phases (query capabilities grow over time)
* a JSON validator or linter (though anomaly detection surfaces validation-like insights)
* a data transformation tool (Vajra reads and analyzes; it does not rewrite source data)

Its core identity is **analysis + reduction + essence generation**.

---

# 5. Core Conceptual Model

## 5.1 The Pipeline

Vajra processes JSON through six layers:

```text
Raw JSON
  -> [1] Parse + Normalize
  -> [2] Structural Analysis (paths, types, shapes)
  -> [3] Statistical Analysis (frequency, entropy, distributions)
  -> [4] Semantic Lifting (inferred types, motifs, relationships)
  -> [5] Concern-Oriented Scoring + Reduction
  -> [6] Deterministic Essence Rendering
```

Each layer depends on the one before it. Each layer's outputs are independently useful. The pipeline can exit early at any layer depending on the command.

## 5.2 JSON as Three Things Simultaneously

This is a foundational insight. Vajra treats every JSON document simultaneously as:

**A tree.** The literal nested representation. Paths, parent-child relationships, depth, sibling structure. This is what parsers give you.

**A graph.** Repeated motifs create implicit references. Co-occurring keys form relationships. Structural patterns connect distant nodes. This is what analysis reveals.

**A distribution.** Keys, values, types, lengths, null density, path rarity, cardinality — all form measurable statistical signals. This is what mathematics quantifies.

Raw JSON is not just data. It is structure, message, shape of intent, and a distribution of constraints and deviations. Vajra reads all three simultaneously.

---

# 6. Algorithm Specification

This is the engine. Every algorithm here was chosen against three criteria:

1. **Works at any scale** — O(n) or O(n log n) time complexity, bounded or streaming-compatible memory
2. **Battle-tested** — published, peer-reviewed, deployed in production systems at scale
3. **Modern** — the best current evolution of its lineage, not the textbook original

Algorithms that fail any criterion were cut. What remains is a small set of powerful primitives that compose to cover the full analysis space.

---

## 6.1 Parsing and Size Strategy

Vajra must handle JSON of any size. This requires a dual-mode parser.

**DOM mode.** For documents that fit in memory. Full random access enables rich analysis in a single pass. Parser: `simd-json` (Langdale & Lemire, 2019) — uses SIMD instructions for 2-4x throughput over conventional parsers, with measured performance exceeding 2 GB/s on modern hardware.

**Streaming mode.** For documents that exceed available memory. SAX-style event parsing with bounded memory. Enables single-pass statistics (frequency, type tracking, path extraction) and two-pass analysis where needed (first pass computes distributions, second pass scores against them).

**Hybrid strategy for large documents:**

1. Streaming first pass: path extraction, frequency counting, type profiling, sketch construction. Memory: O(p + s) where p = distinct paths, s = sketch sizes.
2. Optional selective DOM: for subtrees identified as high-signal in the first pass, parse into DOM for rich motif analysis and essence generation.

**Parsing hardening:**

* maximum nesting depth (configurable, default 256)
* maximum string length
* maximum document size in streaming mode (configurable)
* graceful error on malformed JSON with location reporting
* no panics on any input

**Complexity:** O(n) time for both modes. Memory: O(n) for DOM, O(p + s) for streaming.

---

## 6.2 Canonicalization

**Purpose:** remove irrelevant representational variance while preserving meaning. This enables reproducibility, diffing, fingerprinting, and stable essence generation.

**Standard:** RFC 8785 — JSON Canonicalization Scheme (JCS), published 2020. This is the IETF standard for deterministic JSON serialization. It specifies:

* lexicographic key ordering by UTF-16 code unit sequence
* specific number serialization (no trailing zeros, no positive sign, etc.)
* no whitespace

**Extensions beyond JCS:**

* Unicode NFC normalization (Unicode Standard Annex #15) for string comparison stability
* optional whitespace normalization within string values for analysis purposes (never mutates the source)
* null vs. absent distinction preservation (JCS does not address this; Vajra tracks it explicitly)
* array order handling policy, configurable per analysis:
  * **preserve** — order is meaningful (default)
  * **set** — treat as unordered, deduplicate
  * **multiset** — treat as unordered, preserve duplicates

**Outputs:**

* canonical byte sequence
* canonical structural signature (hash of the canonical form)
* canonical path map

**Complexity:** O(n log k) where n = total nodes, k = maximum keys per object (for sorting). Memory: O(n) for the canonical form.

---

## 6.3 Structural Path Extraction and Indexing

**Purpose:** build the foundational index that nearly every downstream analysis depends on.

For every node in the document, Vajra computes:

* full JSONPath-style path (e.g., `$.claims[*].diagnosis[*].code`)
* path depth
* node type (object, array, string, number, boolean, null)
* parent type
* sibling count
* array index position
* wildcard-normalized path (array indices replaced with `[*]`)

**Data structure:** path trie. Each trie node stores:

* the path segment
* aggregated metadata (count, type set, depth)
* children

The trie naturally deduplicates paths and enables prefix-based queries.

**Example:**

```json
{
  "claims": [
    {
      "patient": { "id": "A1" },
      "diagnosis": [{ "code": "E11.9" }]
    }
  ]
}
```

Derived wildcard paths:

* `$.claims`
* `$.claims[*]`
* `$.claims[*].patient`
* `$.claims[*].patient.id`
* `$.claims[*].diagnosis`
* `$.claims[*].diagnosis[*].code`

**Complexity:** O(n) time via DFS or iterative traversal. Memory: O(p) where p = distinct wildcard paths. For typical JSON, p << n.

---

## 6.4 Structural Fingerprinting

**Purpose:** identify the structure of a document independently of its values, enabling drift detection, clustering, and regression checks.

### Hash Function: BLAKE3

All hashing in Vajra uses BLAKE3 (O'Connor, Aumasson, Neves, Wilcox-O'Hearn, 2020).

Why BLAKE3 over alternatives:

* 3-7x faster than SHA-256 on modern hardware
* internally parallelizable via Bao tree structure
* 256-bit output, cryptographic strength
* Rust-native reference implementation (`blake3` crate)
* deterministic, no configuration needed
* one algorithm for all hashing needs in the system

### Fingerprint Types

**1. Path Set Fingerprint.** BLAKE3 hash of the sorted set of distinct wildcard paths. Captures what fields exist.

**2. Typed Path Fingerprint.** BLAKE3 hash of sorted `(path, dominant_type)` pairs. Captures what fields exist and what types they carry.

**3. Shape Fingerprint via Merkle Subtree Hashing.** Bottom-up hash computation: each leaf hashes its type; each object hashes the sorted concatenation of `(key, child_hash)` pairs; each array hashes the concatenation of child hashes (or sorted child hashes in set/multiset mode). The root hash is the shape fingerprint.

This Merkle approach has a critical secondary benefit: **subtree hashes at every node enable motif detection as a byproduct** (section 6.8). Identical subtrees produce identical hashes. This is O(n) and falls out of a single traversal.

**4. Similarity Digest via MinHash.** For comparing structural similarity across documents without exact matching.

MinHash (Broder, 1997) estimates Jaccard similarity between sets in constant time per comparison. Vajra computes MinHash signatures over the path set using k independent hash functions (k = 128 by default).

For memory efficiency at scale, use the **b-bit MinHash** variant (Li & König, 2011), which stores only the lowest b bits of each hash value. With b = 1, memory drops by 32x with well-characterized accuracy loss. The optimal b depends on the similarity range of interest; for structural comparison where we care about similarity > 0.3, b = 2-4 bits provides excellent accuracy.

**Complexity:** O(n) for all fingerprints in a single traversal. Memory: O(p) for path-based fingerprints, O(k) for MinHash signature.

---

## 6.5 Type Inference and Semantic Lifting

**Purpose:** infer likely semantic types from raw JSON scalar types. A string containing `"2024-03-15"` is technically a string but semantically a date. Vajra lifts these when the signal is unambiguous.

### Base JSON Types

string, number (integer, float), boolean, object, array, null.

### Inferred Semantic Types

Detected via deterministic finite automata and lexical pattern matching:

| Inferred Type | Detection Method |
|---|---|
| date | ISO 8601 date patterns, common US/EU formats via DFA |
| datetime | ISO 8601 datetime, epoch seconds/milliseconds (range heuristic) |
| currency-like | numeric with 2 decimal places + contextual sibling keys (amount, price, cost) |
| identifier | high cardinality + alphanumeric + consistent length or pattern |
| enum-like | low cardinality relative to occurrence count |
| code token | short, uppercase/alphanumeric, low entropy, often with sibling `description` or `system` key |
| phone-like | digit patterns matching E.164 or common national formats |
| free text | high entropy + variable length + word-like token distribution |
| percentage | numeric in [0, 100] or [0, 1] with contextual clues |
| unit-bearing | string matching `<number><whitespace><unit>` patterns |

### Detection Algorithm

1. For each distinct (path, value) pair, run the DFA bank. DFAs are compiled once at startup, evaluated in a fixed priority order. First match wins.
2. Per path, aggregate inferred types across all observed values. The dominant inferred type (> 80% of observations) becomes the path's semantic type.
3. When no DFA matches, fall back to entropy-based classification:
   * entropy < 1.0 and cardinality < 10 → enum-like
   * entropy > 4.0 and mean length > 20 → free text
   * otherwise → unclassified (left as raw JSON type)

### Honesty Contract

Every inferred type carries a confidence label:

* **definite** — DFA matched 100% of values for this path
* **dominant** — DFA matched > 80% of values
* **heuristic** — entropy/cardinality-based inference
* **unclassified** — no inference applied

Vajra never represents a heuristic inference as ground truth.

**Complexity:** O(n) — each value evaluated once against the DFA bank (DFAs are O(m) where m = value length, and m is bounded by string length limits).

---

## 6.6 Statistical Core

The statistical engine is built on five primitives. Together they cover frequency analysis, entropy computation, missingness profiling, numeric distribution analysis, and outlier detection. Each was selected for streaming compatibility, bounded memory, and formal guarantees.

### Primitive 1: Shannon Entropy

For each path, compute the Shannon entropy of observed values:

```
H(X) = -Σ p(x) log₂ p(x)
```

And the normalized entropy:

```
H_norm(X) = H(X) / log₂(|support|)
```

where |support| is the number of distinct values observed.

**Interpretation guide:**

| Entropy | Normalized | Meaning |
|---|---|---|
| 0 | 0 | constant (single value) |
| low | low | enum-like, few states |
| low | high | near-uniform over tiny support |
| high | moderate | meaningful variation — identifiers, dates, codes |
| high | high | near-uniform over large support — free text, UUIDs |

This is one of the strongest universal primitives in the system. It distinguishes boilerplate from signal without domain knowledge.

**Streaming computation:** maintained via running counts per value per path. When the value space per path is large, switch to entropy estimation from the Count-Min Sketch frequency estimates (introduces bounded approximation error).

**Complexity:** O(n) time, O(v) space where v = distinct values per path (bounded by sketch in streaming mode).

### Primitive 2: Count-Min Sketch (CMS)

For streaming frequency estimation when exact counts would exceed memory.

**Algorithm:** Count-Min Sketch with conservative update (Estan & Varghese, 2002; building on Cormode & Muthukrishnan, 2005).

Conservative update tightens the standard CMS: instead of incrementing all d counters, only increment counters that are currently equal to the minimum. This provably reduces over-estimation error without changing the data structure.

**Parameters:**
* width w = ⌈e/ε⌉ (controls accuracy; ε = desired error rate)
* depth d = ⌈ln(1/δ)⌉ (controls failure probability; δ = desired failure probability)
* default: ε = 0.001, δ = 0.01 → w ≈ 2718, d = 5

**Guarantees:** estimated count ĉ satisfies: true count ≤ ĉ ≤ true count + εN with probability ≥ 1 - δ, where N = total count.

**Use in Vajra:** frequency estimation for values, paths, and key names when the cardinality exceeds configurable thresholds. Exact counting is preferred when it fits in memory; CMS is the fallback that preserves the universality guarantee.

**Complexity:** O(d) per update, O(w * d) memory. Both are constants independent of data size.

### Primitive 3: Space-Saving Algorithm

For identifying top-k most frequent elements in a stream.

**Algorithm:** Space-Saving (Metwally, Agrawal, El Abbadi, 2005).

Maintains exactly k counters. When a new element arrives that is not being tracked, evict the element with the smallest count and replace it, incrementing the count. Despite its simplicity, Space-Saving provides guaranteed error bounds: every element whose true frequency exceeds N/k is in the summary, and estimated counts are off by at most N/k.

**Use in Vajra:** identifying the most frequent values per path, the most common structural motifs, and the most frequent key names. These top-k results directly feed essence generation (what is worth mentioning) and anomaly detection (what deviates from the common).

**Complexity:** O(1) amortized per update with a min-heap, O(k) memory.

### Primitive 4: DDSketch

For streaming quantile estimation on numeric fields.

**Algorithm:** DDSketch (Masson, Rim, Lee, 2019). Developed at Datadog and deployed in production across billions of data points per second.

DDSketch provides **relative error guarantees** for quantile estimation: for any quantile q, the returned value v̂ satisfies |v̂ - v| ≤ α|v| where α is the relative accuracy parameter.

Why DDSketch over alternatives:

* **vs. t-digest** (Dunning, 2019): t-digest provides no formal error guarantees — its accuracy is empirically good but theoretically unbounded. DDSketch has provable bounds.
* **vs. fixed-width histograms**: histograms provide absolute error, which is meaningless when values span orders of magnitude (common in financial/billing data). DDSketch's relative error adapts to the data scale.
* **vs. random sampling**: sampling provides no guarantees on tail quantiles, which are often the most important for anomaly detection.

**Key property: mergeability.** DDSketch instances can be merged exactly, preserving accuracy guarantees. This enables parallel batch processing: analyze partitions independently, merge sketches for global statistics.

**Parameters:** α = 0.01 (1% relative error) by default. Memory: O(log(max/min) / log(1 + α)) buckets — typically a few hundred for financial data spanning cents to millions.

**Use in Vajra:** numeric distribution summary (percentiles, spread, tails), outlier detection thresholds, and comparative analysis in drift detection.

**Complexity:** O(1) per insertion, O(1) per quantile query. Memory proportional to the dynamic range of the data, not the volume.

### Primitive 5: Median Absolute Deviation (MAD)

For robust central tendency and dispersion estimation.

```
MAD = median(|xᵢ - median(X)|)
```

**Why MAD over standard deviation:** standard deviation is catastrophically sensitive to outliers — a single extreme value can inflate σ enough to mask all other anomalies. MAD has a **breakdown point of 50%**: up to half the data can be arbitrarily corrupted before MAD gives a misleading result. This is the strongest possible breakdown point for any location/scale estimator.

The **modified z-score** using MAD:

```
z_MAD = 0.6745 * (xᵢ - median(X)) / MAD
```

(where 0.6745 = Φ⁻¹(0.75), making it comparable to standard z-scores under normality)

**Use in Vajra:** the primary outlier detection method for all numeric fields. A |z_MAD| > 3.5 is an anomaly candidate (Iglewicz & Hoaglin, 1993). This threshold is configurable per profile.

**Streaming computation:** exact MAD requires sorted data, but running approximate median via DDSketch enables streaming MAD estimation with bounded relative error.

**Complexity:** O(n) with sorting, or O(n) streaming via DDSketch approximation.

---

### 6.6.1 Frequency Analysis (assembled from primitives)

Using CMS + Space-Saving + Shannon entropy:

* **key frequency** — how often each key name appears
* **path frequency** — how often each wildcard path appears
* **value frequency** — how often each value appears at a given path
* **enum cardinality** — distinct value count per path
* **null rate** and **missingness rate** per path
* **co-occurrence** — measured via Pointwise Mutual Information (PMI) between field pairs:

```
PMI(x, y) = log₂(P(x, y) / (P(x) * P(y)))
```

PMI is the information-theoretic standard for measuring association strength. Positive PMI means x and y co-occur more than chance; negative means they avoid each other. Bounded to top-k paths to control O(k²) pairwise computation.

### 6.6.2 Missingness and Presence Profiling

For each wildcard path, tracked via simple per-path counters:

* present ratio
* absent ratio
* null ratio
* empty-string ratio
* empty-array ratio
* type instability ratio (fraction of observations where type differs from mode)

**Derived concepts:**

* **quasi-required field** — present ratio > 0.95
* **optional but high-value** — present ratio < 0.5 but high entropy when present
* **suspicious omission** — quasi-required field suddenly absent in specific records
* **structurally unstable** — type instability ratio > 0.05

This is essential because often the issue is not wrong values but **silence where signal should exist**.

**Complexity:** O(n) time, O(p) memory.

### 6.6.3 Numeric Distribution Analysis

Using DDSketch + MAD:

* min, max, mean, median (exact or via sketch)
* MAD and modified z-scores
* percentiles (p01, p05, p25, p50, p75, p95, p99) via DDSketch
* skewness proxy: (mean - median) / MAD (robust skewness measure)
* heavy-tail indicator: p99/p50 ratio
* repeated exact-value concentration: fraction of values equal to the mode

**Benford's Law analysis** for leading digit distribution:

Benford's Law (Newcomb, 1881; Benford, 1938; formalized by Hill, 1995) states that in many naturally occurring numeric datasets, the leading digit d occurs with probability:

```
P(d) = log₁₀(1 + 1/d)
```

Departure from this distribution, measured via chi-squared goodness-of-fit or MAD of digit frequencies, is a well-established forensic signal for fabricated or synthetic data (Nigrini, 1996). Effective for financial amounts, counts, and quantities. Not applicable to identifiers, codes, or constrained-range values — Vajra applies this analysis only to paths classified as numeric with high cardinality and range spanning at least one order of magnitude.

**Complexity:** O(n) for all metrics. DDSketch enables streaming computation with O(1) per insertion.

---

## 6.7 Array Morphology Analysis

**Purpose:** treat arrays as meaningful structures with analyzable shape, not opaque blobs.

For each array-typed path, Vajra computes:

* **cardinality distribution** across instances (via DDSketch)
* **type homogeneity index** — fraction of elements sharing the dominant type
* **element uniqueness ratio** — distinct elements / total elements
* **nested shape diversity** — number of distinct Merkle subtree hashes among array elements (from section 6.4)

**Why this matters:** arrays are where complexity hides. A claim with 14 service lines, a patient with 8 diagnosis codes, an event stream with thousands of entries — the array is the container of operational reality. Knowing that an array's elements are structurally uniform (one Merkle hash, repeated 14 times) vs. structurally diverse (5 distinct shapes among 14 elements) is immediately actionable.

**Complexity:** O(n) — all metrics computed during the primary traversal. No additional passes needed.

---

## 6.8 Structural Motif Detection

**Purpose:** find repeated substructures and quantify their frequency.

**Algorithm: Merkle Subtree Hash Counting.**

This is a direct byproduct of the shape fingerprinting in section 6.4. During bottom-up Merkle hash computation, every subtree receives a hash. To detect motifs:

1. Collect all subtree hashes at object and array-element level.
2. Count frequencies via hash map (or CMS in streaming mode).
3. Subtrees whose hash appears more than once are structural motifs.
4. Rank motifs by: frequency × subtree size (node count). This prioritizes large, frequently repeated structures — exactly the motifs that matter for essence compression.

**For near-identical motifs (fuzzy matching):**

SimHash (Charikar, 2002) over the set of (key, value_type) pairs within each subtree. SimHash produces fixed-width fingerprints where Hamming distance approximates cosine distance in the original feature space. Subtrees whose SimHash values have Hamming distance ≤ t (default t = 3 out of 64 bits) are grouped as near-motifs.

This captures structures that are semantically the same but differ in one or two fields — e.g., service lines where most fields are identical but the procedure code and amount vary.

**Uses:**

* essence compression — "this pattern repeats 42 times, here is one instance with variations noted"
* template generation
* structural deduplication in AI handoff
* anomaly detection — an array element whose structure doesn't match the dominant motif

**Complexity:** O(n) time (single traversal), O(m) memory where m = distinct subtree hashes.

---

## 6.9 Anomaly and Outlier Detection

**Purpose:** surface records, fields, or structural elements that deviate meaningfully from the population.

Vajra detects anomalies across four dimensions, using only deterministic, interpretable methods.

### Dimension 1: Numeric Outliers

**Method:** MAD-based modified z-scores (section 6.6, Primitive 5).

Flag values where |z_MAD| exceeds the profile threshold (default 3.5). Every flagged value carries its z_MAD score and the path's median and MAD for context.

### Dimension 2: Rarity Outliers

**Method:** inverse frequency scoring.

For each (path, value) pair, the rarity score is:

```
rarity(path, value) = -log₂(freq(value_at_path) / total_at_path)
```

This is the **self-information** (Shannon, 1948) of observing that value at that path. Common values score low; rare values score high. A value seen once in 10,000 records scores ~13.3 bits — unambiguously rare.

Flag values whose rarity exceeds the profile threshold (default: > mean_rarity + 2 * MAD_of_rarity for that path).

### Dimension 3: Structural Outliers

**Method:** Jaccard distance from the dominant structural fingerprint.

For batch analysis, compute the most common path-set fingerprint (the structural mode). For each document, compute:

```
structural_anomaly = 1 - J(doc_paths, mode_paths)
```

where J is Jaccard similarity. Documents with structural_anomaly > threshold (default 0.2) are flagged. The specific missing/extra paths are reported.

### Dimension 4: Type Instability

**Method:** per-path type instability score.

```
instability(path) = 1 - (count_of_dominant_type / total_observations)
```

Paths with instability > 0.01 are flagged. Individual records that contribute the minority type are identified.

### What Was Excluded and Why

**Isolation Forest** (Liu, Ting, Zhou, 2008): requires random sampling and tree construction, non-deterministic without careful seeding, O(n log n) per tree × number of trees, requires tuning of contamination parameter. Unnecessary when MAD + rarity + structural distance cover the space with stronger interpretability.

**Local Outlier Factor** (Breunig, Kriegel, Ng, Sander, 2000): O(n²) distance computation in naive form, O(n log n) with spatial indexing. Sensitive to the k parameter. Breaks the universality requirement on large datasets. Better suited to exploratory analysis than deterministic pipelines.

**Any method requiring training data or labeled examples:** Vajra operates on cold data with no prior. Every anomaly method must work on a single document or a single batch with no history.

**Complexity:** O(n) for all four dimensions. Each is a single pass or a lookup against precomputed statistics.

---

## 6.10 Schema Drift Detection

**Purpose:** detect and quantify structural, type, and distributional changes between JSON versions, sources, or time periods.

### Structural Drift

**Path set symmetric difference:**

```
added_paths = paths(B) \ paths(A)
removed_paths = paths(A) \ paths(B)
```

O(p) with sorted path sets or hash sets.

### Type Drift

For each path present in both A and B, compare dominant types. Any path where the dominant type changed is flagged.

### Distributional Drift

**Jensen-Shannon Divergence (JSD)** for comparing value distributions between versions.

JSD (Lin, 1991) is defined as:

```
JSD(P ‖ Q) = ½ KL(P ‖ M) + ½ KL(Q ‖ M)
```

where M = ½(P + Q) and KL is Kullback-Leibler divergence.

Why JSD is the right primitive:

* **Symmetric:** JSD(P ‖ Q) = JSD(Q ‖ P), unlike KL divergence
* **Always finite:** well-defined even when P and Q have different supports, unlike KL which diverges when Q(x) = 0 and P(x) > 0
* **Bounded:** JSD ∈ [0, 1] when using log base 2
* **Proper metric:** √JSD is a true metric (Endres & Schindelin, 2003; Österreicher & Vajda, 2003), satisfying the triangle inequality. This means drift magnitudes can be meaningfully compared and accumulated.

For paths with categorical values: compute JSD directly from frequency tables.
For paths with numeric values: compute JSD over discretized distributions (histogram bins), or use **1D Wasserstein distance** (earth mover's distance) which provides an interpretable "how far did values move" measure. 1D Wasserstein is O(n log n) via sorting the CDFs, and unlike JSD, it accounts for the magnitude of value shifts, not just probability mass redistribution.

### Drift Report Structure

Each drifted path is classified:

* **additive** — new paths appeared
* **subtractive** — paths disappeared
* **type-mutative** — dominant type changed
* **distributional** — JSD or Wasserstein exceeds threshold
* **cardinality-shift** — array lengths changed significantly
* **null-rate-shift** — null/missing ratio changed significantly

Each entry includes: path, drift class, magnitude (JSD or Wasserstein value), and human-readable description.

**Severity scoring:** weighted sum of drift dimensions, configurable per profile. Auditor profiles weight subtractive drift highest (missing data); engineer profiles weight type-mutative highest (breaking changes).

**Complexity:** O(n + m) where n, m are the sizes of the two documents. Memory: O(p) for path-level statistics.

---

## 6.11 Similarity and Clustering

**Purpose:** group related JSON records, identify payload families, and find the odd document in a batch.

### For Small Batches (< 1,000 documents)

**Exact pairwise Jaccard similarity** over wildcard path sets.

```
J(A, B) = |paths(A) ∩ paths(B)| / |paths(A) ∪ paths(B)|
```

O(n²) pairwise but tractable at small scale. Results are exact and deterministic.

### For Large Batches

**MinHash + Locality-Sensitive Hashing (LSH).**

MinHash signatures are computed during fingerprinting (section 6.4). LSH partitions the MinHash signature into b bands of r rows each, hashing each band into buckets. Two documents that share a bucket in any band are candidate pairs.

The probability that two documents with Jaccard similarity s become candidates is:

```
P(candidate) = 1 - (1 - s^r)^b
```

With k = b × r hash functions, this creates an S-curve threshold. For k = 128, b = 16, r = 8: documents with similarity > 0.5 have > 98% chance of being found; documents with similarity < 0.2 have < 2% chance of false positive.

**Clustering from LSH candidates:**

1. Build candidate graph from LSH bucket collisions.
2. Connected components form initial clusters.
3. Within each component, compute exact pairwise similarity and split if necessary.

This achieves near-linear time clustering: O(n) for MinHash computation, O(n) amortized for LSH indexing, O(c²) for refinement within each component c.

### What Was Excluded and Why

**Hierarchical agglomerative clustering:** O(n² log n) time, O(n²) memory for the distance matrix. Breaks on batches > ~10K documents. Replaced by LSH-based component detection which provides similar grouping at O(n).

**k-medoids / k-means:** requires specifying k (number of clusters) in advance, which Vajra cannot know. Also O(n²) per iteration for k-medoids. LSH component detection discovers the natural cluster count from the data.

**Complexity:** O(n) for MinHash + LSH. O(c²) worst case for intra-component refinement, but c << n in practice.

---

## 6.12 Cross-Field Relationship Discovery

**Purpose:** infer likely invariants and dependencies between fields from observed data.

### Conditional Entropy

For field pairs (X, Y):

```
H(Y|X) = -Σ p(x,y) log₂(p(y|x))
```

Low H(Y|X) means X strongly predicts Y. If H(Y|X) ≈ 0, Y is functionally determined by X.

### Pointwise Mutual Information

As defined in section 6.6.1. Identifies fields that co-occur more (or less) than expected.

### Discovery Procedure

1. Screen: consider only paths with occurrence count > 30 (configurable). This bounds the search space.
2. For all pairs among the top-k most frequent paths (default k = 50): compute conditional entropy and PMI.
3. Rank by: ascending H(Y|X) for dependency strength, descending |PMI| for association strength.
4. Report the top relationships with examples.

**Example discoveries:**

* if `status = denied`, then `denial_reason` present 97% of the time (conditional presence)
* `procedure_code` and `service_date` have PMI = 3.2 (strong co-occurrence)
* `subscriber.id` functionally determines `subscriber.name` — H(name|id) ≈ 0

### Scope Bounding

Unlike general association rule mining (Apriori, FP-Growth), which explores an exponential itemset space, this approach is bounded by the k² pairwise computation. With k = 50, that is 2,500 pairs — trivial even on large datasets.

**Complexity:** O(k² × n) where k is the field screening threshold. With k = 50 and n = millions, this is practical.

---

## 6.13 Temporal Pattern Analysis

**Purpose:** extract time-based signal from JSON when timestamp or date fields are present.

**Activation:** this module runs only when type inference (section 6.5) identifies one or more paths as date or datetime with confidence level "definite" or "dominant." It is not a universal analysis — many JSON documents contain no temporal data.

### Temporal Parsing

Use a ranked list of format patterns:

1. ISO 8601 (datetime and date)
2. RFC 3339
3. Unix epoch seconds (detected by range: 946684800 to 2524608000, i.e., 2000-2050)
4. Unix epoch milliseconds (range × 1000)
5. Common locale formats (US MM/DD/YYYY, EU DD/MM/YYYY) with ambiguity flagging

When ambiguity exists (e.g., `01/02/2024` could be Jan 2 or Feb 1), Vajra flags the ambiguity rather than silently choosing.

### Temporal Analysis

* **Inter-event interval distribution** — DDSketch of time deltas between consecutive records
* **Monotonicity detection** — is the sequence strictly or mostly increasing? O(n) single pass
* **Gap detection** — intervals that exceed median + 3 × MAD of the interval distribution
* **Invalid chronology** — cases where a logically later event has an earlier timestamp (e.g., `adjudication_date < service_date`)
* **Batching detection** — clusters of identical or near-identical timestamps suggesting bulk processing

**Complexity:** O(n) for all temporal analyses.

---

# 7. Essence Model

An **essence** is a deterministic reduction of analyzed JSON, shaped for a specific concern. It is not a summary — it is a compressed, prioritized, faithful representation that preserves operationally relevant information while minimizing cognitive and token burden.

## 7.1 Essence Types

### Structural Essence

Shape, repeated motifs, key paths, schema characteristics, type stability.

### Operational Essence

Workflow status, error indicators, counts, exceptions, state transitions.

### Audit Essence

Completeness, traceability, missing field patterns, anomalies, drift from baseline.

### Clinical / Medical Review Essence

Patient, service, provider, diagnosis, procedure, coverage, adjudication patterns. Activated only when domain plugins are enabled. Not part of core.

### Fraud / Risk Essence

Suspicious repetition, unusual field combinations, numeric outliers, structural deviations, Benford's Law departures.

### AI Handoff Essence

Optimized for downstream LLM consumption: compact, structured, deterministic, prioritized for context window efficiency.

## 7.2 Essence Construction Algorithm

```text
1. Collect all candidate observations from the analysis pipeline:
   - notable fields (high entropy, high rarity, anomalous)
   - structural motifs with frequency
   - anomalies with scores
   - drift observations
   - relationship discoveries

2. Score each candidate using the concern profile's weight vector (section 9).

3. Collapse repeated motifs:
   - identify dominant motif per array path
   - represent as: "pattern (repeated N times)" + specific variations

4. Rank candidates by composite score, deterministic tie-breaking (section 9.3).

5. If a token budget is specified (--budget N):
   - estimate token cost per candidate (word count × 1.3 as token approximation)
   - select candidates greedily by score-per-token until budget exhausted
   - this is the fractional knapsack approximation — optimal for the greedy case

6. Render using deterministic templates per concern profile.

7. Emit in requested format (text, json, markdown, compact-ai).
```

## 7.3 Essence Output Contract

Every essence includes:

* **identity** — document hash or batch description
* **input statistics** — size, node count, depth, distinct paths
* **dominant structure** — the most common shape with its frequency
* **top motifs** — repeated substructures with counts
* **notable fields** — ranked by importance score with explanations
* **anomalies** — ranked by severity with score decomposition
* **drift notes** — if a comparison baseline was provided
* **concern-specific observations** — from the active profile
* **confidence labels** — per observation, per the honesty contract
* **provenance** — Vajra version, profile used, config hash, timestamp

---

# 8. Scoring Model

Every candidate observation receives a composite importance score. This score determines what appears in the essence and in what order.

## 8.1 Signal Dimensions

Each dimension is normalized to [0, 1] before weighting:

| Dimension | Definition | Normalization |
|---|---|---|
| rarity | self-information of the observation | min-max across all candidates |
| instability | type instability score for the path | raw ratio [0, 1] |
| entropy_signal | |0.5 - normalized_entropy| | raw [0, 0.5], scaled to [0, 1] |
| structural_coverage | fraction of total nodes under this path | raw ratio [0, 1] |
| anomaly_strength | max anomaly score across dimensions | min-max across candidates |
| concern_relevance | profile-specific boost for this path/observation type | defined per profile [0, 1] |

**Note on entropy_signal:** the most informative paths are those with moderate entropy — not constant (boring) and not uniform noise (unstructurable). The distance from 0.5 normalized entropy captures this: both very low and very high entropy push the signal toward 1.

## 8.2 Composite Score

```
score = Σ wᵢ × signal_i
```

where weights wᵢ are defined per concern profile (section 10). The sum of weights is normalized to 1.

## 8.3 Deterministic Tie-Breaking

When two candidates have identical composite scores, break ties by:

1. Path depth — shallower paths first (broader impact)
2. Lexicographic path order — alphabetical by wildcard path

This ensures identical scores always resolve in the same order.

---

# 9. Concern Profiles

Profiles configure the scoring weights, rendering style, vocabulary level, and anomaly sensitivity. They tune how Vajra presents its analysis, not what analysis it performs.

## 9.1 Built-in Profiles

### `--profile staff`

**Purpose:** non-technical operations staff need "what is this and what stands out."

| Weight | Value |
|---|---|
| rarity | 0.10 |
| instability | 0.05 |
| entropy_signal | 0.10 |
| structural_coverage | 0.25 |
| anomaly_strength | 0.30 |
| concern_relevance | 0.20 |

Rendering: plain language, no JSONPath, no technical jargon. Anomalies described in terms of business impact. Structural boilerplate hidden. Section headers: "Document summary," "What stands out," "What this likely means."

### `--profile auditor`

**Purpose:** completeness, traceability, consistency evidence.

| Weight | Value |
|---|---|
| rarity | 0.10 |
| instability | 0.20 |
| entropy_signal | 0.10 |
| structural_coverage | 0.10 |
| anomaly_strength | 0.20 |
| concern_relevance | 0.30 |

Rendering: formal. Missing fields listed with paths. Type inconsistencies with examples. Drift metrics with severity scores. Concern_relevance boosts: completeness, traceability, required-field absence.

### `--profile engineer`

**Purpose:** schema details, structural analysis, regression signals.

| Weight | Value |
|---|---|
| rarity | 0.15 |
| instability | 0.25 |
| entropy_signal | 0.15 |
| structural_coverage | 0.15 |
| anomaly_strength | 0.15 |
| concern_relevance | 0.15 |

Rendering: technical. JSONPath paths, type annotations, cardinalities. Diff-style output for drift. Fingerprints displayed.

### `--profile ai`

**Purpose:** compact, structured context for downstream LLMs.

| Weight | Value |
|---|---|
| rarity | 0.15 |
| instability | 0.10 |
| entropy_signal | 0.20 |
| structural_coverage | 0.20 |
| anomaly_strength | 0.20 |
| concern_relevance | 0.15 |

Rendering: compact machine-readable format. Motifs collapsed aggressively. Repeated structures represented once with count. Explicit caveats on inferences. Structured sections for easy LLM parsing.

### `--profile fraud`

**Purpose:** suspicious patterns, outliers, unusual combinations.

| Weight | Value |
|---|---|
| rarity | 0.25 |
| instability | 0.10 |
| entropy_signal | 0.10 |
| structural_coverage | 0.05 |
| anomaly_strength | 0.35 |
| concern_relevance | 0.15 |

Rendering: investigative. Outliers with context. Benford's Law departures. Suspicious value repetition. Unusual co-occurrence patterns. Concern_relevance boosts: numeric anomalies, identifier patterns, timing irregularities.

## 9.2 Custom Profiles

Custom profiles are defined in TOML:

```toml
[profile.my_profile]
name = "custom-review"
description = "Internal review for claims processing"

[profile.my_profile.weights]
rarity = 0.15
instability = 0.20
entropy_signal = 0.10
structural_coverage = 0.10
anomaly_strength = 0.25
concern_relevance = 0.20

[profile.my_profile.rendering]
vocabulary = "plain"           # plain | technical | formal
show_paths = false
show_scores = false
motif_collapse_threshold = 3   # collapse motifs repeated > N times
anomaly_threshold = 3.5        # MAD z-score threshold

[profile.my_profile.concern_boosts]
paths_containing = ["denied", "adjustment", "override"]
observation_types = ["missingness", "type_instability"]
boost_factor = 1.5
```

---

# 10. CLI Specification

## 10.1 Commands

```bash
# Single-document analysis
vajra inspect <input>                    # full structural report
vajra essence <input> --profile <name>   # concern-oriented essence
vajra anomalies <input>                  # anomaly report
vajra stats <input>                      # statistical summary
vajra fingerprint <input>                # structural fingerprints

# Multi-document analysis
vajra drift <baseline> <candidate>       # drift detection between two documents
vajra cluster <inputs...>                # similarity clustering
vajra invariants <inputs...>             # cross-field relationship discovery

# Query (Phase 2+)
vajra query <input> '<expression>'       # path-based query with analysis functions
```

`<input>` accepts: file path, `-` for stdin, directory (processes all `.json` files), glob pattern.

## 10.2 Global Flags

```
--format <text|json|ndjson|markdown|compact-ai>   Output format (default: text)
--profile <name>                                    Concern profile (default: engineer)
--config <path>                                     Configuration file
--budget <N>                                        Token budget for essence output
--streaming                                         Force streaming mode regardless of file size
--seed <N>                                          Seed for any randomized algorithm
--canonical                                         Emit canonical JSON (RFC 8785)
--strict                                            Error on any ambiguity instead of flagging
--quiet                                             Suppress progress output
--explain                                           Include score decomposition in output
```

## 10.3 Output Modes

| Format | Use Case |
|---|---|
| `text` | human reading in terminal |
| `json` | machine consumption, further processing |
| `ndjson` | streaming consumption, log pipelines |
| `markdown` | documentation, reports, rendered display |
| `compact-ai` | LLM context window optimization |

All formats include the same information; they differ in rendering.

---

# 11. Query Model

## 11.1 Phase 1: Path-Based Filtering

Simple path expressions for targeting analysis:

```bash
vajra stats input.json --path '$.claims[*].amount'
vajra anomalies input.json --path '$.claims[*].diagnosis'
```

## 11.2 Phase 2+: Analysis Functions

Vajra-specific functions that expose the analysis engine:

```
entropy(path)              # Shannon entropy at this path
rarity(path, value)        # self-information of a value
instability(path)          # type instability score
motif(path)                # dominant motif at this array path
drift(path)                # drift magnitude (requires baseline)
anomaly_score(path)        # composite anomaly score
invariants(path)           # discovered relationships involving this path
```

These functions compose with path expressions:

```bash
vajra query input.json 'entropy($.claims[*].status)'
vajra query input.json 'anomaly_score($.claims[*].amount) > 3.5'
```

**Note on JSONAta:** the Rust ecosystem does not have a mature JSONAta implementation as of this writing. Rather than claiming JSONAta compatibility, Vajra defines its own expression language inspired by JSONPath with analysis extensions. Full JSONAta interop is a potential future goal, not a Phase 1-2 commitment.

---

# 12. Rust Architecture

## 12.1 Crate Layout

```
vajra/
├── vajra-core/          # parsing, traversal, canonicalization, path extraction
├── vajra-types/         # shared types, feature vectors, result contracts, metadata
├── vajra-fingerprint/   # BLAKE3 hashing, Merkle subtree hashing, MinHash, SimHash, LSH
├── vajra-stats/         # CMS, Space-Saving, DDSketch, MAD, entropy, frequency, PMI
├── vajra-anomaly/       # outlier scoring, instability, rarity, structural anomaly
├── vajra-drift/         # JSD, Wasserstein, path diff, drift classification
├── vajra-motif/         # motif counting, near-motif grouping, motif compression
├── vajra-essence/       # concern profiles, scoring, ranking, rendering, templates
├── vajra-query/         # expression parsing, path filtering, analysis functions
├── vajra-cli/           # CLI argument parsing, command dispatch, output formatting
├── vajra-domain-med/    # optional: medical/EDI pattern recognizers, domain concern profiles
├── vajra-domain-sec/    # optional: security plugin (CVE, MITRE ATT&CK, IPs, hashes, JWT)
├── vajra-domain-devops/ # optional: DevOps plugin (K8s, Docker, Terraform, AWS ARNs, semver)
├── vajra-source/        # source code parsing via tree-sitter (9 languages)
└── vajra-domain-source/ # source code recognizers (naming conventions, import paths)
```

Each crate has a single responsibility. Dependencies flow downward: `vajra-cli` depends on everything; `vajra-core` and `vajra-types` depend on nothing internal.

## 12.2 Core Traits

```rust
/// Primary analysis trait. Each analyzer examines a document and produces typed output.
pub trait Analyzer {
    type Output;
    fn analyze(&self, doc: &Document) -> Result<Self::Output>;
}

/// Streaming-compatible analysis. Receives events from SAX-style parsing.
pub trait StreamAnalyzer {
    type Accumulator: Default;
    type Output;
    fn on_event(&self, event: &JsonEvent, acc: &mut Self::Accumulator) -> Result<()>;
    fn finalize(&self, acc: Self::Accumulator) -> Result<Self::Output>;
}

/// Feature extraction into the shared feature store.
pub trait FeatureExtractor {
    fn extract(&self, doc: &Document, features: &mut FeatureStore) -> Result<()>;
}

/// Concern profile for scoring and rendering.
pub trait ConcernProfile {
    fn weights(&self) -> &ScoreWeights;
    fn score(&self, candidate: &CandidateObservation) -> f64;
    fn render(&self, essence: &Essence, format: OutputFormat) -> Result<String>;
}

/// Structural fingerprinting.
pub trait Fingerprinter {
    fn fingerprint(&self, doc: &Document) -> Fingerprint;
}

/// Drift detection between two analyzed documents.
pub trait DriftDetector {
    fn compare(&self, lhs: &AnalyzedDocument, rhs: &AnalyzedDocument) -> DriftReport;
}
```

These traits are small, composable, and independently testable. The `StreamAnalyzer` trait is the key addition that enables arbitrary-size document support — any analyzer that implements it can participate in streaming mode.

## 12.3 Public Library API

Beyond the CLI, Vajra is usable as a Rust library:

```rust
use vajra_core::Document;
use vajra_stats::StatsAnalyzer;
use vajra_essence::{EssenceBuilder, profiles};

let doc = Document::parse_file("input.json")?;
let stats = StatsAnalyzer::default().analyze(&doc)?;
let essence = EssenceBuilder::new()
    .profile(profiles::staff())
    .stats(&stats)
    .build()?;

println!("{}", essence.render_text());
```

For streaming:

```rust
use vajra_core::StreamParser;
use vajra_stats::StreamingStats;

let parser = StreamParser::open("huge.json")?;
let mut stats = StreamingStats::default();
for event in parser {
    stats.on_event(&event?)?;
}
let result = stats.finalize()?;
```

---

# 13. Internal Data Model

Vajra maintains a hybrid representation optimized for both semantic analysis and fast path-indexed access.

**Parsed value tree.** The DOM representation when available. Retains full structure. Used for motif analysis, essence generation, and subtree extraction.

**Path trie.** Wildcard-normalized paths as a trie. Each trie node stores aggregated metadata: count, type distribution, depth, parent type. Primary index for path-based queries and statistics.

**Feature store.** Per-path feature vectors: entropy, cardinality, null rate, type distribution, DDSketch for numeric paths, CMS for high-cardinality value sets. This is the statistical backbone.

**Motif index.** Map from Merkle subtree hash to: frequency, representative subtree, list of locations. Built during fingerprinting.

**Scoring view.** Per-observation composite scores computed from the feature store using the active profile's weights. Materialized lazily on essence generation.

**Provenance metadata.** Input file identity (BLAKE3 hash of raw bytes), Vajra version, config hash, analysis timestamp.

**Design rule:** avoid flattening too early. The tree carries semantics that path-indexed access loses (sibling context, nesting meaning). Retain the tree; expose path-indexed access as a parallel fast path.

---

# 14. Performance and Scalability

## 14.1 Targets

| Scenario | Target |
|---|---|
| 1 MB JSON, full analysis | < 100ms |
| 100 MB JSON, full analysis | < 5s |
| 1 GB JSON, streaming mode | < 60s |
| 10 GB JSON, streaming mode | < 10 minutes |
| 10,000 document batch, clustering | < 30s |
| Fingerprint comparison | < 1μs per pair |

These targets assume commodity hardware (8-core, 16GB RAM). They are validation targets for Phase 1 benchmarking, not promises.

## 14.2 Strategies

**Zero-copy parsing** where the parser supports it (simd-json operates on mutable borrowed slices). Avoid copying string values when only hashing or pattern-matching them.

**Arena allocation** for path trie nodes and feature store entries during a single document's analysis. One arena per document, freed atomically when done.

**Bounded memory** in streaming mode. Total memory is O(p + s) where p = distinct paths and s = sum of sketch sizes. For typical JSON with < 1,000 distinct paths and default sketch parameters: < 10MB regardless of document size.

**Parallel batch processing** via Rayon. Each document in a batch is analyzed independently, then per-path statistics are merged. DDSketch mergeability is critical here — partition-level sketches merge into global sketches with no accuracy loss.

**Avoiding accidental quadratics.** The most common performance trap in JSON analysis is O(n × p) nested loops over nodes and paths. Vajra's path trie ensures path lookup is O(depth) not O(p), and all analysis passes are single-traversal O(n).

---

# 15. Determinism and Reproducibility

## 15.1 Guarantee

Given identical:
* input bytes
* configuration (profile, flags, config file)
* Vajra version

Vajra produces identical:
* fingerprints
* scores (to floating-point bit-level)
* orderings
* essence text (byte-for-byte)
* anomaly rankings

## 15.2 Sources of Nondeterminism and Mitigations

| Source | Mitigation |
|---|---|
| Hash map iteration order | Use `BTreeMap` for all externally-visible orderings. `HashMap` only for internal scratch where order is never observed. |
| Thread scheduling in parallel batch | Deterministic merge order: sort by input identity before merging. Parallel execution affects speed, never output. |
| Floating-point accumulation order | Fixed traversal order (DFS, left-to-right). Summations in deterministic order. |
| Randomized algorithms (MinHash, SimHash) | Seeded PRNG. Default seed is 0. `--seed` flag for explicit control. |
| Platform differences (float formatting) | Use Rust's `ryu` crate for float-to-string conversion, which is deterministic and platform-independent. |

---

# 16. Error Model and Graceful Degradation

## 16.1 Error Taxonomy

| Category | Behavior |
|---|---|
| **Parse error** (malformed JSON) | Report error with byte offset and context. Exit with nonzero status. No partial output unless `--partial` flag is set. |
| **Size limit exceeded** | In DOM mode: switch to streaming automatically (with diagnostic message). In streaming mode: report if configured maximum is exceeded. |
| **Depth limit exceeded** | Truncate analysis at the configured depth limit. Flag truncated subtrees in the output. |
| **Pathological input** (e.g., 10M distinct paths) | Sketch-based analysis activates automatically when cardinality thresholds are exceeded. Diagnostic message notes the switch. |
| **Plugin error** | Isolate the failing plugin. Continue core analysis. Report plugin failure in provenance metadata. |
| **Ambiguous inference** | Flag the ambiguity (never guess silently). In `--strict` mode: promote to error. |

## 16.2 Design Rule

Every error path must produce either:
1. A correct, complete result, or
2. A correct, explicitly partial result with clear indication of what was skipped and why, or
3. A clean failure with a diagnostic message

Never: silent data loss, silent degradation, or partial output that looks complete.

---

# 17. Security Model

Vajra will process real operational JSON, potentially including PHI, PII, financial data, and proprietary business logic.

**Local-first execution.** No network calls in core. No telemetry. No cloud dependencies. The binary runs airgapped.

**No data persistence.** Vajra does not write input data to disk, cache it, or store it. All analysis is in-memory (or streaming) and ephemeral.

**Explicit redaction.** `--redact` flag enables pattern-based redaction before essence rendering. Configurable redaction rules in the config file. Built-in patterns for: SSN, email, phone, credit card numbers. Plugin-extensible for domain-specific PII/PHI.

**Deterministic redaction.** Redaction happens before rendering, not after. The essence never contains unredacted sensitive values. The same redaction config always produces the same redacted output.

**Input hardening.**

* maximum document size (configurable, default 10GB)
* maximum nesting depth (configurable, default 256)
* maximum string length (configurable, default 10MB per string)
* no eval, no code execution from input
* all parsing is safe Rust — no unsafe blocks in the parser

**Panic safety.** No analysis path may panic on any input. Use `Result` types throughout. Fuzzing (section 21.3) validates this.

**Log hygiene.** Diagnostic and error logs never contain input values. Only structural information (paths, types, counts) appears in logs.

**Security posture: boringly safe.** No clever tricks. No interesting attack surface. Parse, analyze, emit.

---

# 18. Plugin System

Core Vajra is domain-agnostic. Plugins add domain intelligence without contaminating the universal engine.

## 18.1 Plugin Interface

```rust
/// A plugin registers one or more capabilities.
pub trait VajraPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;

    /// Additional type recognizers beyond the core DFA bank.
    fn type_recognizers(&self) -> Vec<Box<dyn TypeRecognizer>> { vec![] }

    /// Additional concern profile definitions.
    fn concern_profiles(&self) -> Vec<Box<dyn ConcernProfile>> { vec![] }

    /// Field relationship heuristics (e.g., "code + description + system = coded concept").
    fn relationship_hints(&self) -> Vec<RelationshipHint> { vec![] }

    /// Custom rendering templates.
    fn renderers(&self) -> Vec<Box<dyn EssenceRenderer>> { vec![] }
}
```

## 18.2 Loading and Isolation

Plugins are compiled Rust crates linked at build time (static plugins) or loaded as shared libraries via `libloading` (dynamic plugins).

**Error isolation:** if a plugin panics or returns an error, the failure is caught at the plugin boundary. Core analysis continues. The plugin failure is recorded in provenance metadata and reported to the user.

**No plugin may:**
* modify the core analysis pipeline
* access the filesystem beyond its own configuration
* make network calls
* mutate the input document

## 18.3 Medical / EDI Plugin Example (`vajra-domain-med`)

* CPT, HCPCS, ICD-10, NDC pattern recognizers
* claim / service line motif hinting
* subscriber / patient / provider role identification from key patterns
* adjudication terminology lifting (allowed, paid, denied, adjusted)
* denial reason code interpretation

This plugin is the first domain plugin, but the architecture supports any domain: financial (SWIFT, FIX messages), telecom (CDRs), IoT (sensor payloads), etc.

---

# 19. Explainability

Vajra must not emit magic. Every conclusion must be traceable to its evidence.

## 19.1 Per-Observation Explanation

Every observation in the essence carries:

* **score** — the composite importance score
* **contributing signals** — which of the six dimensions contributed, with values
* **evidence paths** — the specific JSON paths that triggered this observation
* **profile influence** — how the concern profile affected the ranking

## 19.2 Explanation Verbosity

Controlled by the `--explain` flag:

* **off (default):** observations are presented without score details
* **on:** each observation includes its score decomposition

And by the profile's vocabulary setting:

* **plain:** "3 service lines are missing allowed amounts"
* **technical:** "paths $.claims[*].service_lines[2,7,11].allowed_amount: null_rate=0.21, expected_presence=0.99, anomaly_score=4.2 (rarity=0.3, instability=0.0, anomaly=0.8)"
* **formal:** "Observations 2, 7, and 11 in the service line array exhibit absent allowed_amount fields. The expected presence rate for this field across the batch is 99%. This absence pattern contributes an anomaly score of 4.2."

---

# 20. Configuration

## 20.1 Format

TOML. Located at `~/.vajra/config.toml` or specified via `--config`.

## 20.2 Schema

```toml
[parsing]
max_depth = 256
max_string_length = 10_485_760    # 10MB
max_document_size = 10_737_418_240 # 10GB
streaming_threshold = 104_857_600  # 100MB — auto-switch to streaming above this

[analysis]
cms_error_rate = 0.001
cms_failure_probability = 0.01
ddsketch_relative_accuracy = 0.01
minhash_num_hashes = 128
lsh_bands = 16
lsh_rows_per_band = 8
top_k_fields = 50                  # for cross-field analysis
min_observations = 30              # minimum count before computing statistics

[anomaly]
mad_threshold = 3.5
rarity_threshold_sigma = 2.0
structural_distance_threshold = 0.2
type_instability_threshold = 0.01

[redaction]
enabled = false
patterns = ["SSN", "email", "phone", "credit_card"]
custom_patterns = []

[plugins]
enabled = []
plugin_dir = "~/.vajra/plugins"

# Custom profiles defined here (see section 9.2)
```

## 20.3 Override Precedence

1. CLI flags (highest)
2. Environment variables (`VAJRA_*`)
3. Config file specified via `--config`
4. User config at `~/.vajra/config.toml`
5. Built-in defaults (lowest)

---

# 21. Testing Strategy

## 21.1 Unit Tests

Per primitive, per crate:

* path extraction: known documents → expected path tries
* canonicalization: known inputs → expected canonical forms
* entropy: known distributions → expected entropy values
* CMS: known streams → frequency estimates within error bounds
* DDSketch: known distributions → quantile estimates within relative error
* MAD: known distributions → expected MAD and z-scores
* MinHash: known sets → similarity estimates within expected variance
* JSD: known distributions → expected divergence values
* BLAKE3: known inputs → expected hashes (determinism)

## 21.2 Property Tests

Using `proptest` or `quickcheck`:

* **canonicalization idempotence:** canonicalize(canonicalize(x)) == canonicalize(x)
* **fingerprint stability:** reordering keys does not change the path set fingerprint
* **Merkle hash determinism:** two structurally identical subtrees always produce the same hash
* **drift symmetry:** drift(A, B).structural_changes == drift(B, A).structural_changes (with direction inverted)
* **MinHash accuracy:** over many random set pairs, estimated Jaccard converges to true Jaccard within theoretical bounds
* **DDSketch guarantees:** for any quantile q, |estimated - true| ≤ α × |true|
* **scoring determinism:** same document × same profile → same scores, same ordering

## 21.3 Fuzzing

Using `cargo-fuzz` and `AFL`:

* malformed JSON (truncated, unbalanced, invalid UTF-8)
* deeply nested JSON (depth 10,000+)
* extremely wide objects (100,000+ keys)
* pathologically repetitive arrays (1M identical elements)
* type chaos (same path alternates types across records)
* adversarial strings (null bytes, multi-byte Unicode, RTL markers, control characters)
* near-maximum-size documents (at streaming threshold boundary)

**Target:** zero panics, zero undefined behavior, graceful error on all inputs.

## 21.4 Differential Testing

* two parsing modes (DOM vs. streaming) on the same input must produce identical statistics
* exact frequency counts vs. CMS estimates: CMS must be within proven error bounds
* exact quantiles vs. DDSketch estimates: DDSketch must be within relative accuracy

## 21.5 Determinism Tests

For a corpus of test documents:

1. Run Vajra N times (N ≥ 10) with identical config.
2. Assert byte-identical output across all runs.
3. Run with `--seed 0` and `--seed 42` — outputs may differ.
4. Run each seed N times — assert identical within-seed output.

## 21.6 Golden Tests

For each profile × format combination, maintain a set of golden output files. These are versioned in the repository and updated explicitly (never auto-updated). CI fails if output diverges from golden files.

This catches: rendering regressions, ordering instabilities, score drift from algorithm changes.

## 21.7 Mutation Testing

Using `cargo-mutants`:

Ensure test suite catches: missing field logic, wrong path ranking, unstable ordering, false anomaly suppression, off-by-one in thresholds.

Target: mutation score > 85%.

## 21.8 Benchmark Tests

Using `criterion`:

* parsing throughput (MB/s) for DOM and streaming modes
* single-document analysis latency at 1KB, 1MB, 100MB, 1GB
* batch clustering throughput at 100, 1K, 10K documents
* fingerprint comparison throughput (pairs/second)

Benchmarks are tracked in CI. Regressions > 10% fail the build.

---

# 22. Success Metrics

These define what "working" means for Vajra.

## 22.1 Correctness

* zero panics on any valid or invalid JSON input (validated by fuzzing)
* 100% determinism (validated by determinism tests)
* all statistical estimates within proven error bounds (validated by differential tests)
* mutation test score > 85%

## 22.2 Performance

* meets latency targets in section 14.1 on CI benchmark hardware
* streaming mode memory usage ≤ 50MB for any document size with default config

## 22.3 Essence Quality

* **compression ratio:** for documents > 10KB, essence is < 20% of original token count while preserving all anomalies and top structural observations
* **anomaly recall:** on a curated test corpus with planted anomalies, Vajra detects > 90% of planted anomalies
* **false positive rate:** on a curated test corpus of clean data, < 5% of observations are flagged as anomalous

## 22.4 Adoption

* CLI installs with a single command (cargo install, Homebrew, or binary download)
* first useful output within 30 seconds of installation (no config required)
* library integrates with `cargo add vajra-core vajra-essence` and < 20 lines of code for basic use

---

# 23. Risks and Mitigations

| Risk | Impact | Likelihood | Mitigation |
|---|---|---|---|
| **Essence quality is subjective** — different users disagree on what matters | Users don't trust output | High | Concern profiles with tunable weights. Golden tests anchored to user feedback. Explain mode for transparency. |
| **Streaming mode loses fidelity** — some analyses degrade without full DOM | Second-class experience on large documents | Medium | Explicit fidelity labels in output. Two-pass streaming for critical analyses. Document which analyses require DOM. |
| **Scoring weights are wrong** — default profiles emphasize the wrong signals | Essences bury important information | Medium | Start with conservative defaults (section 9.1). Tune via user studies on real data. Make all weights visible and overridable. |
| **False anomalies on unusual but valid data** — legitimate variation flagged | Users learn to ignore anomalies | Medium | MAD's 50% breakdown point helps. Conservative thresholds by default. Profile-specific tuning. Honesty labels on all flags. |
| **Performance targets missed on pathological input** — 1M distinct paths, 100-level nesting | Vajra appears slow or unresponsive | Low | Configurable limits. Graceful degradation with diagnostics. Benchmark corpus includes pathological cases. |
| **Plugin interface too restrictive** — domain needs that can't be expressed | Domain plugins feel limited | Medium | Start with the medical plugin to validate the interface before stabilizing it. Iterate the trait before v1.0. |
| **Scope creep toward query engine** — users want transformation, not just analysis | Vajra tries to be jq and loses identity | High | Hard boundary: Vajra reads and analyzes, never rewrites source data. Query model is read-only by design. |

---

# 24. Phased Delivery Plan

## Phase 1 — Core Engine (MVP)

**Goal:** prove the thesis. A single command produces a useful, deterministic essence from any JSON document.

Deliverables:

1. `vajra-core`: simd-json parsing (DOM mode), RFC 8785 canonicalization, DFS path extraction, path trie
2. `vajra-types`: shared types, feature store, result contracts
3. `vajra-fingerprint`: BLAKE3 path set fingerprint, typed path fingerprint, Merkle subtree hashing
4. `vajra-stats`: Shannon entropy, exact frequency counting (CMS deferred to Phase 2), MAD, basic numeric stats (min/max/mean/median/percentiles via sorting)
5. `vajra-anomaly`: MAD-based numeric outliers, rarity scoring, type instability detection
6. `vajra-essence`: engineer and staff profiles, text and json output, basic motif collapsing
7. `vajra-cli`: `inspect`, `essence`, `anomalies`, `stats`, `fingerprint` commands

**Not in Phase 1:** streaming mode, drift detection, batch analysis, clustering, query model, plugins, redaction, DDSketch/CMS (exact counting sufficient at this scale).

**MVP validation:** run Vajra on 50 real-world JSON documents from 5+ domains. Essences must be judged useful by at least one person per domain.

## Phase 2 — Scale and Comparison

**Goal:** handle any size and compare across documents.

Deliverables:

1. Streaming parser integration (two-pass hybrid strategy)
2. DDSketch for streaming numeric analysis
3. CMS with conservative update for streaming frequency
4. Space-Saving for top-k in streaming mode
5. `vajra-drift`: JSD, 1D Wasserstein, path diff, drift classification
6. MinHash + LSH for batch similarity and clustering
7. Cross-field relationship discovery (conditional entropy, PMI)
8. Batch mode with parallel processing and sketch merging
9. `vajra-cli`: `drift`, `cluster`, `invariants` commands
10. Auditor and AI profiles

## Phase 3 — Domain Intelligence

**Goal:** make Vajra deeply useful in specific domains without compromising universality.

Deliverables:

1. Plugin system with trait interface, error isolation, and loading
2. `vajra-domain-med`: medical/EDI type recognizers, claim motif hinting, terminology lifting
3. Custom profile definition via TOML
4. Redaction system with built-in and plugin-extensible patterns
5. Fraud profile with Benford's Law analysis
6. Temporal pattern analysis module

## Phase 4 — AI-First Workflows

**Goal:** make Vajra the standard preprocessing step for feeding JSON to LLMs.

Deliverables:

1. `compact-ai` output format with aggressive motif compression
2. Token budget awareness (`--budget`) with greedy knapsack selection
3. Structured chunking: split large essences into LLM-friendly chunks that preserve structural context
4. Chain-ready output: essences that include enough metadata for an LLM to request deeper analysis on specific paths
5. Query model with analysis functions
6. Markdown output format for documentation workflows

---

# 25. Example End-to-End Workflows

## Scenario 1: Non-Technical Staff

A claims processor has a large medical JSON and needs to understand it.

```bash
vajra essence claim.json --profile staff --format markdown
```

```markdown
## Document Summary
- 1 claim with 14 service lines
- 1 patient, 1 subscriber, 2 diagnosis codes
- Primary status: partially adjudicated

## What Stands Out
- 3 service lines are missing allowed amounts
  (lines 2, 7, 11 — this field is present in 99% of service lines typically)
- 1 diagnosis structure differs from the others
  (the second diagnosis has an extra "qualifier" field not seen in the first)
- Provider identifier is present but taxonomy code is missing
- Adjustment reason code "CO-45" repeats across 8 of 14 lines

## What This Likely Means
- Most of the claim structure is consistent and well-formed
- A subset of service lines looks incomplete or differently processed
- The repeated adjustment code suggests a systematic issue, not random errors
- This may need review before handing to an auditor or AI workflow
```

## Scenario 2: AI Pipeline

An automation system needs compact context for an LLM.

```bash
vajra essence claim.json --profile ai --format compact-ai --budget 500
```

```json
{
  "vajra_essence": {
    "version": "0.1.0",
    "profile": "ai",
    "input_hash": "b3a7...",
    "structure": {
      "root_type": "object",
      "total_nodes": 847,
      "distinct_paths": 23,
      "max_depth": 6
    },
    "dominant_motif": {
      "path": "$.claims[0].service_lines[*]",
      "count": 14,
      "shape_hash": "f2c1...",
      "fields": ["procedure_code", "service_date", "charge_amount", "allowed_amount", "status", "adjustment"]
    },
    "anomalies": [
      {"path": "$.claims[0].service_lines[2,7,11].allowed_amount", "type": "missing", "severity": 4.2},
      {"path": "$.claims[0].diagnosis[1]", "type": "structural_deviation", "severity": 3.1}
    ],
    "notable": [
      {"path": "$.claims[0].service_lines[*].adjustment.reason_code", "observation": "value 'CO-45' in 8/14 instances"}
    ]
  }
}
```

## Scenario 3: Drift Detection

An engineer checks whether today's API response matches yesterday's shape.

```bash
vajra drift yesterday.json today.json --profile engineer --format text
```

```text
Drift Report: yesterday.json -> today.json
Structural similarity: 0.94 (Jaccard)

Added paths (2):
  $.response.metadata.processing_flags    [array of strings]
  $.response.metadata.api_version         [string]

Removed paths (0): none

Type changes (1):
  $.response.items[*].quantity            string -> number (JSD: 0.0, clean type migration)

Distribution shifts (1):
  $.response.items[*].status              JSD: 0.34 (moderate)
    before: {"active": 0.82, "pending": 0.15, "error": 0.03}
    after:  {"active": 0.61, "pending": 0.12, "error": 0.27}
    note: "error" rate increased 9x

Overall severity: MEDIUM (structural additions + significant distribution shift in status field)
```

---

# 26. Product Identity

Vajra is not a viewer.
Vajra is not a formatter.
Vajra is a forge.

Raw JSON enters as structure.
It leaves as:

* shape
* signal
* anomalies
* essences
* explainable conclusions

It is closer to:

* **structured-data observability**
* **semantic reduction**
* **operational cognition tooling**

than to anything in the current JSON tooling landscape.

**Vajra — Break noise. Preserve truth.**

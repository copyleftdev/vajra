# Information Theory

Vajra's analytical core is an information-theoretic pipeline. Every measure of diversity, anomaly, drift, similarity, and dependency traces back to a concept from information theory. This chapter covers the full stack — from foundational primitives through composite metrics to the scoring model that turns bits into insights.

---

## Foundation: Shannon Entropy

The starting point. Shannon entropy measures the average surprise per observation:

```
H(X) = -sum p(x) * log2(p(x))
```

- **0 bits**: constant field (no information)
- **log2(k) bits**: uniform distribution over k values (maximum information)
- **Between**: the interesting space where identifiers, dates, and codes live

**Normalized entropy** scales to [0, 1]:

```
H_norm(X) = H(X) / log2(|support|)
```

This is the single most important signal in Vajra. A field with H_norm near 0 is noise. A field with H_norm near 1 is unstructured randomness. Meaningful variation lives in the middle.

**Files:** `vajra-stats/src/entropy.rs`

---

## The Renyi Spectrum

Shannon entropy is one point on a continuous family parameterized by alpha:

```
H_alpha(X) = (1 / (1 - alpha)) * log2(sum p(x)^alpha)
```

| alpha | Name | What It Measures |
|-------|------|-----------------|
| 0 | Hartley | `log2(support size)` — how many distinct values exist |
| 1 | Shannon | average surprise (limit as alpha approaches 1) |
| 2 | Collision | `-log2(sum p^2)` — probability two random draws match |
| infinity | Min-entropy | `-log2(max p)` — worst-case unpredictability |

**Why a spectrum?** A single entropy number hides the shape of the distribution. The Renyi spectrum reveals it:

- **High Shannon, low min-entropy**: long tail with one dominant value
- **All orders equal**: near-uniform distribution
- **Large divergence (H0 - H_inf)**: heavy concentration with many rare values

**Security application:** Min-entropy is the correct measure for cryptographic key strength — not Shannon. A key with high Shannon but low min-entropy has a predictable most-likely value.

**Spectrum divergence** (H0 - H_inf) is itself a signal: it quantifies how far the distribution is from uniform. Zero divergence = uniform. High divergence = concentrated.

**Complexity:** O(n), same as Shannon. Computed from the same frequency counts.

**Files:** `vajra-stats/src/renyi.rs`

---

## Structural Complexity: Lempel-Ziv

Shannon entropy measures **average information per symbol**. It cannot distinguish:

| Input | Shannon Entropy | LZ Complexity |
|-------|----------------|---------------|
| Random UUIDs | High | **High** |
| `PROJ-001`, `PROJ-002`, ... | High | **Low** |
| Repeated `"active"` | Low | Low |

Lempel-Ziv complexity (LZ76) measures the **number of distinct subpatterns** needed to describe a sequence. The LZ76 algorithm scans left-to-right, extending the current phrase until it hasn't been seen before:

```
Normalized C_LZ = phrase_count / (n / log2(n))
```

**The entropy-complexity plane** has four quadrants:

| | Low LZ | High LZ |
|---|--------|---------|
| **High entropy** | **Structured** (patterned identifiers) | **Random** (UUIDs, hashes) |
| **Low entropy** | **Constant** (repeated values) | **Anomalous** (theoretically unlikely) |

A field in the "structured" quadrant (high entropy, low complexity) is a generated identifier with a pattern. A field in the "random" quadrant is truly unpredictable. Shannon alone cannot tell them apart.

**Complexity:** O(n) single pass. No external dependencies.

**Files:** `vajra-stats/src/lz_complexity.rs`

---

## Relationships: Conditional Entropy and PMI

### Conditional Entropy H(Y|X)

How much knowing X reduces uncertainty about Y:

```
H(Y|X) = -sum p(x,y) * log2(p(y|x))
```

- **H(Y|X) = 0**: X completely determines Y (functional dependency)
- **H(Y|X) = H(Y)**: X tells you nothing about Y (independence)

**Relationship strength** normalizes this:

```
strength = 1 - H(Y|X) / H(Y)
```

Clamped to [0, 1]. Zero = independent. One = deterministic.

### Pointwise Mutual Information

Measures co-occurrence strength between specific value pairs:

```
PMI(x, y) = log2(P(x,y) / (P(x) * P(y)))
```

Positive = co-occur more than chance. Negative = avoid each other. Zero = independent.

### Total Correlation

Pairwise measures miss **higher-order structure**. Three fields can be independent in pairs but jointly constrained (city + state + zip). Total correlation captures this:

```
TC(X1,...,Xn) = sum H(Xi) - H(X1,...,Xn)
```

- **TC = 0**: all fields are independent
- **High TC**: the schema has deep internal structure
- **TC / sum H(Xi)**: normalized to [0, 1]

Total correlation answers: "how much redundancy exists across all fields simultaneously?" This is the gap between pairwise analysis and true multivariate dependency.

**Complexity:** O(n) for marginals. Joint entropy estimated via binning, bounded to 8-field subsets for tractability.

**Files:** `vajra-stats/src/relationships.rs`, `vajra-stats/src/total_correlation.rs`

---

## Distributional Drift: JSD and Wasserstein

### Jensen-Shannon Divergence

Symmetric, bounded, and a proper metric (via square root):

```
JSD(P, Q) = 0.5 * KL(P || M) + 0.5 * KL(Q || M)
```

where M = 0.5 * (P + Q) and KL is Kullback-Leibler divergence.

- JSD in [0, 1] with log base 2
- sqrt(JSD) satisfies the triangle inequality (Endres & Schindelin 2003)
- Used for **categorical** distribution drift

### 1D Wasserstein Distance

For numeric distributions, measures the "earth mover's distance":

```
W1 = integral |CDF_a(x) - CDF_b(x)| dx
```

JSD tells you the distributions changed. Wasserstein tells you **by how much** — it captures the magnitude of the shift, not just its existence.

**When to use which:**

| Data Type | Metric | Why |
|-----------|--------|-----|
| Categorical (strings, enums) | JSD | Probability mass redistribution |
| Numeric (amounts, counts) | Wasserstein | Shift magnitude in original units |

**Files:** `vajra-drift/src/jsd.rs`, `vajra-drift/src/wasserstein.rs`

---

## Directed Information Flow: Transfer Entropy

Transfer entropy measures **how much knowing the past of X reduces uncertainty about Y's future, beyond what Y's own past already tells you**:

```
TE(X->Y) = H(Y_t | Y_{t-1}^k) - H(Y_t | Y_{t-1}^k, X_{t-1}^l)
```

Key properties:

- **Directional:** TE(X->Y) != TE(Y->X) — reveals causal flow
- **Non-negative:** information can only help prediction
- **Granger causality generalized:** captures nonlinear dependencies

This transforms cascade detection from temporal pattern matching into rigorous directed information flow quantification. Instead of "A happened before B," transfer entropy says "A's past carries 2.3 bits of information about B's future that B's own history doesn't contain."

**Net information flow** = TE(X->Y) - TE(Y->X). Positive means X drives Y. Negative means Y drives X.

**Complexity:** O(n * k) where k is history depth. Deterministic with fixed binning.

**Files:** `vajra-stats/src/transfer_entropy.rs`

---

## Universal Similarity: NCD

Normalized Compression Distance approximates the normalized information distance — provably the most general similarity metric:

```
NCD(x, y) = (C(xy) - min(C(x), C(y))) / max(C(x), C(y))
```

where C is a real compressor (zstd at fixed level 3).

**Why NCD is strictly more powerful than feature-based similarity:** MinHash captures set overlap. SimHash captures angular proximity. Both require choosing features. NCD captures **all computable regularities** — structure, patterns, naming conventions, content — with zero feature engineering.

Two documents that share structural patterns but zero literal values will have low NCD. Two documents with random shared tokens but different structure will have high NCD.

- NCD(x, x) approaches 0 (self-similarity)
- NCD(x, random) approaches 1 (dissimilarity)
- Symmetric: NCD(x, y) = NCD(y, x)
- Deterministic given fixed compressor and level

**Complexity:** O(n) per compression. O(n^2) for all-pairs matrix with C(x) caching.

**Files:** `vajra-fingerprint/src/ncd.rs`

---

## Anomaly Scoring

### Self-Information (Surprisal)

The rarity of a single observation:

```
I(x) = -log2(p(x))
```

A value seen once in 10,000 observations carries 13.3 bits of rarity. This is the information-theoretic foundation of rare value detection.

### MAD-Based Outlier Detection

Median Absolute Deviation with modified z-scores:

```
z_MAD = 0.6745 * (x - median) / MAD
```

Values with |z_MAD| > 3.5 are flagged. MAD has a 50% breakdown point — half the data can be corrupted before it gives misleading results.

### Benford's Law

Leading digit distribution for numeric fields:

```
P(d) = log10(1 + 1/d)
```

Conformity tested via chi-squared and Nigrini's MAD score. Non-conformity (MAD > 0.015) signals potentially fabricated or unusual numeric data.

---

## The Six-Dimensional Scoring Model

Every observation is scored across six information-theoretic dimensions:

| Dimension | Source | Range |
|-----------|--------|-------|
| **rarity** | Self-information, cardinality | [0, 1] |
| **instability** | Type distribution: 1 - (dominant/total) | [0, 1] |
| **entropy_signal** | Normalized Shannon entropy | [0, 1] |
| **structural_coverage** | Null rate, enum-like patterns | [0, 1] |
| **anomaly_strength** | MAD z-scores, rarity magnitude | [0, 1] |
| **concern_relevance** | Domain-specific importance | [0, 1] |

The composite score is a weighted sum:

```
score = sum weight_i * dimension_i
```

Weights depend on the concern profile:

| Profile | Rarity | Instability | Entropy | Coverage | Anomaly | Concern |
|---------|--------|-------------|---------|----------|---------|---------|
| Engineer | 0.15 | 0.15 | 0.15 | 0.15 | 0.15 | 0.15 |
| Staff | 0.10 | 0.10 | 0.10 | 0.25 | 0.30 | 0.15 |
| Fraud | 0.25 | 0.10 | 0.10 | 0.10 | 0.35 | 0.10 |

---

## The Integration Pipeline

```
JSON Document
  |
  v
[Stats Analyzer] --- entropy, Renyi spectrum, LZ complexity, cardinality, rarity
  |
  v
[Anomaly Analyzer] --- rare values (surprisal), type instabilities, MAD outliers
  |
  v
[Relationship Discovery] --- conditional entropy, PMI, total correlation
  |
  v
[Drift Analyzer] --- JSD (categorical), Wasserstein (numeric), severity
  |
  v
[Cascade Analyzer] --- transfer entropy, directed information flow
  |
  v
[Feature Store] --- PathFeatures with all information-theoretic signals
  |
  v
[Essence Builder] --- ScoredObservations across 6 dimensions
  |
  v
[Profile Scorer] --- Weighted composite score
  |
  v
[EssenceData] --- Prioritized findings for humans and AI
```

Every anomaly signal, every drift measurement, every relationship discovery, and every cascade detection is rooted in information theory. The entire system is fundamentally an information-theoretic lens on structured data.

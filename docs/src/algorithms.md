# Algorithms

This is the technical heart of Vajra. Every algorithm here was selected against three gates. Any algorithm that failed any gate was cut.

---

## The Three Gates

### Gate 1: Scale

O(n) or O(n log n) time complexity. Bounded or streaming-compatible memory. If an algorithm cannot handle a billion nodes without choking, it does not enter.

### Gate 2: Battle-Tested

Published, peer-reviewed, deployed in production systems at scale. No novel algorithms. No research prototypes. No "clever tricks" that have not survived contact with real data.

### Gate 3: Deterministic

Same input, same output. If an algorithm requires random sampling without seed control, or produces platform-dependent results, or depends on iteration order of an unordered collection — it does not enter.

---

## The Algorithms

### BLAKE3 Hashing

**Provenance:** O'Connor, Aumasson, Neves, Wilcox-O'Hearn, 2020. Rust-native reference implementation.

**What it does:** All hashing in Vajra. Path set fingerprints, typed path fingerprints, Merkle subtree hashing, content hashing, MinHash hash functions.

**Why BLAKE3 over alternatives:**

| Contender | Why Rejected |
|---|---|
| SHA-256 | 3-7x slower on modern hardware. No parallelism. |
| SHA-3 | Slower than BLAKE3 on all platforms. No parallel tree structure. |
| xxHash / FNV | Not cryptographic. Collision resistance matters for fingerprinting. |
| SipHash | Designed for hash tables, not content addressing. Slower for bulk data. |

**Why BLAKE3 wins:** 3-7x faster than SHA-256. Internally parallelizable via Bao tree structure. 256-bit output with cryptographic strength. Rust-native. Deterministic. One algorithm for every hashing need in the system.

**Complexity:** O(n) time, O(1) memory per hash. Internally parallel for large inputs.

---

### simd-json Parsing

**Provenance:** Langdale & Lemire, 2019. Based on the simdjson C++ library, ported to Rust.

**What it does:** DOM-mode JSON parsing at 2+ GB/s using SIMD instructions for structural character classification and string validation.

**Why simd-json over alternatives:**

| Contender | Why Rejected |
|---|---|
| serde_json | 400-800 MB/s. Adequate but not operational speed for large documents. |
| sonic-rs | Young ecosystem. Less battle-tested. |
| Manual SAX parser | Necessary for streaming mode, but DOM mode needs random access. |

**Why simd-json wins:** Measured 2+ GB/s on modern hardware. Uses SIMD for structural indexing. Operates on mutable borrowed slices for zero-copy access to string values.

**Complexity:** O(n) time. O(n) memory for the DOM.

---

### RFC 8785 Canonicalization (JCS)

**Provenance:** IETF RFC 8785, published 2020. The standard for deterministic JSON serialization.

**What it does:** Removes irrelevant representational variance. Lexicographic key ordering by UTF-16 code unit sequence. Deterministic number formatting. No whitespace.

**Extensions beyond the RFC:**

- Unicode NFC normalization (UAX #15) for string comparison stability
- Null vs. absent distinction preservation (RFC 8785 does not address this; Vajra tracks it explicitly)
- Configurable array order policy: preserve (default), set (unordered deduplicated), multiset (unordered with duplicates)

**Complexity:** O(n log k) where k = maximum keys per object. Memory: O(n).

---

### Shannon Entropy

**Provenance:** Shannon, 1948. The foundation of information theory. Sixty-eight years of deployment in every field that measures information.

**What it does:** For each path, measures the information content of observed values.

```
H(X) = -sum p(x) * log2(p(x))
```

Normalized:

```
H_norm(X) = H(X) / log2(|support|)
```

**Why entropy is the strongest universal primitive:** It distinguishes boilerplate from signal without domain knowledge. A constant field (H = 0) is noise. A uniform random field (H_norm = 1) is unstructured. Meaningful variation lives in the middle — identifiers, dates, codes, status values.

**Streaming computation:** Maintained via running counts per value per path. When the value space exceeds memory, entropy is estimated from Count-Min Sketch frequency approximations.

**Complexity:** O(n) time, O(v) space where v = distinct values per path.

---

### Count-Min Sketch (CMS) with Conservative Update

**Provenance:** Cormode & Muthukrishnan, 2005 (original CMS). Conservative update: Estan & Varghese, 2002.

**What it does:** Streaming frequency estimation when exact counts would exceed memory. Maintains a 2D array of counters with multiple hash functions.

**Conservative update improvement:** Instead of incrementing all d counters, only increment counters currently equal to the minimum. Provably reduces over-estimation error without changing the data structure.

**Parameters:**

- Width w = ceil(e / epsilon) where epsilon = desired error rate (default 0.001)
- Depth d = ceil(ln(1 / delta)) where delta = failure probability (default 0.01)
- Default: w = 2,718, d = 5

**Guarantees:** Estimated count satisfies: true count <= estimate <= true count + epsilon * N with probability >= 1 - delta, where N = total count.

**What it replaced:**

| Contender | Why Rejected |
|---|---|
| Exact hash maps | Unbounded memory on high-cardinality paths. |
| Bloom filters | Cannot count — only membership testing. |
| Count Sketch | Returns negative estimates. CMS guarantees non-negative. |

**Complexity:** O(d) per update. O(w * d) memory. Both constants independent of data size.

---

### Space-Saving Algorithm

**Provenance:** Metwally, Agrawal, El Abbadi, 2005.

**What it does:** Identifies the top-k most frequent elements in a stream using exactly k counters.

**Mechanism:** When a new element arrives that is not tracked, evict the element with the smallest count, replace it, and increment. Despite its simplicity, every element whose true frequency exceeds N/k is guaranteed to be in the summary.

**What it replaced:**

| Contender | Why Rejected |
|---|---|
| Frequent algorithm (Misra-Gries) | Weaker error bounds for the same space. |
| Lossy Counting | Higher space complexity. More complex implementation. |
| Full sorting | O(n log n) and requires all data in memory. |

**Complexity:** O(1) amortized per update with a min-heap. O(k) memory.

---

### DDSketch

**Provenance:** Masson, Rim, Lee, 2019. Developed at Datadog. Deployed in production across billions of data points per second.

**What it does:** Streaming quantile estimation with **relative error guarantees**. For any quantile q, the returned value satisfies |estimate - true| <= alpha * |true| where alpha is the relative accuracy parameter.

**Why DDSketch over alternatives:**

| Contender | Why Rejected |
|---|---|
| t-digest (Dunning, 2019) | No formal error guarantees. Accuracy is empirically good but theoretically unbounded. |
| Fixed-width histograms | Absolute error is meaningless when values span orders of magnitude (cents to millions). |
| Random sampling | No guarantees on tail quantiles — precisely where anomalies live. |
| GK sketch (Greenwald-Khanna) | Provides absolute error, not relative. DDSketch adapts to data scale. |

**Critical property: mergeability.** DDSketch instances can be merged exactly, preserving accuracy guarantees. This enables parallel batch processing: analyze partitions independently, merge sketches for global statistics with zero accuracy loss.

**Parameters:** alpha = 0.01 (1% relative error) by default. Memory: O(log(max/min) / log(1 + alpha)) buckets — typically a few hundred for financial data spanning cents to millions.

**Complexity:** O(1) per insertion. O(1) per quantile query.

---

### Median Absolute Deviation (MAD)

**Provenance:** Hampel, 1974. Standard robust statistics.

**What it does:** Robust measure of dispersion.

```
MAD = median(|x_i - median(X)|)
```

Modified z-score:

```
z_MAD = 0.6745 * (x_i - median(X)) / MAD
```

**Why MAD over standard deviation:** Standard deviation has a 0% breakdown point — a single extreme value inflates sigma enough to mask every other anomaly. MAD has a **50% breakdown point** — half the data can be arbitrarily corrupted before MAD gives a misleading result. This is the strongest possible breakdown point for any location/scale estimator.

**Anomaly threshold:** |z_MAD| > 3.5 flags an anomaly candidate (Iglewicz & Hoaglin, 1993). Configurable per profile.

**Streaming computation:** Exact MAD requires sorted data. Running approximate median via DDSketch enables streaming MAD estimation with bounded relative error.

**Complexity:** O(n) with sorting, or O(n) streaming via DDSketch.

---

### MinHash

**Provenance:** Broder, 1997. The foundation of modern near-duplicate detection and similarity search.

**What it does:** Estimates Jaccard similarity between sets in constant time per comparison.

Vajra computes MinHash signatures over wildcard path sets using k independent hash functions (k = 128 by default). For memory efficiency, the **b-bit MinHash** variant (Li & Konig, 2011) stores only the lowest b bits of each hash value.

**What it replaced:**

| Contender | Why Rejected |
|---|---|
| Exact pairwise Jaccard | O(n^2) for batch comparison. Fine for < 1,000 docs, breaks above that. |
| Random projection (SimHash) | Better for cosine similarity. MinHash is optimal for Jaccard. |

**Complexity:** O(n) for signature computation. O(k) for pairwise comparison. O(k) memory per document.

---

### SimHash

**Provenance:** Charikar, 2002.

**What it does:** Fixed-width fingerprints where Hamming distance approximates cosine distance. Used for near-motif detection — subtrees that are semantically the same but differ in one or two fields.

SimHash operates over (key, value_type) feature pairs within each subtree. Subtrees whose SimHash values have Hamming distance <= t (default t = 3 out of 64 bits) are grouped as near-motifs.

**Complexity:** O(n) time. O(1) per fingerprint comparison.

---

### Locality-Sensitive Hashing (LSH)

**Provenance:** Indyk & Motwani, 1998. Banded variant as described by Leskovec, Rajaraman & Ullman.

**What it does:** Partitions MinHash signatures into b bands of r rows, hashing each band into buckets. Documents sharing a bucket in any band are candidate pairs.

The S-curve probability: P(candidate) = 1 - (1 - s^r)^b

With k = 128, b = 16, r = 8: documents with Jaccard similarity > 0.5 have > 98% chance of being found. Documents with similarity < 0.2 have < 2% false positive rate.

**What it replaced:**

| Contender | Why Rejected |
|---|---|
| Hierarchical agglomerative clustering | O(n^2 log n) time, O(n^2) memory. Breaks on > 10K documents. |
| k-means / k-medoids | Requires specifying k in advance. Vajra cannot know the cluster count. |

**Complexity:** O(n) amortized for LSH indexing.

---

### Jensen-Shannon Divergence (JSD)

**Provenance:** Lin, 1991. Square root metric property: Endres & Schindelin, 2003; Osterreicher & Vajda, 2003.

**What it does:** Measures distributional drift between two value distributions.

```
JSD(P || Q) = 0.5 * KL(P || M) + 0.5 * KL(Q || M)
```

where M = 0.5 * (P + Q).

**Why JSD over alternatives:**

| Contender | Why Rejected |
|---|---|
| KL divergence | Asymmetric. Infinite when Q(x) = 0 and P(x) > 0. |
| Chi-squared test | Sensitive to bin size. Not a proper metric. |
| Kolmogorov-Smirnov | Measures maximum deviation only, not overall distribution shape. |
| Total variation distance | Does not account for similarity between nearby values. |

**Why JSD wins:** Symmetric. Always finite. Bounded to [0, 1] with log base 2. Square root is a proper metric satisfying the triangle inequality. Drift magnitudes can be meaningfully compared.

**Complexity:** O(v) where v = union of value supports.

---

### 1D Wasserstein Distance (Earth Mover's Distance)

**Provenance:** Kantorovich, 1942. Computational formulation: O(n log n) via CDF sorting.

**What it does:** For numeric paths, measures "how far did values move" — not just that the distribution changed, but the magnitude of the shift.

**Why included alongside JSD:** JSD measures probability mass redistribution. Wasserstein captures the *distance* of the shift. A distribution that shifts entirely from $100 to $100.01 has low Wasserstein but potentially high JSD. A distribution that shifts from $100 to $10,000 has high Wasserstein. Both measures together give a complete picture.

**Complexity:** O(n log n) via sorting.

---

### Pointwise Mutual Information (PMI)

**Provenance:** Church & Hanks, 1989 (in NLP). Rooted in Shannon, 1948.

**What it does:** Measures association strength between field pairs.

```
PMI(x, y) = log2(P(x, y) / (P(x) * P(y)))
```

Positive = co-occur more than chance. Negative = avoid each other. Zero = independent.

**Complexity:** O(1) per pair given precomputed frequencies.

---

### Conditional Entropy

**Provenance:** Shannon, 1948.

**What it does:** Measures how much knowing field X reduces uncertainty about field Y.

```
H(Y|X) = -sum p(x,y) * log2(p(y|x))
```

Low H(Y|X) means X predicts Y. H(Y|X) near 0 means functional dependency.

**Complexity:** O(n) to compute from co-occurrence counts.

---

### Benford's Law Analysis

**Provenance:** Newcomb, 1881. Benford, 1938. Formalized by Hill, 1995. Forensic application: Nigrini, 1996.

**What it does:** Tests whether leading digit distribution matches the expected logarithmic distribution:

```
P(d) = log10(1 + 1/d)
```

Departure measured via chi-squared goodness-of-fit. Effective for financial amounts, counts, and quantities. Not applicable to identifiers, codes, or constrained-range values — Vajra applies this only to paths classified as numeric with high cardinality and range spanning at least one order of magnitude.

**Complexity:** O(n) — single pass to count leading digits.

---

## The Rejection List

These algorithms were evaluated and rejected. Each had a specific reason.

| Algorithm | Reason for Rejection |
|---|---|
| **Isolation Forest** (Liu et al., 2008) | Non-deterministic without careful seeding. O(n log n) per tree. Contamination parameter requires tuning. MAD + rarity + structural distance cover the same space with stronger interpretability. |
| **Local Outlier Factor** (Breunig et al., 2000) | O(n^2) naive, O(n log n) with spatial indexing. Sensitive to k parameter. Breaks universality on large datasets. |
| **t-digest** (Dunning, 2019) | No formal error guarantees. Accuracy is empirically good but theoretically unbounded. DDSketch provides provable bounds. |
| **Hierarchical agglomerative clustering** | O(n^2 log n) time, O(n^2) memory. Replaced by LSH-based component detection at O(n). |
| **k-means / k-medoids** | Requires specifying k in advance. Also O(n^2) per iteration for k-medoids. |
| **Any method requiring training data** | Vajra operates on cold data with no prior. Every method must work on a single document or batch with no history. |
| **Any method requiring labeled examples** | Same constraint. Unsupervised only. |
| **Any method without deterministic output** | The determinism guarantee is non-negotiable. |

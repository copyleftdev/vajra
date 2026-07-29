# cluster

`cluster` groups similar documents by structural similarity. Feed it a batch of files and it tells you how many structural families exist, which documents belong to each, and which documents are structural outliers that fit nowhere.

No predefined cluster count. No training. The algorithm discovers the natural grouping from the data.

---

## Usage

```bash
vajra cluster <inputs...> [flags]
```

**Arguments:**

| Argument | Description |
|---|---|
| `<inputs...>` | One or more files, glob patterns, or directories |

**Flags:**

| Flag | Description | Default |
|---|---|---|
| `--format <fmt>` | Output format: `text`, `json`, `markdown`, `compact-ai` | `text` |
| `--input-format <fmt>` | Override auto-detected input format | auto |
| `--lang <lang>` | Source language, with `--input-format source` | auto |
| `--similarity-threshold <f>` | Jaccard similarity above which documents group (0.0–1.0) | `0.5` |
| `--redact` | Apply built-in redaction before output | off |
| `--quiet` | Suppress progress output | off |

When a directory is given, `cluster` reads its `.json` files — or, with `--input-format source`, its source files.

---

## Clustering Source Code

With `--input-format source`, clustering runs over tree-sitter ASTs instead of JSON structure, grouping files that share a syntactic shape regardless of identifier names:

```bash
vajra cluster ./packages --input-format source --lang javascript --similarity-threshold 0.9
```

**Pick the threshold deliberately here.** Source files of the same language share generic AST paths, so Jaccard similarity between any two JavaScript files is high in absolute terms. At the `0.5` default a corpus of unrelated JS will collapse into a single cluster; meaningful families separate in the `0.9`–`0.95` range. Sweep it and watch where clusters stabilise rather than trusting one value.

---

## How It Works

### Small Batches (< 1,000 documents)

Exact pairwise Jaccard similarity over wildcard path sets:

```
J(A, B) = |paths(A) intersection paths(B)| / |paths(A) union paths(B)|
```

O(n^2) pairwise but tractable at small scale. Results are exact and deterministic.

### Large Batches

**MinHash + Locality-Sensitive Hashing (LSH).**

1. During fingerprinting, each document receives a 128-hash MinHash signature.
2. LSH partitions each signature into bands, hashing each band into buckets.
3. Documents sharing a bucket in any band are candidate pairs.
4. Connected components in the candidate graph form initial clusters.
5. Within each component, exact pairwise similarity refines the grouping.

The probability curve is tuned so that documents with Jaccard similarity > 0.5 have > 98% chance of being found as candidates, while documents with similarity < 0.2 have < 2% false positive rate.

This achieves near-linear time clustering: O(n) for MinHash, O(n) amortized for LSH indexing.

---

## Example: Text Output

```bash
vajra cluster claims_batch/*.json
```

```text
=== Cluster Report ===
Documents: 247
Clusters:  3

--- Cluster 0 (198 documents, 80.2%) ---
  Representative: claim_001.json
  Distinct paths: 23
  Structural signature: a1b2c3d4...
  Members: claim_001.json, claim_002.json, claim_003.json, ... (+195 more)

--- Cluster 1 (41 documents, 16.6%) ---
  Representative: claim_048.json
  Distinct paths: 27
  Structural signature: e5f6a7b8...
  Additional paths vs Cluster 0:
    $.claims[*].service_lines[*].modifier_codes
    $.claims[*].rendering_provider
    $.claims[*].rendering_provider.npi
    $.claims[*].rendering_provider.taxonomy
  Members: claim_048.json, claim_052.json, claim_067.json, ... (+38 more)

--- Cluster 2 (8 documents, 3.2%) ---
  Representative: claim_199.json
  Distinct paths: 18
  Structural signature: c9d0e1f2...
  Missing paths vs Cluster 0:
    $.claims[*].subscriber.group_number
    $.claims[*].subscriber.member_id
    $.claims[*].provider.taxonomy
    $.claims[*].service_lines[*].adjustment
    $.claims[*].service_lines[*].adjustment.reason
  Members: claim_199.json, claim_201.json, claim_215.json, ... (+5 more)
  ** Potential structural anomalies — missing common fields **

=== Similarity Matrix (cluster centroids) ===
             Cluster 0  Cluster 1  Cluster 2
  Cluster 0      1.000      0.852      0.783
  Cluster 1      0.852      1.000      0.667
  Cluster 2      0.783      0.667      1.000
```

---

## Example: JSON Output

```bash
vajra cluster claims_batch/*.json --format json
```

```json
{
  "document_count": 247,
  "cluster_count": 3,
  "clusters": [
    {
      "id": 0,
      "size": 198,
      "representative": "claim_001.json",
      "distinct_paths": 23,
      "structural_signature": "a1b2c3d4...",
      "members": ["claim_001.json", "claim_002.json", "..."]
    },
    {
      "id": 1,
      "size": 41,
      "representative": "claim_048.json",
      "distinct_paths": 27,
      "structural_signature": "e5f6a7b8...",
      "additional_paths": [
        "$.claims[*].service_lines[*].modifier_codes",
        "$.claims[*].rendering_provider",
        "$.claims[*].rendering_provider.npi",
        "$.claims[*].rendering_provider.taxonomy"
      ],
      "members": ["claim_048.json", "claim_052.json", "..."]
    },
    {
      "id": 2,
      "size": 8,
      "representative": "claim_199.json",
      "distinct_paths": 18,
      "structural_signature": "c9d0e1f2...",
      "missing_paths": [
        "$.claims[*].subscriber.group_number",
        "$.claims[*].subscriber.member_id",
        "$.claims[*].provider.taxonomy"
      ],
      "members": ["claim_199.json", "claim_201.json", "..."]
    }
  ],
  "similarity_matrix": [
    [1.0, 0.852, 0.783],
    [0.852, 1.0, 0.667],
    [0.783, 0.667, 1.0]
  ]
}
```

---

## Interpreting the Results

**Large dominant cluster + small outlier clusters** is the most common pattern. It means most documents share a structural template, and the outliers represent schema variants, incomplete records, or data from a different source.

**Many clusters of similar size** suggests multiple payload families — perhaps different message types, different API versions, or different upstream sources mixed in a single directory.

**High similarity between clusters** (> 0.8) means the clusters differ by only a few fields. This often indicates optional fields that are sometimes present and sometimes absent.

**Low similarity between clusters** (< 0.5) means fundamentally different structural families. These probably should not be processed by the same pipeline.

---

## When to Use It

- **Batch triage.** Before analyzing 10,000 claims, cluster them to understand how many structural families you are dealing with.
- **Schema variant discovery.** A vendor says they send one format. Clustering reveals three.
- **Outlier isolation.** The smallest cluster often contains the documents with missing fields or unusual structure — the ones that need manual review.
- **Pipeline routing.** Different structural families may need different processing logic. Clustering reveals the routing keys.

---

## Pairs Well With

- [`fingerprint`](./cmd-fingerprint.md) — clustering uses MinHash signatures from the fingerprinting layer
- [`drift`](./cmd-drift.md) — compare cluster representatives to understand how the families differ
- [`anomalies`](./cmd-anomalies.md) — documents in small outlier clusters are strong anomaly candidates
- [`batch`](./cmd-batch.md) — batch analysis with clustering to segment results by structural family

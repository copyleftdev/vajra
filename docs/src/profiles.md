# Profiles

Profiles are the lens. They do not change *what* Vajra analyzes — they change *how results are scored, ranked, and rendered*.

The same document analyzed with `--profile staff` and `--profile engineer` produces the same underlying statistics. The difference is which observations surface, what language describes them, and what gets collapsed as noise.

---

## The Scoring Model

Every observation in the analysis pipeline receives a composite importance score:

```
score = sum(weight_i * signal_i)
```

Six signal dimensions, each normalized to [0, 1]:

| Dimension | What It Measures |
|---|---|
| `rarity` | Self-information of the observation. Rare things score high. |
| `instability` | Type instability at the path. Mixed types score high. |
| `entropy_signal` | Distance from 0.5 normalized entropy. Constants and noise both score high. Meaningful variation scores low. |
| `structural_coverage` | Fraction of total nodes under this path. Wide-reaching paths score high. |
| `anomaly_strength` | Maximum anomaly score across all four dimensions. |
| `concern_relevance` | Profile-specific boost for certain paths or observation types. |

The profile defines the weights. The weights determine what rises to the top.

---

## Built-in Profiles

### staff

**For:** Non-technical operations staff who need "what is this and what stands out."

| Dimension | Weight |
|---|---|
| rarity | 0.10 |
| instability | 0.05 |
| entropy_signal | 0.10 |
| structural_coverage | 0.25 |
| anomaly_strength | 0.30 |
| concern_relevance | 0.20 |

**Rendering:** Plain language. No JSONPath. No technical jargon. Anomalies described in terms of business impact. Structural boilerplate hidden.

**Section headers:** "Document Summary," "What Stands Out," "What This Likely Means."

```bash
vajra essence claim.json --profile staff
```

```text
Document Summary:
  1 claim with 14 service lines, 1 patient, 2 diagnosis codes.

What Stands Out:
  - 3 service lines are missing allowed amounts.
  - Adjustment reason "CO-45" repeats across 8 of 14 lines.

What This Likely Means:
  - A subset of service lines appears incomplete.
  - The repeated adjustment code suggests a systematic issue.
```

---

### engineer

**For:** Engineers who need schema details, structural analysis, and regression signals.

| Dimension | Weight |
|---|---|
| rarity | 0.15 |
| instability | 0.25 |
| entropy_signal | 0.15 |
| structural_coverage | 0.15 |
| anomaly_strength | 0.15 |
| concern_relevance | 0.15 |

**Rendering:** Technical. JSONPath paths, type annotations, cardinalities. Diff-style output for drift. Fingerprints displayed.

```bash
vajra essence claim.json --profile engineer
```

```text
Structure: 847 nodes, 23 distinct paths, max depth 6
Fingerprint (path set): a1b2c3d4...

Notable paths:
  $.claims[*].service_lines[*].allowed_amount
    null_rate: 0.214, entropy: 3.12, type: number (100%)

  $.claims[*].service_lines[*].adjustment.reason
    entropy: 1.56, cardinality: 4, dominant: "CO-45" (57.1%)
```

---

### auditor

**For:** Auditors and compliance teams who need completeness, traceability, and consistency evidence.

| Dimension | Weight |
|---|---|
| rarity | 0.10 |
| instability | 0.20 |
| entropy_signal | 0.10 |
| structural_coverage | 0.10 |
| anomaly_strength | 0.20 |
| concern_relevance | 0.30 |

**Rendering:** Formal vocabulary. Missing fields listed with full paths. Type inconsistencies documented with examples. Drift metrics with severity scores.

**Concern relevance boosts:** completeness, traceability, required-field absence.

```bash
vajra essence claim.json --profile auditor --format markdown
```

```markdown
## Audit Essence

### Completeness Assessment
- **21.4%** of service lines are missing `allowed_amount`
  (3 of 14 service line records; field path: `$.claims[*].service_lines[*].allowed_amount`)
- Provider `taxonomy` field: absent
  (expected presence rate in comparable data: 94%)

### Type Consistency
- All paths exhibit 100% type stability. No type inconsistencies detected.

### Pattern Observations
- Adjustment reason code `CO-45` appears in 57.1% of service lines (8 of 14).
  This concentration exceeds typical variance for this field.
```

---

### ai

**For:** Downstream LLM consumption. Maximum information density per token.

| Dimension | Weight |
|---|---|
| rarity | 0.15 |
| instability | 0.10 |
| entropy_signal | 0.20 |
| structural_coverage | 0.20 |
| anomaly_strength | 0.20 |
| concern_relevance | 0.15 |

**Rendering:** Compact, machine-readable. Motifs collapsed aggressively. Repeated structures represented once with count. Explicit caveats on inferences.

```bash
vajra essence claim.json --profile ai --format compact-ai --budget 300
```

```json
{"v":"vajra/1","n":847,"p":23,"d":6,"motif":{"p":"$.claims[0].service_lines[*]","c":14,"f":["procedure_code","charge_amount","allowed_amount","status","adjustment"]},"a":[{"p":"$.claims[0].service_lines[2,7,11].allowed_amount","t":"miss","s":4.2}],"drill":[{"p":"$.claims[*].service_lines","avail":["stats","anomalies"]}]}
```

---

### fraud

**For:** Fraud and risk analysts who need suspicious patterns, outliers, and unusual combinations.

| Dimension | Weight |
|---|---|
| rarity | 0.25 |
| instability | 0.10 |
| entropy_signal | 0.10 |
| structural_coverage | 0.05 |
| anomaly_strength | 0.35 |
| concern_relevance | 0.15 |

**Rendering:** Investigative framing. Outliers with full context. Benford's Law departures. Suspicious value repetition. Unusual co-occurrence patterns.

**Concern relevance boosts:** numeric anomalies, identifier patterns, timing irregularities.

```bash
vajra essence claims_batch.ndjson --profile fraud
```

```text
=== Fraud Screening Essence ===

Flagged Patterns:
  - charge_amount outlier: $47,250.00 in record 834
    (z_MAD = 6.3, population median = $285.00)
    This value is 165x the median. Review recommended.

  - Status value "voided" in record 419
    (seen once in 1,247 records, self-information = 10.3 bits)
    Extremely rare status. May warrant investigation.

  - Benford's Law departure for charge_amount leading digits
    Chi-squared: 14.2 (p = 0.028)
    Observed leading digit "1": 18% (expected: 30%)
    Observed leading digit "5": 22% (expected: 8%)
    Suggestive of non-natural distribution.

  - Identical charge_amount ($285.00) in 47 records from same provider
    Exact-value concentration: 3.8% of population
    Pattern is unusual for this field's typical variance.
```

---

## Custom Profiles

Define custom profiles in TOML. Load with `--config path/to/profiles.toml`.

### Full TOML Example

```toml
[profile.claims_review]
name = "claims-review"
description = "Internal review for claims processing teams"

[profile.claims_review.weights]
rarity = 0.15
instability = 0.20
entropy_signal = 0.10
structural_coverage = 0.10
anomaly_strength = 0.25
concern_relevance = 0.20

[profile.claims_review.rendering]
vocabulary = "plain"           # plain | technical | formal
show_paths = false             # hide JSONPath in output
show_scores = false            # hide numeric scores
motif_collapse_threshold = 3   # collapse motifs repeated > N times
anomaly_threshold = 3.5        # MAD z-score threshold for flagging

[profile.claims_review.concern_boosts]
paths_containing = ["denied", "adjustment", "override", "void"]
observation_types = ["missingness", "type_instability"]
boost_factor = 1.5
```

### Loading Custom Profiles

```bash
vajra essence claim.json --profile claims-review --config ./profiles.toml
```

### Multiple Custom Profiles in One File

```toml
[profile.claims_review]
name = "claims-review"
description = "Internal claims processing review"
# ... weights, rendering, boosts ...

[profile.vendor_audit]
name = "vendor-audit"
description = "Vendor data feed quality assessment"
# ... weights, rendering, boosts ...

[profile.ml_preprocessing]
name = "ml-preprocessing"
description = "Data quality check before ML pipeline ingestion"
# ... weights, rendering, boosts ...
```

### Listing Available Profiles

```bash
vajra profiles
```

```text
=== Built-in Profiles ===
  staff        Plain vocabulary, narrative rendering; emphasizes anomalies and structural coverage
  engineer     Technical vocabulary, list-based rendering; balanced scoring
  auditor      Formal vocabulary, completeness-focused; emphasizes instability and concern relevance
  ai           Compact terse rendering optimized for machine consumption
  fraud        Investigative framing; emphasizes outliers, rarity, and suspicious patterns

=== Custom Profiles ===
  claims-review   Internal claims processing review
```

```bash
vajra profiles --config ./profiles.toml --format json
```

```json
[
  {"name": "staff", "description": "...", "source": "built-in"},
  {"name": "engineer", "description": "...", "source": "built-in"},
  {"name": "auditor", "description": "...", "source": "built-in"},
  {"name": "ai", "description": "...", "source": "built-in"},
  {"name": "fraud", "description": "...", "source": "built-in"},
  {"name": "claims-review", "description": "Internal claims processing review", "source": "custom"}
]
```

---

## Rendering Vocabulary

| Level | Description | Example |
|---|---|---|
| `plain` | No jargon, no paths, business-oriented language | "3 service lines are missing allowed amounts" |
| `technical` | JSONPath, type annotations, statistical measures | "$.claims[*].service_lines[2,7,11].allowed_amount: null_rate=0.21, anomaly_score=4.2" |
| `formal` | Full sentences, compliance-appropriate language | "Observations 2, 7, and 11 in the service line array exhibit absent allowed_amount fields." |

---

## Deterministic Tie-Breaking

When two observations have identical composite scores, ties are broken by:

1. **Path depth** — shallower paths first (broader impact)
2. **Lexicographic path order** — alphabetical by wildcard path

This ensures identical scores always resolve in the same order, regardless of platform or run.

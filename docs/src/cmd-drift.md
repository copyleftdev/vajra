# drift

`drift` detects and quantifies structural, type, and distributional changes between two JSON documents. It answers the question every engineer asks when something breaks: *what changed?*

Not what changed in the values — what changed in the *shape, types, and statistical behavior* of the data.

---

## Usage

```bash
vajra drift <baseline> <candidate> [flags]
```

**Arguments:**

| Argument | Description |
|---|---|
| `<baseline>` | The reference document (the "before") |
| `<candidate>` | The comparison document (the "after") |

**Flags:**

| Flag | Description | Default |
|---|---|---|
| `--format <fmt>` | Output format: `text`, `json`, `markdown`, `compact-ai` | `text` |
| `--profile <name>` | Concern profile for severity weighting | `engineer` |
| `--input-format <fmt>` | Override auto-detected input format | auto |
| `--redact` | Apply built-in redaction before output | off |
| `--quiet` | Suppress progress output | off |
| `--group-by <path>` | JSONPath for population-level comparison (e.g., `'$.author_type'`) | off |

---

## Population-Level Comparison

When `--group-by` is specified, `drift` partitions records by the field value and computes pairwise drift between all groups. Instead of comparing two documents, you compare two (or more) subpopulations within the same dataset.

```bash
vajra drift prs.ndjson --group-by '$.author_type'
```

```text
Drift Report (grouped by $.author_type)
Groups: bot (412 records), human (835 records)

Pairwise drift: bot vs human
  Structural similarity: 0.91 (Jaccard)

  Distribution shifts:
    $.files_changed              JSD: 0.42 (high)
      bot:   median 1.0, p95 3.0
      human: median 4.0, p95 18.0

    $.review_comments            JSD: 0.38 (moderate)
      bot:   median 0.0, p95 1.0
      human: median 2.0, p95 8.0

  Overall severity: HIGH (significant distributional divergence)
```

This is useful for comparing behavioral subgroups — bot vs. human PRs, different teams, production vs. staging, before vs. after a policy change — without needing separate files.

---

## Drift Dimensions

### Structural Drift

Path set symmetric difference:

```
added_paths   = paths(candidate) \ paths(baseline)
removed_paths = paths(baseline) \ paths(candidate)
```

New fields appearing. Old fields disappearing. The most visible form of schema evolution.

### Type Drift

For each path present in both documents, the dominant type is compared. Any path where the type changed (e.g., string to number, array to object) is flagged.

### Distributional Drift

**Jensen-Shannon Divergence (JSD)** measures how much value distributions shifted between baseline and candidate:

```
JSD(P || Q) = 0.5 * KL(P || M) + 0.5 * KL(Q || M)
```

where M = 0.5 * (P + Q).

JSD is symmetric, always finite, bounded to [0, 1], and its square root is a proper metric.

For numeric paths, Vajra instead computes the **1D Wasserstein distance** (earth mover's distance), which captures *how far* values moved, not just that they moved.

#### `value` is not comparable across metrics — rank by `effect_size`

Each drift carries both a `value` and an `effect_size`, and they answer different questions.

`value` is in the metric's own units. JSD is bounded to `[0,1]`, but Wasserstein is in the units of the underlying field, so the two cannot be ranked against each other. On a real corpus, sorting by `value` produces this:

```console
$ vajra drift features.ndjson --group-by '$.label' --format json --quiet \
    | jq '.pairwise_drift[0].distributional_drifts | sort_by(-.value)'
  value=1321026.74   WassersteinDistance   $[*].total_bytes
  value=20085.70     WassersteinDistance   $[*].file_bytes
  value=15075.44     WassersteinDistance   $[*].ast_nodes
```

`total_bytes` leads by six orders of magnitude for one reason only: it is measured in bytes. A boolean path that separates the two populations almost perfectly reports a `value` of 0.64 and sorts near the bottom.

`effect_size` is unit-free and bounded to `[0,1]`, so the same list ranks usefully:

```console
    | jq '.pairwise_drift[0].distributional_drifts | sort_by(-.effect_size)'
  effect=1.0000   value=1.0000        JSD           $[*].label
  effect=0.6435   value=0.6435        Wasserstein   $[*].has_repository
  effect=0.6352   value=17.4676       Wasserstein   $[*].pj_distinct_paths
```

How it is computed:

| Path type | `effect_size` |
|---|---|
| Numeric | \|Cliff's delta\| — a rank-based non-parametric effect size. 0 = the samples are stochastically indistinguishable; 1 = every value in one group exceeds every value in the other. |
| Categorical | The JSD itself, which is already bounded to `[0,1]`. |

Both are 0 for identical distributions and 1 for maximal separation, so they order together. They are **not the same statistic** — treat `effect_size` as a magnitude for ranking, not as an estimate of one specific quantity. When you need the real-world size of a shift ("payloads grew by 1.3 MB"), read `value`.

Note that Cliff's delta relates to the AUC of the two samples as `|delta| = 2 * |AUC - 0.5|`, so it is directly comparable to a separation score computed from ranks.

**Thresholds are still in raw units.** A path is only reported when its `value` exceeds a per-metric threshold (JSD > 0.05, Wasserstein > 0.1). Because the Wasserstein threshold is in field units, a byte-scale path clears it on almost any change while a small-scale path may not. `effect_size` fixes ranking, not filtering.

### Drift Classification

Each drifted path receives a classification:

| Class | Meaning |
|---|---|
| `additive` | New path appeared in candidate |
| `subtractive` | Path present in baseline, absent in candidate |
| `type-mutative` | Dominant type changed |
| `distributional` | Value distribution shifted (JSD > threshold) |
| `cardinality-shift` | Array lengths changed significantly |
| `null-rate-shift` | Null/missing ratio changed significantly |

### Severity Scoring

The overall drift severity is a weighted sum of drift dimensions, tuned by the active profile:

- **Auditor profiles** weight subtractive drift highest (missing data is critical for compliance)
- **Engineer profiles** weight type-mutative drift highest (breaking changes)
- **Fraud profiles** weight distributional drift highest (behavioral shifts)

---

## Example: Text Output

```bash
vajra drift yesterday.json today.json
```

```text
Drift Report: yesterday.json -> today.json
Structural similarity: 0.94 (Jaccard)

Added paths (2):
  $.response.metadata.processing_flags    [array of strings]
  $.response.metadata.api_version         [string]

Removed paths (0): none

Type changes (1):
  $.response.items[*].quantity            string -> number (clean type migration)

Distribution shifts (1):
  $.response.items[*].status              JSD: 0.34 (moderate)
    before: {"active": 0.82, "pending": 0.15, "error": 0.03}
    after:  {"active": 0.61, "pending": 0.12, "error": 0.27}
    note: "error" rate increased 9x

Null rate changes (0): none

Overall severity: MEDIUM (structural additions + significant distribution shift)
```

---

## Example: JSON Output

```bash
vajra drift baseline.json candidate.json --format json --quiet
```

```json
{
  "added_paths": [],
  "removed_paths": [],
  "type_changes": [],
  "structural_similarity": 1.0,
  "severity": "High",
  "distributional_drifts": [
    {
      "path": "$[*].latency_ms",
      "metric": "WassersteinDistance",
      "value": 188.25,
      "effect_size": 1.0
    },
    {
      "path": "$[*].status",
      "metric": "JensenShannonDivergence",
      "value": 0.75,
      "effect_size": 0.75
    }
  ]
}
```

Read both numbers together. `latency_ms` moved 188 ms (`value`) and the two samples do not overlap at all (`effect_size` 1.0). `status` has an `effect_size` equal to its `value`, because for categorical paths the effect size *is* the JSD.

With `--group-by`, the same structure appears once per population pair under `pairwise_drift`, alongside `group_sizes` and `groups`.

---

## Example: Medical Claim Drift

```bash
vajra drift baseline_claim.json updated_claim.json --profile auditor
```

```text
Drift Report: baseline_claim.json -> updated_claim.json
Structural similarity: 0.87 (Jaccard)

Added paths (3):
  $.claims[*].service_lines[*].modifier_codes     [array of strings]
  $.claims[*].rendering_provider                   [object]
  $.claims[*].rendering_provider.npi               [string]

Removed paths (1):
  $.claims[*].provider.taxonomy                    [string]
    ** SUBTRACTIVE: field present in baseline, absent in candidate **

Type changes (0): none

Distribution shifts (2):
  $.claims[*].service_lines[*].status              JSD: 0.22
    before: {"adjudicated": 0.85, "pending": 0.15}
    after:  {"adjudicated": 0.64, "pending": 0.21, "denied": 0.15}
    note: new value "denied" appeared

  $.claims[*].service_lines[*].charge_amount       Wasserstein: 125.40
    before: median 285.00, p95 890.00
    after:  median 410.00, p95 1350.00
    note: charges shifted upward

Overall severity: HIGH (subtractive drift in auditor profile)
```

The auditor profile flags the removed `taxonomy` path as high severity because subtractive drift — data that was present and is now absent — is the most dangerous form of schema evolution for compliance.

---

## When to Use It

- **API version migration.** Compare the response shape before and after a deploy.
- **Vendor data monitoring.** Compare this week's feed to last week's. Detect undocumented schema changes before they break your pipeline.
- **Regulatory compliance.** Prove that the data structure has not drifted outside acceptable bounds.
- **CI integration.** Gate deploys on drift severity. If drift exceeds a threshold, fail the build and require review.

---

## Pairs Well With

- [`fingerprint`](./cmd-fingerprint.md) — quick structural same-or-different check before detailed drift analysis
- [`inspect`](./cmd-inspect.md) — understand each document's structure before comparing
- [`anomalies`](./cmd-anomalies.md) — drift detects changes between versions; anomalies detect deviations within a version
- [`essence`](./cmd-essence.md) — drift observations feed into essence generation when a baseline is provided

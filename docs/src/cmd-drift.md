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

JSD is symmetric, always finite, bounded to [0, 1], and its square root is a proper metric. This means drift magnitudes can be meaningfully compared and accumulated across paths.

For numeric paths, Vajra also computes the **1D Wasserstein distance** (earth mover's distance), which captures *how far* values moved, not just that they moved.

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
vajra drift yesterday.json today.json --format json
```

```json
{
  "baseline": "yesterday.json",
  "candidate": "today.json",
  "jaccard_similarity": 0.94,
  "overall_severity": "medium",
  "added_paths": [
    {
      "path": "$.response.metadata.processing_flags",
      "type": "array"
    },
    {
      "path": "$.response.metadata.api_version",
      "type": "string"
    }
  ],
  "removed_paths": [],
  "type_changes": [
    {
      "path": "$.response.items[*].quantity",
      "baseline_type": "string",
      "candidate_type": "number",
      "jsd": 0.0
    }
  ],
  "distribution_shifts": [
    {
      "path": "$.response.items[*].status",
      "jsd": 0.34,
      "baseline_distribution": {
        "active": 0.82,
        "pending": 0.15,
        "error": 0.03
      },
      "candidate_distribution": {
        "active": 0.61,
        "pending": 0.12,
        "error": 0.27
      }
    }
  ],
  "null_rate_changes": []
}
```

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

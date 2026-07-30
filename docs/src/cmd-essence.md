# essence

`essence` is the command Vajra was built for. It takes a JSON document, runs the full analysis pipeline, scores every observation against a concern profile's weight vector, and renders a compressed, ranked, faithful representation — shaped for whoever is reading it.

An essence is not a summary. A summary loses information probabilistically. An essence compresses information deterministically, preserving everything above a configurable importance threshold while collapsing structural noise.

---

## Usage

```bash
vajra essence <input> [flags]
```

**Arguments:**

| Argument | Description |
|---|---|
| `<input>` | Path to a JSON file, `-` for stdin, directory, or HTTP URL |

**Flags:**

| Flag | Description | Default |
|---|---|---|
| `--format <fmt>` | Output format: `text`, `json`, `markdown`, `compact-ai` | `text` |
| `--profile <name>` | Concern profile: `staff`, `engineer`, `auditor`, `ai`, `fraud`, or custom | `engineer` |
| `--budget <N>` | Approximate token budget for output | unlimited |
| `--config <path>` | Path to TOML file with custom profile definitions | none |
| `--input-format <fmt>` | Override auto-detected input format | auto |
| `--streaming` | Force streaming mode | off |
| `--redact` | Apply built-in redaction before rendering | off |
| `--explain` | Include score decomposition for each observation | off |
| `--quiet` | Suppress progress output | off |

---

## How Essence Construction Works

1. **Collect candidates.** All observations from the analysis pipeline — notable fields, motifs, anomalies, relationship discoveries — become candidates.

2. **Score each candidate** using the active profile's six-dimensional weight vector:
   - `rarity` — self-information of the observation
   - `instability` — type instability at the path
   - `entropy_signal` — distance from 0.5 normalized entropy (both constants and noise score high)
   - `structural_coverage` — fraction of total nodes under this path
   - `anomaly_strength` — maximum anomaly score across dimensions
   - `concern_relevance` — profile-specific boost for this path or observation type

3. **Collapse motifs.** Repeated structural patterns are represented once with a count and specific variations noted.

4. **Rank by composite score** with deterministic tie-breaking (shallower paths first, then lexicographic).

5. **Apply token budget** (if `--budget` is set). Greedy selection by score-per-token — the fractional knapsack approximation.

6. **Render** using the profile's vocabulary and rendering style.

---

## Profiles at a Glance

| Profile | Vocabulary | Rendering | Emphasizes |
|---|---|---|---|
| `staff` | Plain language | Narrative sections | Anomalies, structural coverage |
| `engineer` | Technical, JSONPath | Tabular, list-based | Type instability, all dimensions balanced |
| `auditor` | Formal | Completeness-focused | Instability, concern relevance, missingness |
| `ai` | Compact, terse | Machine-readable | Entropy signal, structural coverage, anomalies |
| `fraud` | Investigative | Outlier-focused | Rarity, anomaly strength |

See [Profiles](./profiles.md) for full weight vectors and customization.

---

## Example: Staff Profile

```bash
vajra essence claim.json --profile staff
```

```text
=== Essence (staff profile) ===

Document Summary:
  1 claim with 14 service lines, 1 patient, 2 diagnosis codes.
  Primary status: partially adjudicated.

What Stands Out:
  - 3 service lines are missing allowed amounts (lines 2, 7, 11).
    This field is present in 79% of service lines — its absence is notable.
  - Adjustment reason code "CO-45" repeats across 8 of 14 lines.
    Repetition at this frequency suggests a systematic pattern, not random variation.
  - 1 diagnosis structure differs from the other.
    The second diagnosis carries an extra "qualifier" field.
  - Provider taxonomy code is absent.
    This field is expected in 94% of claims in typical batches.

What This Likely Means:
  - Most of the claim is consistent and well-formed.
  - A subset of service lines appears incomplete or differently processed.
  - The repeated adjustment code points to a systematic issue.
```

No JSONPath. No z-scores. No jargon. The staff member gets what they need to act.

---

## Example: Engineer Profile

```bash
vajra essence claim.json --profile engineer
```

```text
=== Essence (engineer profile) ===

Structure: 847 nodes, 23 distinct paths, max depth 6
Fingerprint (path set): a1b2c3d4...
Dominant motif: $.claims[*].service_lines[*] (14 instances, 8 fields each)

Notable paths:
  $.claims[*].service_lines[*].allowed_amount
    null_rate: 0.214, entropy: 3.12, type: number (100%)
    absent in 3 of 14 service lines (indices 2, 7, 11)

  $.claims[*].service_lines[*].adjustment.reason
    entropy: 1.56, cardinality: 4
    dominant value: "CO-45" (57.1%, 8 of 14)

  $.claims[*].diagnosis[1]
    structural deviation: extra field "qualifier" (not in diagnosis[0])

Type stability: 100% across all paths
Array homogeneity: service_lines 100% (1 shape hash), diagnosis 50% (2 shape hashes)
```

---

## Example: AI Profile with Token Budget

```bash
vajra essence claim.json --profile ai --format json --budget 500
```

```json
{
  "identity": "document(nodes=25, paths=9, depth=3)",
  "structure": [],
  "anomalies": [
    {
      "path": "$[*].score",
      "description": "numeric outlier (value=22.1, z_mad=13.04)",
      "score": 0.22499999999999998,
      "anomaly_strength": 1.0,
      "concern_relevance": 0.5,
      "entropy_signal": 0.0,
      "instability": 0.0,
      "rarity": 0.0,
      "structural_coverage": 0.0
    }
  ],
  "observations": []
}
```

Each entry carries its scoring components alongside the final `score`, so a ranking is decomposable — `--explain` prints the same weights in text form. The active profile's weight vector determines how the components combine.

The AI profile collapses aggressively. Motifs are represented once with counts. Observations are sorted by score-per-token. The `meta.truncated` field tells the downstream model whether anything was cut.

---

## Example: Compact-AI Format

```bash
vajra essence claim.json --profile ai --format compact-ai --budget 300
```

```json
{"v":"vajra/1","n":847,"p":23,"d":6,"motif":{"p":"$.claims[0].service_lines[*]","c":14},"a":[{"p":"$.claims[0].service_lines[2,7,11].allowed_amount","t":"miss","s":4.2},{"p":"$.claims[0].diagnosis[1]","t":"struct","s":3.1}],"drill":[{"p":"$.claims[*].service_lines","avail":["stats","anomalies","motifs"]}]}
```

Maximum compression. Every key shortened. The `drill` section tells the LLM which paths have deeper analysis available for follow-up queries.

---

## Example: With --explain

```bash
vajra essence claim.json --profile engineer --explain
```

```text
Notable paths:
  $.claims[*].service_lines[*].allowed_amount
    null_rate: 0.214, entropy: 3.12
    [score: 0.68]
      rarity:             0.42  x  weight 0.15  =  0.063
      instability:        0.00  x  weight 0.25  =  0.000
      entropy_signal:     0.24  x  weight 0.15  =  0.036
      structural_coverage: 0.18 x  weight 0.15  =  0.027
      anomaly_strength:   0.89  x  weight 0.15  =  0.134
      concern_relevance:  0.75  x  weight 0.15  =  0.113
```

Every score decomposed into its six dimensions. Nothing hidden. Nothing magic.

---

## The Token Budget

When `--budget N` is specified, Vajra estimates the token cost of each observation (word count x 1.3) and selects greedily by score-per-token until the budget is exhausted. This is the fractional knapsack approximation — optimal for the greedy case.

The budget is approximate, not exact. It prevents bloated output without requiring precise token counting.

---

## When to Use It

- **Non-technical stakeholders.** `--profile staff` translates the data into plain language.
- **AI pipelines.** `--profile ai --format compact-ai --budget 500` compresses a 1000-node document into a token-efficient context.
- **Audits.** `--profile auditor` emphasizes completeness, missingness, and traceability.
- **Fraud screening.** `--profile fraud` amplifies anomalies and rare patterns.
- **Documentation.** `--format markdown` renders the essence as publishable documentation.

---

## Pairs Well With

- [`stats`](./cmd-stats.md) — the statistical baseline that feeds scoring
- [`anomalies`](./cmd-anomalies.md) — anomalies are the highest-priority candidates in most profiles
- [`drift`](./cmd-drift.md) — drift observations appear in the essence when a baseline is available
- [Profiles](./profiles.md) — full control over what gets emphasized and how it gets rendered

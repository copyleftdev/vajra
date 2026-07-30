# stats

`stats` computes the statistical profile of a JSON document. Entropy, frequency distributions, numeric summaries, null rates, cardinality — the quantitative foundation that every other analysis depends on.

Where `inspect` tells you *what exists*, `stats` tells you *how it behaves*.

---

## Usage

```bash
vajra stats <input> [flags]
```

**Arguments:**

| Argument | Description |
|---|---|
| `<input>` | Path to a JSON file, `-` for stdin, or an HTTP URL |

**Flags:**

| Flag | Description | Default |
|---|---|---|
| `--format <fmt>` | Output format: `text`, `json`, `markdown`, `compact-ai` | `text` |
| `--input-format <fmt>` | Override auto-detected input format | auto |
| `--streaming` | Force streaming mode (sketch-based approximations) | off |
| `--redact` | Apply built-in redaction before output | off |
| `--quiet` | Suppress progress output | off |
| `--window <period>` | Temporal windowing: `month`, `week`, or `day` | off |
| `--time-field <path>` | JSONPath to timestamp field (e.g., `'$.date'`). Auto-detected if omitted. | auto |

---

## Temporal Windowing

When `--window` is specified, `stats` partitions records by time period and computes per-window statistics. Cross-window trend lines are included in the output, showing how distributions shift over time.

The `--time-field` flag tells Vajra which field contains the timestamp. If omitted, Vajra auto-detects by scanning for fields with date/time patterns (ISO 8601, Unix timestamps, common date formats).

```bash
vajra stats commits.ndjson --window month --time-field '$.date'
```

```text
=== Statistical Summary (windowed: month) ===
Document: commits.ndjson (1,247 records, 8 paths)

--- Window: 2026-01 (312 records) ---
  $.files_changed
    Mean: 4.2   Median: 3.0   p95: 12.0

--- Window: 2026-02 (298 records) ---
  $.files_changed
    Mean: 5.1   Median: 4.0   p95: 15.0

--- Window: 2026-03 (337 records) ---
  $.files_changed
    Mean: 6.8   Median: 5.0   p95: 19.0

--- Cross-Window Trends ---
  $.files_changed  mean: 4.2 -> 5.1 -> 6.8 (upward, +62% over 3 months)
  $.type           "fix" share: 0.18 -> 0.24 -> 0.31 (increasing)
```

Windowing works with any multi-record input: NDJSON, CSV, multi-document YAML, or directories.

---

## What It Reports

For every wildcard path in the document:

### Frequency and Cardinality
- **Count** — total observations at this path
- **Cardinality** — number of distinct values
- **Top values** — the most frequent values with their counts

### Entropy
- **Shannon entropy** — H(X) in bits. Measures information content.
- **Normalized entropy** — H(X) / log2(|support|). Scales to [0, 1] regardless of cardinality.

The entropy pair is one of the most powerful signals in the system:

| Entropy | Normalized | Interpretation |
|---|---|---|
| 0 | 0 | Constant — single value, pure boilerplate |
| Low | Low | Enum-like — few distinct states |
| Low | High | Near-uniform over tiny support |
| High | Moderate | Meaningful variation — identifiers, dates, codes |
| High | High | Near-uniform over large support — free text, UUIDs |

### Missingness
- **Null rate** — fraction of observations that are JSON `null`
- **Absent rate** — fraction of parent records where this path does not appear
- **Empty rate** — fraction of values that are empty strings, empty arrays, or empty objects

### Numeric Distributions (for numeric paths)
- **Min, max, mean, median**
- **Percentiles** — p01, p05, p25, p50, p75, p95, p99
- **MAD** — Median Absolute Deviation (robust dispersion)
- **Skewness proxy** — (mean - median) / MAD

### Type Distribution
- Breakdown of JSON types observed at each path (e.g., 98% number, 2% string)
- **Type instability score** — fraction of observations deviating from the dominant type

---

## Example: Text Output

```bash
vajra stats claim.json
```

```text
=== Statistical Summary ===
Document: claim.json (847 nodes, 23 paths)

--- $.claims[*].service_lines[*].charge_amount ---
  Count:       14
  Cardinality: 12
  Entropy:     3.41 bits (normalized: 0.88)
  Type:        number (100%)
  Min:         45.00    Max: 1250.00
  Mean:        312.50   Median: 285.00
  MAD:         195.00
  p25:         125.00   p75: 425.00
  p95:         890.00   p99: 1125.00

--- $.claims[*].service_lines[*].status ---
  Count:       14
  Cardinality: 3
  Entropy:     1.22 bits (normalized: 0.77)
  Type:        string (100%)
  Top values:
    "adjudicated"  10 (71.4%)
    "pending"        3 (21.4%)
    "denied"         1 (7.1%)

--- $.claims[*].service_lines[*].allowed_amount ---
  Count:       11
  Cardinality: 9
  Entropy:     3.12 bits (normalized: 0.93)
  Type:        number (100%)
  Null rate:   0.000
  Absent rate: 0.214  ** notable: missing in 3 of 14 service lines **
  Min:         32.00    Max: 875.00
  Mean:        245.30   Median: 210.00
  MAD:         142.00

--- $.claims[*].diagnosis[*].code ---
  Count:       2
  Cardinality: 2
  Entropy:     1.00 bits (normalized: 1.00)
  Type:        string (100%)
  Top values:
    "E11.9"  1 (50.0%)
    "J44.1"  1 (50.0%)

--- $.claims[*].service_lines[*].adjustment.reason ---
  Count:       14
  Cardinality: 4
  Entropy:     1.56 bits (normalized: 0.78)
  Type:        string (100%)
  Top values:
    "CO-45"   8 (57.1%)
    "CO-97"   3 (21.4%)
    "PR-1"    2 (14.3%)
    "OA-23"   1 (7.1%)
```

---

## Example: JSON Output

```bash
vajra stats claim.json --format json
```

```json
{
  "paths": [
    {
      "path": "$[*].id",
      "entropy": 1.584962500721156,
      "normalized_entropy": 1.0,
      "cardinality": 3,
      "total_count": 3,
      "max_rarity": 1.5849625007211563,
      "numeric": {
        "min": 1.0, "max": 3.0, "mean": 2.0, "median": 2.0, "mad": 1.0,
        "p05": 1.1, "p25": 1.5, "p75": 2.5, "p95": 2.9
      },
      "top_values": [
        { "value": "1", "count": 1 }
      ]
    },
    {
      "path": "$[*].name",
      "entropy": 1.584962500721156,
      "normalized_entropy": 1.0,
      "cardinality": 3,
      "total_count": 3,
      "max_rarity": 1.5849625007211563,
      "numeric": null,
      "top_values": [
        { "value": "a", "count": 1 }
      ]
    }
  ]
}
```

Output is a single `paths` array, one entry per distinct wildcard path. `numeric` is `null` for non-numeric paths. With `--window`, the response instead carries `windows` and `trends`.

---

## When to Use It

- **Understanding data distributions.** What does the `charge_amount` field actually look like? What are the common status values? How much entropy does this field carry?
- **Finding hidden nulls and absences.** A field with 21% absent rate across service lines is operationally significant — `stats` surfaces this.
- **Establishing baselines.** Run `stats` on today's batch. Run it again tomorrow. Compare the distributions manually or feed them to `drift`.
- **Identifying enum-like fields.** Low cardinality + low entropy = enum. High cardinality + high entropy = identifier. `stats` makes this distinction quantitative.

---

## Pairs Well With

- [`inspect`](./cmd-inspect.md) — structural overview before statistical deep dive
- [`anomalies`](./cmd-anomalies.md) — `stats` computes the distributions; `anomalies` flags what deviates from them
- [`essence`](./cmd-essence.md) — the essence builder uses stats internally to score observation importance
- [`invariants`](./cmd-invariants.md) — cross-field analysis builds on per-field statistics

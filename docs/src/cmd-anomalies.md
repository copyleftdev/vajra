# anomalies

`anomalies` surfaces records, fields, and structural elements that deviate meaningfully from the population. It does this across four dimensions — numeric outliers, rarity, structural deviation, and type instability — using only deterministic, interpretable methods.

No training data. No labeled examples. No rules to configure. Feed it cold data and it finds what deviates from what the data says is normal.

---

## Usage

```bash
vajra anomalies <input> [flags]
```

**Arguments:**

| Argument | Description |
|---|---|
| `<input>` | Path to a JSON file, NDJSON batch, `-` for stdin, or directory |

**Flags:**

| Flag | Description | Default |
|---|---|---|
| `--format <fmt>` | Output format: `text`, `json`, `markdown`, `compact-ai` | `text` |
| `--input-format <fmt>` | Override auto-detected input format | auto |
| `--streaming` | Force streaming mode | off |
| `--redact` | Apply built-in redaction before output | off |
| `--explain` | Include score decomposition for each anomaly | off |
| `--quiet` | Suppress progress output | off |

---

## The Four Dimensions

### Dimension 1: Numeric Outliers

**Method:** MAD-based modified z-scores.

For every numeric path, Vajra computes the median and the Median Absolute Deviation (MAD). Values where the modified z-score exceeds the threshold (default 3.5) are flagged.

```
z_MAD = 0.6745 * (value - median) / MAD
```

MAD has a 50% breakdown point — half the data can be arbitrarily corrupted before it gives a misleading result. Standard deviation has a 0% breakdown point. This distinction matters when the data you are analyzing might contain the very outliers you are trying to detect.

### Dimension 2: Rarity Outliers

**Method:** self-information scoring.

For each (path, value) pair:

```
rarity = -log2(frequency / total)
```

A value seen once in 10,000 records scores ~13.3 bits. A value seen in half the records scores 1 bit. The threshold adapts per path: values exceeding `mean_rarity + 2 * MAD_of_rarity` are flagged.

### Dimension 3: Structural Deviations

**Method:** Jaccard distance from the dominant path set.

In batch analysis, Vajra computes the most common set of paths (the structural mode). Each document is compared:

```
structural_anomaly = 1 - Jaccard(doc_paths, mode_paths)
```

Documents with structural anomaly > 0.2 are flagged, with the specific missing and extra paths listed.

### Dimension 4: Type Instability

**Method:** per-path type instability score.

```
instability = 1 - (dominant_type_count / total_observations)
```

Paths with instability > 0.01 are flagged. Individual records contributing the minority type are identified.

---

## Example: Text Output

```bash
vajra anomalies claims_batch.ndjson
```

```text
=== Anomaly Report ===
Records analyzed: 1,247
Anomalies found:  8

--- Numeric Outliers ---
  $.claims[*].service_lines[*].charge_amount
    Record 834: 47,250.00 (z_MAD = 6.3, median = 285.00, MAD = 195.00)
    Record 1102: 0.01 (z_MAD = -4.8, median = 285.00, MAD = 195.00)

  $.claims[*].service_lines[*].allowed_amount
    Record 834: 45,000.00 (z_MAD = 5.9, median = 210.00, MAD = 142.00)

--- Rarity Outliers ---
  $.claims[*].status
    Record 419: "voided" (10.3 bits, 1 of 1,247 records)

  $.claims[*].service_lines[*].adjustment.reason
    Record 77: "N-832" (9.1 bits, 2 of 17,458 service lines)

--- Structural Deviations ---
  Record 662: Jaccard distance 0.31 from structural mode
    Missing paths:
      $.claims[*].subscriber.group_number
      $.claims[*].subscriber.member_id
      $.claims[*].provider.npi
      $.claims[*].provider.taxonomy

--- Type Instability ---
  $.claims[*].service_lines[*].quantity
    Records 88, 204, 917: string where number expected
    Instability: 0.002 (3 of 1,247 records)
```

---

## Example: JSON Output

```bash
vajra anomalies claims_batch.ndjson --format json
```

```json
{
  "type_instabilities": [],
  "numeric_outliers": [
    {
      "path": "$[*].score",
      "value": 22.1,
      "z_mad": 13.040333333333344,
      "median": 10.5,
      "mad": 0.5999999999999996
    }
  ],
  "rare_values": []
}
```

Three arrays, always present even when empty. `z_mad` is the modified z-score — deviation from the median in MAD units — so it is robust to the outliers it is detecting.

---

## Example: With --explain

```bash
vajra anomalies claim.json --explain
```

```text
--- Numeric Outliers ---
  $.claims[*].service_lines[*].charge_amount
    Record 834: 47,250.00
      z_MAD:       6.3
      median:      285.00
      MAD:         195.00
      threshold:   3.5
      score decomposition:
        rarity:             0.82
        instability:        0.00
        entropy_signal:     0.34
        structural_coverage: 0.15
        anomaly_strength:   0.95
        concern_relevance:  0.40
        composite:          0.71
```

---

## When to Use It

- **Cold data triage.** You received a batch of claims and need to know what is unusual before reading any of them.
- **Fraud screening.** The `--profile fraud` variant amplifies rarity and numeric outlier weights. Unusual charge amounts, rare status values, and missing provider fields all surface.
- **Data quality monitoring.** Run `anomalies` on each day's batch in CI. If the anomaly count spikes, something changed upstream.
- **Pre-audit preparation.** Give auditors the anomaly report alongside the raw data. They know where to look.

---

## Pairs Well With

- [`stats`](./cmd-stats.md) — anomalies are scored against the statistical baseline that `stats` computes
- [`essence`](./cmd-essence.md) — anomalies feed into the essence as high-priority observations
- [`drift`](./cmd-drift.md) — anomalies detect deviations within a batch; `drift` detects changes between batches
- [`cluster`](./cmd-cluster.md) — structural deviations often indicate documents that belong to different clusters

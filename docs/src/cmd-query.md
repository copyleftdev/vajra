# query

`query` runs path-based expressions with analysis functions against a document. It lets you ask specific questions — what is the entropy at this path? Which values at this path are anomalous? What is the null rate for this field?

Where other commands analyze everything and present results, `query` lets you target a specific path and a specific measurement.

---

## Usage

```bash
vajra query <input> '<expression>' [flags]
```

**Arguments:**

| Argument | Description |
|---|---|
| `<input>` | Path to a JSON file, `-` for stdin, or an HTTP URL |
| `<expression>` | Query expression (path filter or analysis function) |

**Flags:**

| Flag | Description | Default |
|---|---|---|
| `--format <fmt>` | Output format: `text`, `json`, `markdown`, `compact-ai` | `text` |
| `--input-format <fmt>` | Override auto-detected input format | auto |
| `--redact` | Apply built-in redaction before output | off |
| `--quiet` | Suppress progress output | off |

---

## Expression Language

Vajra defines its own expression language inspired by JSONPath with analysis extensions. This is not JSONAta — it is a purpose-built query system for structural analysis.

### Path Filtering

Select values at a specific path:

```bash
vajra query claim.json '$.claims[*].service_lines[*].charge_amount'
```

```text
Path: $.claims[*].service_lines[*].charge_amount
Values (14):
  125.00, 285.00, 45.00, 890.00, 310.00, 425.00, 285.00,
  1250.00, 175.00, 520.00, 95.00, 680.00, 340.00, 410.00
```

### Analysis Functions

Apply analysis functions to a path:

```bash
vajra query claim.json 'entropy($.claims[*].service_lines[*].status)'
```

```text
entropy($.claims[*].service_lines[*].status)
  Shannon entropy: 1.22 bits
  Normalized entropy: 0.77
  Cardinality: 3
  Interpretation: enum-like, few distinct states
```

### Available Functions

| Function | Returns | Description |
|---|---|---|
| `entropy(path)` | Shannon entropy and normalized entropy | Information content at this path |
| `rarity(path, value)` | Self-information in bits | How rare a specific value is at this path |
| `instability(path)` | Type instability ratio | Fraction of values deviating from dominant type |
| `null_rate(path)` | Null and absent rates | Missingness profile at this path |
| `stats(path)` | Full statistical summary | Entropy, frequency, numeric distribution |
| `anomaly_score(path)` | Composite anomaly score | Maximum anomaly strength across dimensions |
| `motif(path)` | Dominant motif description | Repeated structural pattern at an array path |

### Conditional Expressions

Filter by analysis thresholds:

```bash
vajra query claim.json 'entropy($.claims[*].service_lines[*].status) > 0.5'
```

```text
entropy($.claims[*].service_lines[*].status) = 1.22
  Condition: > 0.5
  Result: TRUE
```

```bash
vajra query claim.json 'anomaly_score($.claims[*].service_lines[*].charge_amount) > 3.5'
```

```text
anomaly_score($.claims[*].service_lines[*].charge_amount)
  Max z_MAD across values: 6.3 (at value 47,250.00)
  Condition: > 3.5
  Result: TRUE
  Flagged values:
    47,250.00 (z_MAD = 6.3)
```

---

## Example: Text Output

```bash
vajra query claim.json 'stats($.claims[*].service_lines[*].charge_amount)'
```

```text
stats($.claims[*].service_lines[*].charge_amount)
  Count:       14
  Cardinality: 12
  Entropy:     3.41 bits (normalized: 0.88)
  Type:        number (100%)
  Min:         45.00
  Max:         1250.00
  Mean:        312.50
  Median:      285.00
  MAD:         195.00
  p25:         125.00
  p75:         425.00
  p95:         890.00
  p99:         1125.00
```

---

## Example: JSON Output

```bash
vajra query claim.json 'entropy($.claims[*].status)' --format json
```

```json
1.584962500721156
```

`query` emits the bare result, not an envelope. An analysis function returns a number; a path expression returns the selected value or array. That makes it directly composable:

```bash
if (( $(echo "$(vajra query data.json 'entropy($[*].author)') < 3" | bc -l) )); then
  echo "contributor diversity below threshold"
fi
```

---

## Example: Rarity Check

```bash
vajra query claims_batch.ndjson 'rarity($.claims[*].status, "voided")'
```

```text
rarity($.claims[*].status, "voided")
  Self-information: 10.3 bits
  Frequency: 1 of 1,247
  Interpretation: extremely rare (> 10 bits)
```

---

## Example: Null Rate Investigation

```bash
vajra query claim.json 'null_rate($.claims[*].service_lines[*].allowed_amount)'
```

```text
null_rate($.claims[*].service_lines[*].allowed_amount)
  Null rate:   0.000 (0 of 14 are JSON null)
  Absent rate: 0.214 (3 of 14 parent records lack this field)
  Empty rate:  0.000
  Total missingness: 0.214
```

---

## When to Use It

- **Targeted investigation.** You saw an anomaly in the essence. Now drill into the specific path.
- **Threshold checks in CI.** `vajra query data.json 'instability($.status) > 0.01'` — fail the build if type instability exceeds tolerance.
- **Statistical spot-checks.** What is the entropy of this field? What is the null rate? How rare is this value?
- **Script integration.** The `--format json` output is machine-readable. Parse it in your pipeline.

---

## Pairs Well With

- [`stats`](./cmd-stats.md) — `query` targets a single path; `stats` gives you everything
- [`anomalies`](./cmd-anomalies.md) — `query` lets you drill into a specific anomaly with `anomaly_score(path)`
- [`essence`](./cmd-essence.md) — the AI profile's `drill` section tells downstream models which paths to `query`
- [`inspect`](./cmd-inspect.md) — `inspect` reveals the paths; `query` interrogates them

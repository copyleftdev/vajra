# invariants

`invariants` discovers cross-field relationships from observed data. It finds fields that predict other fields, fields that always co-occur, and fields that are functionally dependent — all without prior knowledge of the schema.

This is data archaeology. Vajra examines the statistical co-occurrence of fields and extracts the latent rules that the data obeys.

---

## Usage

```bash
vajra invariants <input> [flags]
```

**Arguments:**

| Argument | Description |
|---|---|
| `<input>` | Path to a JSON file, NDJSON batch, `-` for stdin, or directory |

**Flags:**

| Flag | Description | Default |
|---|---|---|
| `--top-k <N>` | Maximum number of field pairs to consider | 50 |
| `--format <fmt>` | Output format: `text`, `json`, `markdown`, `compact-ai` | `text` |
| `--input-format <fmt>` | Override auto-detected input format | auto |
| `--redact` | Apply built-in redaction before output | off |
| `--quiet` | Suppress progress output | off |

---

## The Mathematics

### Conditional Entropy

For field pairs (X, Y):

```
H(Y|X) = -sum p(x,y) * log2(p(y|x))
```

Low H(Y|X) means X strongly predicts Y. If H(Y|X) approaches 0, Y is functionally determined by X — knowing X tells you Y with near-certainty.

### Pointwise Mutual Information (PMI)

```
PMI(x, y) = log2(P(x, y) / (P(x) * P(y)))
```

Positive PMI means x and y co-occur more than chance predicts. Negative PMI means they avoid each other. Zero means independence.

PMI is the information-theoretic standard for measuring association strength.

### Discovery Procedure

1. **Screen:** consider only paths with observation count > 30 (configurable). This filters noise.
2. **Compute:** for all pairs among the top-k most frequent paths, calculate conditional entropy and PMI.
3. **Rank:** ascending H(Y|X) for dependency strength, descending |PMI| for association strength.
4. **Report:** the strongest relationships with examples from the data.

With k = 50, this is 2,500 pairs — trivial even on large datasets. Unlike general association rule mining (which explores an exponential itemset space), this approach is bounded by design.

---

## Example: Text Output

```bash
vajra invariants claims_batch.ndjson
```

```text
=== Cross-Field Invariants ===
Records analyzed: 1,247
Field pairs screened: 1,225 (top 50 paths)

--- Functional Dependencies (H(Y|X) < 0.1) ---
  $.claims[*].subscriber.id -> $.claims[*].subscriber.name
    H(name|id) = 0.00
    subscriber.id fully determines subscriber.name
    Example: id "SUB-4421" -> name "Martinez, Elena" (47 records)

  $.claims[*].provider.npi -> $.claims[*].provider.name
    H(name|npi) = 0.03
    provider.npi nearly determines provider.name (3 exceptions in 1,247)
    Example: npi "1234567890" -> name "Valley Medical Group" (312 records)

--- Strong Co-occurrence (PMI > 2.0) ---
  $.claims[*].status = "denied" <-> $.claims[*].denial_reason present
    PMI = 3.8
    When status is "denied", denial_reason is present 97% of the time.
    When status is not "denied", denial_reason is present 2% of the time.

  $.claims[*].service_lines[*].procedure_code <-> $.claims[*].service_lines[*].service_date
    PMI = 3.2
    These fields co-occur in 99.8% of service lines. Effectively always together.

--- Conditional Presence ---
  $.claims[*].service_lines[*].modifier_codes
    Present in 100% of records where procedure_code starts with "9921"
    Present in 12% of records where procedure_code starts with "9939"
    Modifier presence is conditionally dependent on procedure type.

--- Anti-Correlation (PMI < -1.0) ---
  $.claims[*].status = "adjudicated" <-> $.claims[*].hold_reason present
    PMI = -2.1
    These rarely co-occur. Adjudicated claims almost never have hold reasons.
```

---

## Example: JSON Output

```bash
vajra invariants claims_batch.ndjson --format json
```

```json
{
  "records_analyzed": 1247,
  "pairs_screened": 1225,
  "functional_dependencies": [
    {
      "source": "$.claims[*].subscriber.id",
      "target": "$.claims[*].subscriber.name",
      "conditional_entropy": 0.0,
      "strength": "exact",
      "example": {
        "source_value": "SUB-4421",
        "target_value": "Martinez, Elena",
        "count": 47
      }
    },
    {
      "source": "$.claims[*].provider.npi",
      "target": "$.claims[*].provider.name",
      "conditional_entropy": 0.03,
      "strength": "near_exact",
      "exceptions": 3,
      "example": {
        "source_value": "1234567890",
        "target_value": "Valley Medical Group",
        "count": 312
      }
    }
  ],
  "co_occurrences": [
    {
      "field_a": "$.claims[*].status",
      "value_a": "denied",
      "field_b": "$.claims[*].denial_reason",
      "pmi": 3.8,
      "conditional_presence": 0.97
    }
  ],
  "anti_correlations": [
    {
      "field_a": "$.claims[*].status",
      "value_a": "adjudicated",
      "field_b": "$.claims[*].hold_reason",
      "pmi": -2.1
    }
  ]
}
```

---

## What Invariants Reveal

**Functional dependencies** are the strongest signal. When `subscriber.id` fully determines `subscriber.name`, that is not an accident — it reflects a real-world constraint. If that constraint breaks (a subscriber ID mapping to two different names), you have a data quality issue.

**Co-occurrence patterns** reveal implicit business rules. "When status is denied, denial_reason is present" is a rule that lives in the data, not in a schema. Vajra discovers it empirically.

**Anti-correlations** reveal mutual exclusions. Fields that never co-occur often represent different branches of a state machine — knowing which branch you are on determines which fields exist.

**Conditional presence** reveals fields whose existence depends on the value of another field. This is where JSON schemas fall short — they cannot express "this field exists only when that field equals X."

---

## When to Use It

- **Schema documentation.** Discover the implicit rules that the data already obeys. Document them before they are lost.
- **Data quality rules.** Turn discovered invariants into validation rules. If `subscriber.id` always determines `subscriber.name`, alert when it does not.
- **Onboarding.** New to a dataset? `invariants` shows you the relationships between fields faster than reading documentation (which may not exist).
- **Audit evidence.** Demonstrate that field dependencies are consistent across a batch.

---

## Pairs Well With

- [`stats`](./cmd-stats.md) — invariants build on per-field statistics (entropy, frequency, null rates)
- [`anomalies`](./cmd-anomalies.md) — broken invariants (a dependency that holds 99% of the time but not in record 662) are anomalies
- [`essence`](./cmd-essence.md) — discovered relationships appear in the essence as notable observations
- [`drift`](./cmd-drift.md) — if an invariant holds in the baseline but breaks in the candidate, that is a significant drift signal

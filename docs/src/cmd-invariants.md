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
| `--bin <spec>` | Discretise numeric fields: `quantile:N`, `equal-width:N`, `none` | `quantile:5` |
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

### Numeric Fields Are Discretised First

Conditional entropy over a continuous field is degenerate. If `score` holds 400 distinct values across 400 records, every distinct score maps to exactly one target value, so H(target | score) = 0 and the relationship reports as *perfect* whether or not one exists.

`invariants` therefore buckets numeric fields before building joint distributions. Controlled by `--bin`:

| `--bin` | Behaviour |
|---|---|
| `quantile:N` | Equal-frequency buckets, labelled `q0`..`q{N-1}`. **Default is `quantile:5`.** |
| `equal-width:N` | Equal-width buckets over the observed range, labelled `w0`..`w{N-1}` |
| `none` | Raw values — reproduces the degenerate behaviour above |

On 400 records where `score` is unique per record and noisily drives `outcome`:

```console
$ vajra invariants scores.json --bin none      # strength 1.0000 — meaningless
$ vajra invariants scores.json                 # strength 0.6055 — a real measurement
$ vajra invariants scores.json --bin equal-width:4
                                               # strength 0.4505
```

Two guards keep the transformation from doing harm:

- **A field is only binned if its distinct count exceeds the bucket count.** Booleans, small integer enums and other low-cardinality numerics pass through untouched — bucketing them could only merge categories that were already separable.
- **Non-numeric and non-finite columns are never binned.** A column containing `NaN` or `inf` is treated as non-numeric rather than silently mis-bucketed.

Every result records `field_x_binned` / `field_y_binned` so you can tell which values were transformed; text output marks them `[b]`.

**Known limitation.** Binning fixes *numeric* degeneracy only. A high-cardinality **string** field — a primary key, a UUID, a filename — is still near-unique and will still report a perfect relationship with everything. Treat any near-1.0 strength involving an identifier-like field as an artefact, not a finding.

### Direction Matters: `relationship_strength` vs `mutual_information`

`relationship_strength` normalises by the target's own entropy:

```
strength(X -> Y) = 1 - H(Y|X) / H(Y)
```

This is **not symmetric**. Consider a field `fine` with six distinct values and `coarse = fine mod 2`. `coarse` is fully determined by `fine`, so:

| direction | H(Y&#124;X) | strength |
|---|---|---|
| `fine -> coarse` | 0.0000 | **1.0000** |
| `coarse -> fine` | 1.5849 | **0.3868** |

Same pair, same data, strengths differing by 2.6×. The number answers "how well does the predictor determine *this particular* target", and the target's entropy is the yardstick.

Two consequences:

1. **Both directions of every pair are reported.** Filtering on `field_x` or `field_y` selects a *direction*, not a subset of pairs — you get every pair either way.
2. **Do not rank across pairs by `relationship_strength`** when the fields have different entropies. A 5-bucket numeric field has ~2.3 bits of entropy while a boolean has ~1; dividing by those different denominators makes the resulting strengths incomparable, and systematically understates the high-entropy field.

For cross-pair comparison use `mutual_information`:

```
I(X;Y) = H(Y) - H(Y|X) = H(X) - H(X|Y)
```

It is symmetric, measured in bits, and identical for both directions of a pair — so it ranks fields on one common scale regardless of their individual entropies.

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

Output is a flat array with one entry per direction per pair, sorted by `relationship_strength` descending (ties broken on the path pair, so ordering is fully deterministic).

```bash
vajra invariants records.json --format json --quiet
```

```json
[
  {
    "field_x": "$.fine",
    "field_y": "$.coarse",
    "conditional_entropy": 0.0,
    "mean_pmi": 1.0,
    "mutual_information": 0.9999999999999999,
    "relationship_strength": 1.0,
    "field_x_binned": false,
    "field_y_binned": false
  },
  {
    "field_x": "$.coarse",
    "field_y": "$.fine",
    "conditional_entropy": 1.5849625007211563,
    "mean_pmi": 1.0,
    "mutual_information": 0.9999999999999999,
    "relationship_strength": 0.3868528072345415,
    "field_x_binned": false,
    "field_y_binned": false
  }
]
```

Both rows describe the same pair. `conditional_entropy` and `relationship_strength` differ because they are directional; `mean_pmi` and `mutual_information` are symmetric and match.

To rank every field by how much it tells you about one target field, filter on that target and sort by `mutual_information`:

```bash
vajra invariants records.ndjson --top-k 400 --format json --quiet \
  | jq -r '.[] | select(.field_y == "$.label")
           | "\(.mutual_information)\t\(.field_x)"' | sort -rn
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

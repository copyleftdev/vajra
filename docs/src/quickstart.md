# Quickstart

You have 60 seconds. Let us not waste them.

---

## Install

From crates.io:

```bash
cargo install vajra-cli
```

From source:

```bash
git clone https://github.com/zuub-don/vajra
cd vajra
cargo build --release
# Binary lands at ./target/release/vajra
```

Verify:

```bash
vajra --help
```

---

## Four Commands That Prove the Point

### 1. Inspect a JSON document

Feed Vajra a medical claim. Get back its skeleton — every path, every type, every fingerprint.

```bash
vajra inspect claim.json
```

```text
=== Document Metadata ===
  Total nodes:    847
  Max depth:      6
  Distinct paths: 23
  Raw size:       14208 bytes

=== Wildcard Paths ===
  PATH                                           TYPE      COUNT  INSTABILITY  NULL_RATE
  $                                              object        1       0.0000     0.0000
  $.claims                                       array         1       0.0000     0.0000
  $.claims[*]                                    object        1       0.0000     0.0000
  $.claims[*].patient.id                         string        1       0.0000     0.0000
  $.claims[*].diagnosis[*].code                  string        2       0.0000     0.0000
  $.claims[*].service_lines[*].procedure_code    string       14       0.0000     0.0000
  $.claims[*].service_lines[*].charge_amount     number       14       0.0000     0.0000
  $.claims[*].service_lines[*].allowed_amount    number       11       0.0000     0.2143
  $.claims[*].service_lines[*].status            string       14       0.0000     0.0000

=== Fingerprints ===
  Path set:    a1b2c3d4e5f6...
  Typed path:  f7e8d9c0b1a2...
  Shape:       1234abcd5678...

=== Domain Type Recognition ===
  $.claims[*].diagnosis[*].code           E11.9      ICD-10-CM
  $.claims[*].service_lines[*].procedure_code  99213  CPT
```

Every path. Every type. Every structural fingerprint. Domain-specific codes recognized automatically. Zero configuration.

### 2. Generate an essence

Compress the entire document into what matters, shaped for a specific audience.

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

What This Likely Means:
  - Most of the claim is consistent and well-formed.
  - A subset of service lines appears incomplete or differently processed.
  - The repeated adjustment code points to a systematic issue.
```

Same command, different audience:

```bash
vajra essence claim.json --profile ai --format json --budget 500
```

```json
{
  "vajra_essence": {
    "version": "0.1.0",
    "profile": "ai",
    "structure": {
      "root_type": "object",
      "total_nodes": 847,
      "distinct_paths": 23,
      "max_depth": 6
    },
    "dominant_motif": {
      "path": "$.claims[0].service_lines[*]",
      "count": 14,
      "shape_hash": "f2c1..."
    },
    "anomalies": [
      {"path": "$.claims[0].service_lines[2,7,11].allowed_amount", "type": "missing", "severity": 4.2},
      {"path": "$.claims[0].diagnosis[1]", "type": "structural_deviation", "severity": 3.1}
    ]
  }
}
```

### 3. Detect drift between versions

Compare yesterday's API response to today's. Find what changed and how much it changed.

```bash
vajra drift baseline.json current.json
```

```text
Drift Report: baseline.json -> current.json
Structural similarity: 0.94 (Jaccard)

Added paths (2):
  $.response.metadata.processing_flags    [array of strings]
  $.response.metadata.api_version         [string]

Removed paths (0): none

Type changes (1):
  $.response.items[*].quantity            string -> number

Distribution shifts (1):
  $.response.items[*].status              JSD: 0.34
    before: {"active": 0.82, "pending": 0.15, "error": 0.03}
    after:  {"active": 0.61, "pending": 0.12, "error": 0.27}
    note: "error" rate increased 9x

Overall severity: MEDIUM
```

Two paths added. One type migrated. The `error` rate in `status` jumped ninefold. Vajra found all of it in one pass.

### 4. Surface anomalies

Find what deviates from the population — without defining what "normal" looks like.

```bash
vajra anomalies claims_batch.ndjson
```

```text
=== Anomaly Report ===
Records analyzed: 1,247
Anomalies found:  8

Numeric outliers:
  $.claims[*].service_lines[*].charge_amount
    Record 834: value 47,250.00 (z_MAD = 6.3, median = 285.00, MAD = 195.00)
    Record 1102: value 0.01 (z_MAD = -4.8)

Rarity outliers:
  $.claims[*].status
    Record 419: value "voided" (self-information = 10.3 bits, seen 1/1247)

Structural deviations:
  Record 662: missing 4 paths present in 99%+ of records
    - $.claims[*].subscriber.group_number
    - $.claims[*].subscriber.member_id
    - $.claims[*].provider.npi
    - $.claims[*].provider.taxonomy

Type instability:
  $.claims[*].service_lines[*].quantity
    Records 88, 204, 917: string where number expected (instability = 0.002)
```

Eight anomalies across four dimensions. Every one carries its score, its evidence, and the statistical context that makes it interpretable.

---

## What Just Happened

You did not configure a schema. You did not define rules. You did not train a model.

Vajra read the raw structure, computed its statistical profile, and surfaced what deviates from the population — deterministically, explainably, in seconds.

That is the point.

---

## Next Steps

- [Philosophy](./philosophy.md) — why Vajra exists and what it refuses to be
- [Commands](./commands.md) — all 11 commands at a glance
- [Profiles](./profiles.md) — tune the lens for your audience
- [Algorithms](./algorithms.md) — the mathematics behind every score

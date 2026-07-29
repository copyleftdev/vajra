# batch

`batch` runs parallel analysis across the files in a directory. It produces aggregated statistics, per-file summaries, and batch-level observations — processing hundreds or thousands of files in seconds via Rayon-based parallelism.

Where single-document commands analyze one file, `batch` analyzes the population.

---

## Usage

```bash
vajra batch <directory> [flags]
```

**Arguments:**

| Argument | Description |
|---|---|
| `<directory>` | Path to a directory containing files to analyze |

**Flags:**

| Flag | Description | Default |
|---|---|---|
| `--format <fmt>` | Output format: `text`, `json`, `markdown`, `compact-ai` | `text` |
| `--profile <name>` | Concern profile for essence generation | `engineer` |
| `--input-format <fmt>` | Override auto-detected input format | auto |
| `--lang <lang>` | Source language, with `--input-format source` | auto |
| `--streaming` | Force streaming mode for each file | off |
| `--redact` | Apply built-in redaction before output | off |
| `--quiet` | Suppress progress output | off |

---

## Which Files Get Analyzed

The directory scan selects `.json` files by default, or source files when `--input-format source` is given:

```bash
vajra batch ./payloads                                          # .json files
vajra batch ./src --input-format source --lang javascript       # source files
```

Everything else in the directory is **skipped and reported**, never silently dropped. Both output modes state the count, and JSON mode lists the names:

```json
{
  "total_documents": 3,
  "skipped_count": 4,
  "skipped": ["p.js", "q.js", "r.js", "s.js"]
}
```

Check `skipped_count` before trusting a batch result. A run that analyzed 3 of 19 files and one that analyzed all 3 files present are otherwise indistinguishable.

Subdirectories are ignored and are not counted as skipped — the scan is not recursive. Note that the directory scan does not pick up `.yaml`, `.csv`, or `.ndjson` by extension; those files are reported as skipped. Pass them to a single-document command instead.

If no file matches, `batch` fails and reports how many files were present but unselected, so "empty directory" is distinguishable from "wrong `--input-format`".

---

## What It Does

1. **Discovers files.** Scans the directory as described above, partitioning it into selected and skipped files.

2. **Parallel analysis.** Each file is analyzed independently using Rayon's work-stealing thread pool. On an 8-core machine, 8 files are analyzed simultaneously.

3. **Per-file statistics.** For each file: node count, path count, depth, fingerprint, anomaly count.

4. **Aggregated statistics.** Across the entire batch: merged frequency distributions, merged DDSketch quantiles, population-level entropy, cross-file type stability.

5. **Batch-level observations.** Structural families (via clustering), population anomalies, files that deviate from the batch norm.

---

## Example: Text Output

```bash
vajra batch ./claims/
```

```text
=== Batch Analysis ===
Directory: ./claims/
Files processed: 247
Total nodes: 208,729
Processing time: 1.4s (148,378 nodes/s)

=== Per-File Summary ===
  FILE                  NODES  PATHS  DEPTH  ANOMALIES  FINGERPRINT
  claim_001.json          847     23      6          0  a1b2c3d4...
  claim_002.json          891     23      6          0  a1b2c3d4...
  claim_003.json          723     23      6          1  a1b2c3d4...
  claim_048.json         1102     27      7          0  e5f6a7b8...
  claim_199.json          412     18      5          3  c9d0e1f2...
  ... (242 more files)

=== Structural Families ===
  Family 1: 198 files (80.2%) — 23 paths, signature a1b2c3d4...
  Family 2:  41 files (16.6%) — 27 paths, signature e5f6a7b8...
  Family 3:   8 files ( 3.2%) — 18 paths, signature c9d0e1f2...

=== Aggregated Statistics ===
  $.claims[*].service_lines[*].charge_amount
    Population median: 285.00
    Population MAD: 195.00
    Population p95: 1,420.00
    Cross-file consistency: high (coefficient of variation = 0.12)

  $.claims[*].service_lines[*].status
    Population entropy: 1.45 bits
    Dominant value: "adjudicated" (72.3%)
    Cardinality: 5 values across batch

=== Batch-Level Anomalies ===
  claim_199.json: structural outlier (Jaccard distance 0.31 from dominant family)
  claim_201.json: structural outlier (Jaccard distance 0.28 from dominant family)
  claim_834.json: contains numeric outlier (charge_amount = 47,250.00, z_MAD = 6.3)
```

---

## Example: JSON Output

```bash
vajra batch ./claims/ --format json
```

```json
{
  "directory": "./claims/",
  "files_processed": 247,
  "total_nodes": 208729,
  "processing_time_ms": 1400,
  "per_file": [
    {
      "file": "claim_001.json",
      "nodes": 847,
      "paths": 23,
      "depth": 6,
      "anomaly_count": 0,
      "fingerprint": "a1b2c3d4..."
    }
  ],
  "structural_families": [
    {
      "id": 0,
      "count": 198,
      "percentage": 80.2,
      "distinct_paths": 23,
      "signature": "a1b2c3d4..."
    },
    {
      "id": 1,
      "count": 41,
      "percentage": 16.6,
      "distinct_paths": 27,
      "signature": "e5f6a7b8..."
    },
    {
      "id": 2,
      "count": 8,
      "percentage": 3.2,
      "distinct_paths": 18,
      "signature": "c9d0e1f2..."
    }
  ],
  "aggregated_stats": {
    "$.claims[*].service_lines[*].charge_amount": {
      "population_median": 285.0,
      "population_mad": 195.0,
      "population_p95": 1420.0
    }
  },
  "batch_anomalies": [
    {
      "file": "claim_199.json",
      "type": "structural_outlier",
      "jaccard_distance": 0.31
    },
    {
      "file": "claim_834.json",
      "type": "numeric_outlier",
      "path": "$.claims[*].service_lines[*].charge_amount",
      "value": 47250.0,
      "z_mad": 6.3
    }
  ]
}
```

---

## Parallelism and Performance

Batch uses Rayon's work-stealing thread pool. The number of threads defaults to the number of CPU cores.

**Performance targets:**

| Batch Size | Target |
|---|---|
| 100 files, ~1 MB each | < 5 seconds |
| 1,000 files, ~1 MB each | < 30 seconds |
| 10,000 files, ~100 KB each | < 30 seconds |

DDSketch instances are computed per-file and merged globally with no accuracy loss — this is the key property that makes parallel batch processing exact rather than approximate.

---

## When to Use It

- **Daily batch monitoring.** Run `batch` on each day's incoming data. Track structural families, anomaly counts, and distribution shifts over time.
- **Pre-processing audit.** Before feeding a batch to a downstream system, run `batch` to verify structural consistency and flag outliers.
- **Population baselines.** Establish population-level statistics (median charge amount, expected null rates, typical structural signature) that individual-file analysis can compare against.
- **Quick directory survey.** "What is in this folder?" — `batch` answers in seconds.

---

## Pairs Well With

- [`cluster`](./cmd-cluster.md) — batch includes lightweight clustering; `cluster` provides detailed similarity analysis
- [`anomalies`](./cmd-anomalies.md) — batch flags files with anomalies; drill into specific files for details
- [`drift`](./cmd-drift.md) — compare today's batch aggregates to yesterday's for population-level drift
- [`essence`](./cmd-essence.md) — run essence on specific files that batch identified as notable

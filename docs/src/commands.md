# Commands

Each command does one thing. They compose.


## A note on `--format`

`json` and `text` are implemented everywhere. `markdown` and `compact-ai` need a real renderer, and coverage is partial:

| format | commands with a real renderer |
|---|---|
| `markdown` | `essence`, `anomalies`, `stats`, `invariants`, `fingerprint`, `separation` |
| `compact-ai` | `essence` |

For every other command those formats produce the text output verbatim. Commands are being migrated onto a shared renderer, and a command only joins the table above once it genuinely renders that format — so the notice below can never claim more than is true.

Rather than accept the flag and quietly ignore it, those commands now say so on stderr:

```console
$ vajra stats data.json --format markdown
vajra: `stats` has no markdown renderer; output is the text format. Use --format json for a machine-readable form.
```

`--quiet` suppresses the notice, and stdout is unaffected either way. Use `--format json` for anything machine-readable.

---


## Reference Table

| Command | Purpose | Input | Key Output |
|---|---|---|---|
| [`inspect`](./cmd-inspect.md) | Full structural analysis | Single document | Paths, types, fingerprints, domain hints |
| [`stats`](./cmd-stats.md) | Statistical summary | Single document | Entropy, frequency, distributions, null rates |
| [`anomalies`](./cmd-anomalies.md) | Anomaly detection | Single or batch | Outliers, rarity, structural deviations |
| [`fingerprint`](./cmd-fingerprint.md) | Structural fingerprints | Single document | BLAKE3 hashes, MinHash signature |
| [`essence`](./cmd-essence.md) | Concern-oriented reduction | Single document | Compressed, ranked, profile-shaped output |
| [`drift`](./cmd-drift.md) | Schema drift detection | Two documents | Added/removed paths, type changes, JSD |
| [`cluster`](./cmd-cluster.md) | Similarity clustering | Multiple documents | Cluster assignments, centroids, outliers |
| [`invariants`](./cmd-invariants.md) | Cross-field relationships | Single or batch | Conditional entropy, PMI, dependencies |
| [`separation`](./cmd-separation.md) | Labelled feature evaluation | Labelled batch | MI, AUC, effect size, priced precision |
| [`query`](./cmd-query.md) | Path-based query with analysis functions | Single document | Filtered analysis results |
| [`batch`](./cmd-batch.md) | Parallel batch analysis | Directory | Aggregated stats, per-file summaries |
| `profiles` | List available profiles | None | Built-in and custom profile descriptions |

---

## Global Flags

Every command accepts these flags:

```
--format <text|json|markdown|compact-ai>   Output format (default: text)
--profile <name>                           Concern profile (default: engineer)
--config <path>                            Path to TOML config with custom profiles
--budget <N>                               Token budget for essence output
--streaming                                Force streaming mode (bounded memory)
--input-format <format>                    Override input format auto-detection
--redact                                   Apply built-in redaction patterns
--quiet                                    Suppress progress output
--explain                                  Include score decomposition in output
```

---

## Quick Examples

### Inspect

```bash
vajra inspect claim.json
vajra inspect claim.json --format json
cat payload.json | vajra inspect -
```

### Stats

```bash
vajra stats claim.json
vajra stats claim.json --format json
```

### Anomalies

```bash
vajra anomalies claim.json
vajra anomalies claims_batch.ndjson --format json
```

### Fingerprint

```bash
vajra fingerprint claim.json
vajra fingerprint claim.json --format json
```

### Essence

```bash
vajra essence claim.json --profile staff
vajra essence claim.json --profile ai --format compact-ai --budget 500
vajra essence claim.json --profile auditor --format markdown
```

### Drift

```bash
vajra drift v1.json v2.json
vajra drift baseline.json candidate.json --format json
```

### Cluster

```bash
vajra cluster batch/*.json
vajra cluster file1.json file2.json file3.json --format json
```

### Invariants

```bash
vajra invariants claims_batch.ndjson
vajra invariants claims_batch.ndjson --top-k 100
```

### Query

```bash
vajra query claim.json 'entropy($.claims[*].status) > 0.5'
vajra query claim.json '$.claims[*].service_lines[*].charge_amount'
```

### Batch

```bash
vajra batch ./claims_directory/
vajra batch ./claims_directory/ --format json --profile auditor
```

### Profiles

```bash
vajra profiles
vajra profiles --config custom.toml
```

---

## Input Conventions

All commands that accept `<input>` understand:

- **File path:** `claim.json`, `./data/payload.yaml`
- **Stdin:** `-` (pipe data in)
- **Directory:** `./batch/` (processes all supported files)
- **Compressed:** `.json.gz`, `.json.zst` (auto-decompressed)
- **HTTP URL:** `https://api.example.com/data.json` (fetched, then analyzed)

Format is auto-detected from extension and content. Override with `--input-format`.

See [Input Formats](./formats.md) for the full list.

---

## Output Conventions

All commands emit to stdout. All commands support `--format json` for machine-readable output. Diagnostics and errors go to stderr.

The `--explain` flag adds score decomposition to essence and anomaly output — showing exactly which dimensions contributed to each observation's ranking.

The `--redact` flag applies built-in pattern redaction (SSN, email, phone, credit card) before any output is rendered. The essence never sees unredacted values.

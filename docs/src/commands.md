# Commands

Each command does one thing. They compose.


## A note on `--format`

`text`, `json` and `markdown` are implemented for every command. **`compact-ai` has a real renderer only in `cascade`, `compare`, `essence` and `score`** — for the rest it produces the text output verbatim, and says so:

```console
$ vajra stats data.json --format compact-ai
vajra: `stats` has no compact-ai renderer; output is the text format. Use --format json for a machine-readable form.
```

`--quiet` suppresses the notice, and stdout is unaffected either way.

Coverage is verified by measurement, not inspection: tests assert that every command/format pair claimed as renderer-backed produces output differing from that command's text form, **and** that unclaimed pairs genuinely do fall back. So the claim can neither over- nor under-claim.

---|---|
| `markdown` | `anomalies`, `cascade`, `cluster`, `compare`, `essence`, `fingerprint`, `governance`, `inspect`, `invariants`, `separation`, `stats` |
| `compact-ai` | `cascade`, `compare`, `essence`, `score` |

For every other command those formats produce the text output verbatim. Commands are being migrated onto a shared renderer. Membership of the table above is verified by measurement, not by inspection: tests assert that every listed pair produces output differing from that command's text form, **and** that every unlisted pair genuinely does fall through. So the table can neither over- nor under-claim.

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
--streaming                                Bounded memory for `stats` on JSON/NDJSON files; other
                                           commands and inputs load whole and say so
--input-format <format>                    Override input format auto-detection
--redact                                   Apply built-in redaction patterns
--provenance                               Record the build and schema version in the output
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

## `--provenance`

Records which build produced a result, so a stored artifact can be traced back to it. Off by default: attaching it unconditionally would change every consumer's output shape, and would break the byte-identical guarantee across builds — the property it exists to make checkable.

JSON output is wrapped uniformly, whether the command emits an object or an array:

```json
{
  "_vajra": {
    "version": "0.5.0",
    "build": "10422c6",
    "commit_date": "2026-07-30",
    "schema": 1,
    "command": "stats"
  },
  "data": { "paths": [ ... ] }
}
```

Text and Markdown gain a trailing line instead:

```
vajra 0.5.0 (10422c6 2026-07-30), output schema 1
```

`schema` is the version of the output contract, bumped when the shape or meaning of emitted fields changes. It is deliberately separate from the crate version, which is a release artefact: eight output changes shipped under an unchanged `0.5.0`, and an integer is something to branch on without parsing semver.

`build` carries the commit the binary was built from, with a `-dirty` suffix when the working tree had uncommitted changes. It uses the **commit** date, never the build time — a build timestamp would make the binary itself non-reproducible. Outside a git checkout it reads `unknown`, and `VAJRA_BUILD_COMMIT` overrides it for packagers building from a tarball.

Redaction runs before provenance, so `--redact --provenance` composes.

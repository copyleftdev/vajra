# inspect

The foundational command. `inspect` performs full structural analysis of a JSON document and reports every path, every type, every fingerprint, and every domain-recognized value it finds.

This is the command you reach for first. Before you know what you are looking for, `inspect` tells you what is there.

---

## Usage

```bash
vajra inspect <input> [flags]
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
| `--streaming` | Force streaming mode (bounded memory) | off |
| `--redact` | Apply built-in redaction before output | off |
| `--quiet` | Suppress progress output | off |

---

## What It Reports

### Document Metadata

Total node count, maximum nesting depth, number of distinct wildcard paths, raw byte size.

### Wildcard Path Table

Every distinct path in the document, normalized with `[*]` for array indices. For each path:

- **Dominant type** — the most common JSON type at that path
- **Count** — how many times that path appears across the document
- **Type instability** — fraction of observations where the type differs from the dominant type (0.0 = perfectly stable)
- **Null rate** — fraction of observations that are null

### Structural Fingerprints

Three BLAKE3-based fingerprints:

- **Path set fingerprint** — hash of the sorted set of distinct wildcard paths. Captures *what fields exist*.
- **Typed path fingerprint** — hash of sorted (path, dominant_type) pairs. Captures *what fields exist and what types they carry*.
- **Shape fingerprint** — Merkle subtree hash computed bottom-up. Captures the full structural shape including nesting.

### Domain Type Recognition

Values matched against domain-specific type recognizers (e.g., the medical plugin recognizes ICD-10-CM codes, CPT codes, NPI numbers). Each match reports the path, the value, and the recognized type.

---

## Example: Text Output

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
  PATH                                              TYPE      COUNT  INSTABILITY  NULL_RATE
  $                                                 object        1       0.0000     0.0000
  $.claims                                          array         1       0.0000     0.0000
  $.claims[*]                                       object        1       0.0000     0.0000
  $.claims[*].claim_id                              string        1       0.0000     0.0000
  $.claims[*].patient                               object        1       0.0000     0.0000
  $.claims[*].patient.id                            string        1       0.0000     0.0000
  $.claims[*].patient.name                          string        1       0.0000     0.0000
  $.claims[*].diagnosis                             array         1       0.0000     0.0000
  $.claims[*].diagnosis[*]                          object        2       0.0000     0.0000
  $.claims[*].diagnosis[*].code                     string        2       0.0000     0.0000
  $.claims[*].diagnosis[*].system                   string        2       0.0000     0.0000
  $.claims[*].service_lines                         array         1       0.0000     0.0000
  $.claims[*].service_lines[*]                      object       14       0.0000     0.0000
  $.claims[*].service_lines[*].procedure_code       string       14       0.0000     0.0000
  $.claims[*].service_lines[*].charge_amount        number       14       0.0000     0.0000
  $.claims[*].service_lines[*].allowed_amount       number       11       0.0000     0.2143
  $.claims[*].service_lines[*].status               string       14       0.0000     0.0000
  $.claims[*].service_lines[*].service_date         string       14       0.0000     0.0000
  $.claims[*].service_lines[*].adjustment           object       14       0.0000     0.0000
  $.claims[*].service_lines[*].adjustment.reason    string       14       0.0000     0.0000
  $.claims[*].service_lines[*].adjustment.amount    number       14       0.0000     0.0000
  $.claims[*].provider.npi                          string        1       0.0000     0.0000
  $.claims[*].subscriber.member_id                  string        1       0.0000     0.0000

=== Fingerprints ===
  Path set:    a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2
  Typed path:  f7e8d9c0b1a2f7e8d9c0b1a2f7e8d9c0b1a2f7e8d9c0b1a2f7e8d9c0b1a2f7e8
  Shape:       1234abcd56781234abcd56781234abcd56781234abcd56781234abcd56781234abcd

=== Domain Type Recognition ===
  $.claims[*].diagnosis[*].code                  E11.9      ICD-10-CM
  $.claims[*].diagnosis[*].code                  J44.1      ICD-10-CM
  $.claims[*].service_lines[*].procedure_code    99213      CPT
  $.claims[*].service_lines[*].procedure_code    99214      CPT
  $.claims[*].provider.npi                       1234567890 NPI
```

---

## Example: JSON Output

```bash
vajra inspect claim.json --format json
```

```json
{
  "metadata": {
    "total_nodes": 847,
    "max_depth": 6,
    "distinct_paths": 23,
    "raw_size_bytes": 14208
  },
  "paths": [
    {
      "path": "$.claims[*].service_lines[*].charge_amount",
      "dominant_type": "number",
      "count": 14,
      "type_instability": 0.0,
      "null_rate": 0.0
    },
    {
      "path": "$.claims[*].service_lines[*].allowed_amount",
      "dominant_type": "number",
      "count": 11,
      "type_instability": 0.0,
      "null_rate": 0.2143
    }
  ],
  "fingerprints": {
    "path_set": "a1b2c3d4...",
    "typed_path": "f7e8d9c0...",
    "shape": "1234abcd..."
  },
  "domain_hints": [
    {
      "path": "$.claims[*].diagnosis[*].code",
      "value": "E11.9",
      "recognized_type": "ICD-10-CM"
    }
  ]
}
```

`structural_findings` also appears when a domain plugin recognises the document's shape — a package manifest declaring an install-time hook, for example. It is omitted entirely when empty, so its absence means "nothing recognised", not "not checked". See [Domain Plugins](./plugins.md#structural-detectors).


---

## When to Use It

- **First contact with unfamiliar data.** You just received a JSON payload and need to know its shape.
- **Schema exploration.** What paths exist? What types do they carry? How stable are those types?
- **Domain validation.** Does the medical plugin recognize the codes in this claim? Is the NPI present?
- **Regression gating.** Fingerprint the output of an API endpoint. If the fingerprint changes, the schema changed.

---

## Pairs Well With

- [`stats`](./cmd-stats.md) — once you know the structure, `stats` tells you the statistical profile
- [`fingerprint`](./cmd-fingerprint.md) — if you only need the fingerprints (faster, less output)
- [`drift`](./cmd-drift.md) — compare two `inspect` snapshots to find what changed
- [`essence`](./cmd-essence.md) — when you want the compressed version, not the full inventory

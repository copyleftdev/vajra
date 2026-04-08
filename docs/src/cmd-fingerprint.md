# fingerprint

`fingerprint` computes structural fingerprints for a JSON document — cryptographic hashes that capture *what the document looks like* independently of its values.

Two documents with the same fingerprint have the same structure. If the fingerprint changes, the schema changed. This is the fastest possible regression check.

---

## Usage

```bash
vajra fingerprint <input> [flags]
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
| `--streaming` | Force streaming mode | off |
| `--redact` | Apply built-in redaction before output | off |
| `--quiet` | Suppress progress output | off |

---

## Fingerprint Types

### Path Set Fingerprint

BLAKE3 hash of the sorted set of distinct wildcard paths. Captures **what fields exist**, ignoring their types and values.

Two documents with the same path set fingerprint have identical field structures — the same keys at the same nesting levels, even if every value differs.

### Typed Path Fingerprint

BLAKE3 hash of sorted `(path, dominant_type)` pairs. Captures **what fields exist and what types they carry**.

This is strictly more specific than the path set fingerprint. A type migration (e.g., `quantity` changing from string to number) changes the typed path fingerprint but not the path set fingerprint.

### Shape Fingerprint (Merkle)

Bottom-up hash computed via Merkle subtree hashing:

- Leaf nodes hash their type
- Objects hash the sorted concatenation of `(key, child_hash)` pairs
- Arrays hash the concatenation of child hashes

The root hash is the shape fingerprint. This captures the **full structural shape** including nesting hierarchy.

A critical secondary benefit: subtree hashes at every node enable motif detection as a byproduct. Identical subtrees produce identical hashes. This falls out of a single O(n) traversal.

### MinHash Signature

A 128-hash MinHash signature over the path set, enabling constant-time Jaccard similarity estimation between documents. Used internally by `cluster` and `drift`, but exposed here for direct access.

---

## Example: Text Output

```bash
vajra fingerprint claim.json
```

```text
=== Fingerprints ===
  Path set:    a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2
  Typed path:  f7e8d9c0b1a2f7e8d9c0b1a2f7e8d9c0b1a2f7e8d9c0b1a2f7e8d9c0b1a2f7e8
  Shape:       1234abcd56781234abcd56781234abcd56781234abcd56781234abcd56781234abcd
  MinHash:     [64 x u64 values]

=== Subtree Motifs ===
  Hash d4e5f6a1... appears 14 times (service line object)
  Hash b2c3d4e5... appears 2 times (diagnosis object)
```

---

## Example: JSON Output

```bash
vajra fingerprint claim.json --format json
```

```json
{
  "path_set": "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
  "typed_path": "f7e8d9c0b1a2f7e8d9c0b1a2f7e8d9c0b1a2f7e8d9c0b1a2f7e8d9c0b1a2f7e8",
  "shape": "1234abcd56781234abcd56781234abcd56781234abcd56781234abcd56781234abcd",
  "minhash": [18446744073709551615, 12345678901234567890, "..."],
  "motifs": [
    {
      "hash": "d4e5f6a1...",
      "count": 14,
      "node_count": 8,
      "representative_path": "$.claims[*].service_lines[*]"
    },
    {
      "hash": "b2c3d4e5...",
      "count": 2,
      "node_count": 3,
      "representative_path": "$.claims[*].diagnosis[*]"
    }
  ]
}
```

---

## Use Cases

### CI Regression Check

Store the fingerprint of your API's response format. On every deploy, compare:

```bash
# Capture baseline
vajra fingerprint api_response.json --format json > baseline_fp.json

# On each CI run
vajra fingerprint today_response.json --format json > current_fp.json
diff baseline_fp.json current_fp.json
```

If the path set fingerprint changed, fields were added or removed. If the typed path fingerprint changed, a type migrated. If only the shape fingerprint changed, the nesting structure shifted.

### Quick Structural Comparison

```bash
vajra fingerprint file_a.json --format json | jq .path_set
vajra fingerprint file_b.json --format json | jq .path_set
```

Same hash? Same structure. Different hash? Feed them to `drift` for the details.

### Motif Discovery

The motif section reveals repeated substructures. In a medical claim, you will see the service line object repeated 14 times with the same hash — proof that those 14 elements are structurally identical.

---

## When to Use It

- **Schema regression gating.** The fastest way to detect structural changes.
- **Deduplication.** Documents with identical shape fingerprints are structurally identical.
- **Batch pre-screening.** Fingerprint a batch before clustering to quickly identify structural families.
- **Motif identification.** What substructures repeat, and how many times?

---

## Pairs Well With

- [`drift`](./cmd-drift.md) — when fingerprints differ, `drift` tells you exactly what changed
- [`cluster`](./cmd-cluster.md) — uses MinHash signatures internally for similarity estimation
- [`inspect`](./cmd-inspect.md) — `fingerprint` is the focused subset of what `inspect` computes
- [`essence`](./cmd-essence.md) — motif discovery feeds directly into essence compression

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
| `--min-nodes <N>` | Withhold hashes for documents with fewer than N nodes (0 = never) | `0` |
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

## Complexity Floor: `node_count` and `--min-nodes`

Structural hashing deliberately ignores string contents and identifier names — that is what makes it catch renamed and obfuscated code. The consequence is that the space of distinct *small* shapes is tiny, so trivially small documents collide regardless of what they actually say:

```console
$ echo 'console.log("hello world");'              > a.js
$ echo 'console.log("send creds to evil.example");' > b.js
$ echo 'console.info("Hello World! ngab")'         > c.js

$ for f in a b c; do vajra fingerprint $f.js --input-format source --lang javascript \
    --format json --quiet | jq -r '"\(.node_count) \(.shape[0:16])"'; done
43 30ddb960025c2ffa
43 30ddb960025c2ffa
43 30ddb960025c2ffa
```

Three files with entirely different meanings, one hash. The same happens with JavaScript re-export stubs — `module.exports = require('./x')` is 53 nodes, and every package that ships one shares a shape with every other.

A shape match below some complexity is a true statement about the documents and a useless one about their relationship. Two things address that:

**`node_count`** is always reported, so you can see how much structure backed the hash and filter downstream.

**`--min-nodes N`** withholds the hashes when `node_count < N`. The result is still emitted — with `node_count`, `min_nodes` and `suppressed: true` — so a caller can tell "too simple to fingerprint" from "not analysed":

```console
$ vajra fingerprint stub.js --input-format source --lang javascript \
    --min-nodes 100 --format json --quiet
{
  "node_count": 53,
  "min_nodes": 100,
  "suppressed": true,
  "path_set": null,
  "typed_path": null,
  "shape": null,
  "repeated_motifs": []
}
```

Documents *at* the floor are kept — the test is `node_count < min_nodes` — and clearing the floor never changes the hash.

**Choosing a floor is a judgement call, not a solved problem.** There is no universal value, and no floor cleanly separates "trivial" from "small but meaningful": a one-line `console.log` beacon is 43 nodes while a benign re-export stub is 53, so any floor that drops the stub also drops the beacon. Pick the floor from your own corpus, and treat a hash match near the floor as a lead rather than evidence.

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
vajra fingerprint claim.json --format json --quiet
```

```json
{
  "node_count": 122,
  "suppressed": false,
  "path_set": "db22fd53d848cc7ed5f7b6fe8126a62b775a736dcc5f8e28cd42ed2492af48b4",
  "typed_path": "fb06293ab115045ad9aecec5aa192d63be37f8946f42806a0b252aa439ef7549",
  "shape": "646d525a06fb8b816b4ed63a137ba30880ea3f113a538c23352e5e02fc641230",
  "repeated_motifs": [
    { "hash": "bf0b731c90564bc8c1a8b8078964f3fb4e20636f1beb54ff1cfecb06a7ca2ac8", "count": 55 },
    { "hash": "88d3ce9a7ddc1cdb461b8ff3d6106ad21f17d8e970d3f69cb6e5fdc0c1d20f39", "count": 44 }
  ]
}
```

`repeated_motifs` lists subtree shapes occurring more than once, sorted by count descending and then by hash, so the ordering is fully deterministic.

`min_nodes` appears only when a floor was requested. When a floor suppresses the result, `path_set`, `typed_path` and `shape` are `null` and `repeated_motifs` is empty — see the complexity-floor section above.

In `--streaming` mode `shape` is always `null`: the Merkle shape hash needs the whole tree in memory. `path_set`, `typed_path` and `node_count` are still produced.

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

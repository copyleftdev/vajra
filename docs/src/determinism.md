# Determinism

Determinism in Vajra is not a feature. It is a structural guarantee.

Given identical input bytes, identical configuration, and identical Vajra version, the output is identical — byte for byte. Fingerprints, scores, orderings, essence text, anomaly rankings. Every run. Every platform. Every time.

This is what makes Vajra trustworthy in CI pipelines, audits, compliance workflows, and AI systems that depend on stable preprocessing.

---

## The Guarantee

**Identical:**
- Input bytes
- Configuration (profile, flags, config file)
- Vajra version

**Produces identical:**
- Fingerprints
- Scores (to floating-point bit-level)
- Orderings
- Essence text (byte-for-byte)
- Anomaly rankings

---

## Sources of Nondeterminism and How Each Is Eliminated

### The HashMap Rule

**Problem:** Rust's `HashMap` uses a random seed for its hash function (SipHash with random key by default). Iteration order is nondeterministic. Any code that iterates a `HashMap` and includes the iteration order in output produces nondeterministic results.

**Mitigation:** `BTreeMap` is used for all externally-visible orderings. `HashMap` is permitted only for internal scratch computations where iteration order is never observed in output.

This is enforced by code review and tested by the determinism test suite. If a `HashMap` iteration order leaks into output, the 10-run determinism test catches it immediately.

### The Thread Scheduling Rule

**Problem:** Rayon's parallel batch processing schedules work across threads nondeterministically. If results are merged in arrival order, the output depends on thread scheduling.

**Mitigation:** Deterministic merge order. After parallel analysis, results are sorted by input identity (file path or record index) before merging. Parallel execution affects speed, never output.

```rust
// Parallel analysis
let results: Vec<_> = files.par_iter()
    .map(|f| analyze(f))
    .collect();

// Deterministic merge — sorted by input identity, not arrival order
let mut results = results;
results.sort_by_key(|r| r.input_path.clone());
```

### The Floating-Point Accumulation Rule

**Problem:** Floating-point addition is not associative. `(a + b) + c` can differ from `a + (b + c)` at the bit level. If summation order varies (due to thread scheduling, hash map iteration, or unstable sorting), floating-point results drift.

**Mitigation:** Fixed traversal order. All traversals are DFS, left-to-right. All summations occur in deterministic order defined by the path trie's BTreeMap-based key ordering. The traversal order is a function of the input alone.

### The Seed Rule

**Problem:** MinHash and SimHash use hash functions that can be seeded. Different seeds produce different signatures (and different similarity estimates, cluster assignments, etc.).

**Mitigation:** Default seed is 0. The `--seed` flag provides explicit control.

```bash
vajra cluster batch/*.json              # seed = 0 (default, deterministic)
vajra cluster batch/*.json --seed 42    # seed = 42 (different but still deterministic)
```

Same seed + same input = same output. Different seed = potentially different output. Both are deterministic within their seed.

### The ryu Rule

**Problem:** Floating-point to string conversion varies across platforms. Rust's default `Display` for `f64` can produce different decimal representations on different architectures or with different optimization levels.

**Mitigation:** All float-to-string conversion uses the `ryu` crate — Ulf Adams' algorithm (2018) for shortest round-trip-safe decimal representation. `ryu` is deterministic and platform-independent. The same `f64` bit pattern produces the same string on every platform Vajra supports.

```rust
// Not this:
format!("{}", value)         // platform-dependent

// This:
ryu::Buffer::new().format(value)  // deterministic, platform-independent
```

### The Canonicalization Rule

**Problem:** JSON objects are unordered by specification. `{"a": 1, "b": 2}` and `{"b": 2, "a": 1}` are semantically identical but textually different. Any operation that depends on key order (hashing, fingerprinting, rendering) must first impose a deterministic order.

**Mitigation:** RFC 8785 canonicalization. Keys sorted by UTF-16 code unit sequence (the RFC's specified ordering). Numbers formatted deterministically. Unicode NFC normalized. Applied before any hashing, fingerprinting, or comparison operation.

### The Unicode Rule

**Problem:** The same visual string can have multiple Unicode representations. "e with acute accent" can be a single codepoint (U+00E9) or a base character plus combining mark (U+0065 U+0301). If these are treated as different strings, frequency counts, entropy, and fingerprints diverge.

**Mitigation:** Unicode NFC normalization (UAX #15) applied during canonicalization. All string comparisons, frequency counting, and hashing operate on NFC-normalized forms.

---

## Verifying Determinism: The 10-Run Test

The determinism test suite runs every command on every document in the test corpus:

1. Run Vajra N times (N >= 10) with identical configuration.
2. Assert byte-identical output across all runs.
3. Run with `--seed 0` and `--seed 42` — outputs may differ between seeds.
4. Run each seed N times — assert identical within-seed output.

```bash
# Manual verification
for i in $(seq 1 10); do
  vajra essence claim.json --profile engineer --format json > "run_$i.json"
done

# All files must be identical
md5sum run_*.json
# Every line shows the same hash
```

If any two runs produce different output, the determinism contract is broken. This test runs in CI on every commit.

---

## What Determinism Costs

The determinism guarantee imposes real engineering costs:

| Constraint | Cost | Payoff |
|---|---|---|
| BTreeMap everywhere | ~10-20% slower than HashMap for insertion-heavy code | Deterministic iteration order |
| Fixed traversal order | Cannot parallelize within-document traversal for speed | Deterministic accumulation |
| ryu for float formatting | Additional dependency | Platform-independent output |
| Seeded PRNG for MinHash | Cannot use hardware RNG for "better" randomness | Reproducible signatures |
| Deterministic merge order | Sorting step after parallel batch processing | Reproducible batch results |

Every cost is paid gladly. Determinism is not negotiable. Speed optimizations that violate it are rejected.

---

## What Determinism Does NOT Cover

Determinism applies to the mapping from (input, config, version) to output. It does not mean:

- **Different versions produce the same output.** Algorithm changes, bug fixes, and threshold adjustments may change output between versions. The version is part of the contract.
- **Different configs produce the same output.** Changing the profile, the seed, the budget, or any flag may change output. The config is part of the contract.
- **Streaming mode matches DOM mode exactly.** Streaming mode uses approximate algorithms (DDSketch, CMS) that produce bounded approximations of DOM mode's exact results. Both modes are internally deterministic. They may differ from each other within the documented error bounds.

---

## For Library Users

The determinism guarantee extends to the Rust library API. If you call the same analyzer with the same `Document` and the same configuration, you get the same result.

```rust
use vajra_core::Document;
use vajra_stats::StatsAnalyzer;
use vajra_types::Analyzer;

let doc = Document::parse_file("claim.json")?;

let stats1 = StatsAnalyzer.analyze(&doc)?;
let stats2 = StatsAnalyzer.analyze(&doc)?;

// stats1 and stats2 are identical at the bit level
assert_eq!(
    serde_json::to_string(&stats1)?,
    serde_json::to_string(&stats2)?
);
```

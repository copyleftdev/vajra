# Streaming

> **Status: `--streaming` is bounded for JSON and NDJSON files.**
>
> Records are pulled one at a time — NDJSON line by line, a top-level array
> element by element through a `SeqAccess` visitor — and each is converted to
> events in a reused buffer, so nothing grows with the record count. Measured
> peak RSS on `vajra stats`:
>
> | input | default (DOM) | `--streaming` |
> |---|---|---|
> | 15 MB / 120k records | 232 MB | **6.4 MB** |
> | 142 MB / 1.2M records | 2,284 MB | **6.3 MB** |
>
> Ten times the input, the same memory: bounded, not merely smaller.
>
> Two limits are real and stated rather than implied. Memory is bounded by the
> largest single **record**, not by the file — one enormous record is still
> materialised. And other inputs (stdin, URLs, git repositories, CSV, YAML,
> source code) go through readers that produce a whole document by
> construction; passing `--streaming` to those reports that the input was
> loaded whole rather than silently doing nothing.
>
> Past the exact-tracking threshold, results carry `exact: false`. See
> [Accuracy](#accuracy) below.

Vajra aims to handle JSON of any size: a 50 KB medical claim and a 10 GB event log entering the same pipeline. The streaming engine is what makes this possible for record-oriented JSON.

---

## Two Modes

### DOM Mode

For documents that fit in memory. The parser builds a full in-memory tree with random access to every node. All analysis passes can access any part of the document at any time.

**Parser:** `simd-json` at 2+ GB/s.  
**Memory:** O(n) where n = document size.  
**Activates:** By default, for documents below the streaming threshold (default 100 MB, configurable).

### Streaming Mode

For documents that exceed available memory. Records are pulled one at a time and converted to events individually, so memory is bounded by the largest record rather than the file. The reader emits events (start-object, key, value, end-object, start-array, end-array) and the analyzers update their accumulators incrementally.

**Memory:** O(p + s) where p = distinct paths and s = sum of sketch sizes. For typical JSON with < 1,000 distinct paths: < 10 MB regardless of document size.  
**Activates:** Automatically when document size exceeds the streaming threshold. Force with `--streaming`.

---

## The Two-Pass Hybrid Strategy

Streaming mode does not mean single-pass-only. Vajra uses a hybrid strategy that balances memory efficiency with analysis depth.

### Pass 1: Profile the Document

A single streaming pass collects:

- **Path extraction.** Every wildcard path discovered and registered in the path trie.
- **Frequency counting.** Value frequencies per path via Count-Min Sketch (conservative update).
- **Top-k identification.** Most frequent values per path via Space-Saving algorithm.
- **Type profiling.** Type distribution per path tracked via simple counters.
- **Numeric sketches.** DDSketch accumulators for every numeric path — percentiles, median, MAD.
- **Null and missingness tracking.** Per-path counters for null, absent, empty.
- **Entropy estimation.** Computed from CMS frequency estimates when exact counting exceeds memory.
- **Fingerprint accumulation.** Merkle hashes built incrementally as subtrees complete.

After Pass 1, Vajra has a complete statistical profile of the document without having held more than one event in memory at a time.

### Pass 2 (Optional): Selective DOM for High-Signal Subtrees

If the command requires rich analysis that streaming cannot provide (motif analysis, essence generation with deep context), Vajra can selectively parse high-signal subtrees into DOM.

The decision is based on Pass 1 results:

- Subtrees with high anomaly density are candidates for DOM parsing.
- Subtrees with high entropy fields that need value-level analysis.
- The dominant motif's representative instance.

Pass 2 is optional. Commands like `stats` and `fingerprint` need only Pass 1. Commands like `essence` may invoke Pass 2 for targeted depth.

---

## Sketch Data Structures in Streaming Mode

### DDSketch

**Role:** Numeric distribution analysis — percentiles, median, MAD.

One DDSketch per numeric path. Each sketch maintains O(log(max/min) / log(1 + alpha)) buckets. With alpha = 0.01 and financial data spanning $0.01 to $1,000,000, this is roughly 700 buckets — a few KB of memory per path.

**Key property:** Mergeability. When processing a batch in parallel, per-file DDSketch instances merge into a global sketch with zero accuracy loss.

```rust
// Streaming numeric stats
let mut stats = StreamingStatsAccumulator::default();
for event in parser {
    stats.on_event(&event?)?;
}
let result = stats.finalize()?;
// result.numeric_stats contains DDSketch-derived percentiles
```

### Count-Min Sketch (CMS)

**Role:** Frequency estimation for values, paths, and key names when cardinality exceeds configurable thresholds.

Default configuration: width = 2,718, depth = 5. Total memory: ~54 KB per sketch. Error guarantee: estimated count within 0.1% of total count with 99% probability.

**Activation:** Exact counting is preferred when it fits in memory. CMS activates as a fallback when distinct value count per path exceeds the threshold (default: 10,000 distinct values).

### Space-Saving

**Role:** Identifying top-k most frequent elements without storing all elements.

Maintains exactly k counters (default k = 100). Guaranteed to include every element whose true frequency exceeds N/k. Memory: k entries, a few KB.

---

## Memory Budget

The total streaming memory budget is bounded:

| Component | Memory |
|---|---|
| Path trie | O(p) where p = distinct wildcard paths |
| DDSketch (per numeric path) | ~3 KB per path |
| CMS (per high-cardinality path) | ~54 KB per path |
| Space-Saving (per path) | ~4 KB per path (k=100) |
| Type counters (per path) | ~48 bytes per path |
| Null/absent counters (per path) | ~32 bytes per path |
| Fingerprint accumulator | O(current depth) |

For a document with 500 distinct paths, 100 numeric paths, and 50 high-cardinality paths:

```
Path trie:           ~100 KB
DDSketch:            ~300 KB  (100 paths x 3 KB)
CMS:                 ~2.7 MB  (50 paths x 54 KB)
Space-Saving:        ~2.0 MB  (500 paths x 4 KB)
Type/null counters:  ~40 KB   (500 paths x 80 bytes)
Fingerprint:         ~10 KB
---
Total:               ~5.2 MB
```

This budget holds regardless of whether the document is 100 MB or 100 GB, for JSON and NDJSON files. Measured at 6.3 MB on a 142 MB input; see the status note at the top of this page.

## Accuracy

Below `exact_threshold` distinct values per path (default 10,000), values are tracked by identity and the streaming result is **identical** to the DOM path — not an approximation. There is a test asserting entropy agrees to 1e-12.

Past it, the path switches to sketches and its statistics carry `exact: false`:

- `cardinality` becomes a **lower bound** — k occupied Space-Saving counters means at least k distinct values were seen. It is a weak one: 120,000 distinct values currently report 100, the counter budget. A distinct-count sketch would fix this and is tracked in [#106](https://github.com/copyleftdev/vajra/issues/106).
- `entropy`, `normalized_entropy` and `max_rarity` are approximations with no proven bound, because Space-Saving admits an evicted item at `min_count + 1` rather than cleanly grouping the tail.

`exact` is omitted from the output when true, so the DOM path's output is unchanged and the flag only ever appears to mark a figure that is not a measurement.

---

## DOM vs. Streaming: What Changes

| Capability | DOM Mode | Streaming Mode |
|---|---|---|
| Parsing speed | 2+ GB/s (simd-json) | ~500 MB/s (event parser) |
| Random access | Full | None (sequential events) |
| Exact frequency counts | Yes | Only when cardinality fits in memory; CMS otherwise |
| Exact percentiles | Yes (via sorting) | Approximate (DDSketch, 1% relative error) |
| Exact entropy | Yes | Approximate (from CMS estimates) |
| Motif detection | Full (Merkle subtree hashing) | Partial (incremental, no lookback) |
| Relationship discovery | Full (random access to value pairs) | Partial (co-occurrence counters) |
| Essence quality | Full | Slightly reduced (no selective subtree re-parse in Pass 1) |

Every streaming approximation carries formal error bounds. The output explicitly labels which statistics are exact and which are approximate.

---

## When Each Mode Activates

```
Document size < streaming_threshold (default 100 MB)
  -> DOM mode

Document size >= streaming_threshold
  -> Streaming mode (automatic)

--streaming flag present
  -> Streaming mode (forced, regardless of size)
```

The threshold is configurable in the TOML config:

```toml
[parsing]
streaming_threshold = 104_857_600  # 100 MB
```

---

## The StreamAnalyzer Trait

Any analyzer that implements `StreamAnalyzer` can participate in streaming mode:

```rust
pub trait StreamAnalyzer {
    type Accumulator: Default;
    type Output;

    fn on_event(&self, event: &JsonEvent, acc: &mut Self::Accumulator) -> Result<()>;
    fn finalize(&self, acc: Self::Accumulator) -> Result<Self::Output>;
}
```

The accumulator holds all state. Events arrive one at a time. `finalize` produces the result when the stream ends.

This trait is the key to extensibility. Custom analyzers that implement it automatically work in both DOM and streaming modes — DOM mode simply feeds all events from the pre-parsed tree.

---

## Differential Testing: DOM vs. Streaming

For every document in the test corpus, Vajra runs both modes and asserts:

- CMS frequency estimates are within proven error bounds of exact counts
- DDSketch quantile estimates are within relative accuracy of exact quantiles
- Path sets are identical
- Fingerprints are identical
- Type distributions are identical

This ensures streaming mode is not a second-class citizen. It is a formally bounded approximation of DOM mode, not a degraded fallback.

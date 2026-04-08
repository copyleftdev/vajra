<div class="vajra-hero" id="hero">
  <div class="vajra-weapon" id="weapon">
    <img src="images/vajra-logo.svg" alt="Vajra" class="vajra-logo" id="vajra-logo" />
  </div>
  <div class="vajra-title-block" id="title-block">
    <h1 class="vajra-title" id="main-title">VAJRA</h1>
    <p class="vajra-subtitle" id="subtitle">Deterministic Semantic Reduction Engine</p>
    <div class="vajra-mantra" id="mantra">
      <span class="mantra-break" id="mantra-break">Break noise.</span>
      <span class="mantra-preserve" id="mantra-preserve">Preserve truth.</span>
    </div>
  </div>
</div>

<div class="vajra-stats-bar" id="stats-bar">
  <div class="stat" id="stat-1">
    <span class="stat-number">761</span>
    <span class="stat-label">Tests</span>
  </div>
  <div class="stat" id="stat-2">
    <span class="stat-number">12</span>
    <span class="stat-label">Crates</span>
  </div>
  <div class="stat" id="stat-3">
    <span class="stat-number">11</span>
    <span class="stat-label">Commands</span>
  </div>
  <div class="stat" id="stat-4">
    <span class="stat-number">22K</span>
    <span class="stat-label">Lines of Rust</span>
  </div>
  <div class="stat" id="stat-5">
    <span class="stat-number">0</span>
    <span class="stat-label">Failures</span>
  </div>
</div>

---

<div class="vajra-section" id="section-what">

## What Vajra Does

Feed it any structured data. Get back **shape, signal, anomalies, and truth**.

Vajra analyzes JSON, YAML, CSV, NDJSON, Markdown, and PDF. It extracts structural fingerprints, computes entropy and statistical profiles, detects anomalies and schema drift, discovers cross-field relationships, and renders deterministic essences tuned for humans, auditors, or AI pipelines.

</div>

<div class="vajra-demo-grid" id="demo-grid">

<div class="demo-card" id="card-inspect">

### Inspect

```bash
vajra inspect claim.json
```

Full structural analysis — paths, types, fingerprints, domain recognition.

</div>

<div class="demo-card" id="card-essence">

### Essence

```bash
vajra essence data.json --profile staff
```

Concern-oriented reduction. 7 profiles. Token budgets. Compact-AI output for LLMs.

</div>

<div class="demo-card" id="card-drift">

### Drift

```bash
vajra drift v1.json v2.json
```

Schema drift detection with JSD, Wasserstein distance, severity classification.

</div>

<div class="demo-card" id="card-anomaly">

### Anomalies

```bash
vajra anomalies batch.ndjson
```

MAD-based outliers, rarity scoring, type instability. Deterministic. Explainable.

</div>

<div class="demo-card" id="card-query">

### Query

```bash
vajra query data.json 'entropy($.status) > 0.5'
```

Path expressions with analysis functions. Entropy, rarity, null rate, instability.

</div>

<div class="demo-card" id="card-cluster">

### Cluster

```bash
vajra cluster batch/*.json
```

MinHash + LSH similarity clustering. Finds payload families in seconds.

</div>

</div>

---

<div class="vajra-section" id="section-forged">

## Forged for the Agent Gods

Vajra was not designed for casual use. It was forged as a weapon — an instrument of precision for AI systems that need to understand structured data at scale.

**The compact-ai output** compresses a 1000-node JSON document into a token-efficient essence that preserves every anomaly, every structural motif, every statistical signal — in a format an LLM can parse in a single pass.

**The chain-ready drill section** tells the downstream model exactly which paths have deeper analysis available, enabling multi-turn investigation without re-processing.

**The determinism guarantee** means the same input always produces the same output. No drift. No randomness. No surprises. An AI pipeline that depends on Vajra can depend on Vajra.

```bash
vajra essence massive.json --profile ai --format compact-ai --budget 500
```

```json
{
  "v": "vajra/1",
  "doc": {"nodes": 847, "paths": 23, "depth": 6},
  "anomalies": [
    {"p": "$.claims[*].allowed", "t": "type_instability", "s": 0.4},
    {"p": "$.claims[*].charge", "t": "numeric_outlier", "v": 350, "z": 4.2}
  ],
  "drill": [
    {"path": "$.claims[*].service_lines", "available": ["stats", "anomalies", "motifs"]}
  ],
  "meta": {"profile": "ai", "truncated": false}
}
```

</div>

---

<div class="vajra-section" id="section-engine">

## The Engine

<div class="engine-grid" id="engine-grid">

<div class="engine-card" id="eng-fingerprint">
  <div class="engine-icon">&#x2726;</div>
  <h4>BLAKE3 Fingerprinting</h4>
  <p>Merkle subtree hashing. Path set signatures. Motif detection falls out for free. O(n).</p>
</div>

<div class="engine-card" id="eng-entropy">
  <div class="engine-icon">&#x223F;</div>
  <h4>Shannon Entropy</h4>
  <p>Distinguishes boilerplate from signal without domain knowledge. The strongest universal primitive.</p>
</div>

<div class="engine-card" id="eng-mad">
  <div class="engine-icon">&#x2206;</div>
  <h4>MAD Outliers</h4>
  <p>50% breakdown point. Half the data can be corrupted before MAD gives a misleading result.</p>
</div>

<div class="engine-card" id="eng-jsd">
  <div class="engine-icon">&#x21C4;</div>
  <h4>Jensen-Shannon Divergence</h4>
  <p>Symmetric. Bounded. A proper metric via sqrt. The right way to measure distribution drift.</p>
</div>

<div class="engine-card" id="eng-ddsketch">
  <div class="engine-icon">&#x2261;</div>
  <h4>DDSketch</h4>
  <p>Relative-error quantile estimation. Mergeable. O(1) per insert. Streams terabytes in megabytes of RAM.</p>
</div>

<div class="engine-card" id="eng-lsh">
  <div class="engine-icon">&#x2318;</div>
  <h4>MinHash + LSH</h4>
  <p>Sublinear similarity search. Cluster 10K documents in seconds. No O(n^2) anywhere.</p>
</div>

</div>
</div>

---

<div class="vajra-section" id="section-install">

## Install

```bash
cargo install vajra-cli
```

Or from source:

```bash
git clone https://github.com/zuub-don/vajra
cd vajra
cargo build --release
```

First useful output in under 30 seconds:

```bash
echo '{"hello": "world"}' | vajra inspect -
```

</div>

<div class="vajra-footer" id="footer">
  <div class="footer-mantra">Break noise. Preserve truth.</div>
</div>

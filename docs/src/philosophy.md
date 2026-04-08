# Philosophy

Vajra exists because JSON is lying to you — not about its content, but about its complexity.

A 14,000-line medical claim is not 14,000 lines of information. It is a handful of structural motifs repeated dozens of times, wrapped in representational noise, carrying a few critical signals buried at unpredictable depths. The humans who depend on this data cannot see the signal. The AI systems consuming it waste tokens on the noise. The auditors verifying it have no tools that operate at the right level of abstraction.

Vajra was forged to solve this. Not by transforming the data. Not by summarizing it probabilistically. By *analyzing* it — deterministically, mathematically, and completely — and rendering the result as a compressed, faithful essence tuned to the concern of whoever is reading it.

---

## The Three Views of JSON

This is the foundational insight. Every JSON document is three things simultaneously.

### A Tree

The literal parse tree. Parent-child relationships, nesting depth, sibling structure, array indices. This is what `JSON.parse()` gives you. It is necessary but not sufficient.

The tree tells you *what is here*. It does not tell you what matters.

### A Graph

Repeated structures create implicit references. Co-occurring keys form relationships. A `diagnosis[*].code` that appears alongside a `diagnosis[*].system` and a `diagnosis[*].display` is not three independent strings — it is a coded concept. A `subscriber.id` that functionally determines a `subscriber.name` is a dependency edge, invisible in the tree but real in the data.

The graph tells you *how things relate*. It reveals structure that the tree hides.

### A Distribution

Every key name, every value, every type, every path, every null, every length — all form measurable statistical distributions. Shannon entropy distinguishes boilerplate from signal. Frequency reveals what is common and what is rare. MAD scores expose outliers that standard deviation would mask. The distribution of leading digits (Benford's Law) separates naturally occurring financial data from fabricated numbers.

The distribution tells you *what is normal and what deviates*. It does this without rules, without schemas, without training data.

**Raw JSON exposes only the tree. Vajra reads all three simultaneously.**

---

## The Six Design Principles

These are not aspirations. They are constraints. Every design decision in Vajra was tested against all six. Anything that violated even one was cut.

### 1. Universal

Any JSON. Any size. Any schema. Any nesting depth. No required schema definition, no required domain knowledge, no assumption about structure. If it parses as JSON, Vajra handles it.

This means: the core engine cannot contain a single line of code that assumes the data is a medical claim, or a financial transaction, or an API response. Domain intelligence enters only through plugins and profiles — never through the engine.

### 2. Deterministic

Same input + same config + same version = same output. Always. Fingerprints, scores, orderings, essence text, anomaly rankings — all reproducible to the byte.

This is not a nice-to-have. It is the foundation that makes Vajra trustworthy in pipelines, audits, and CI. An AI system that depends on Vajra can depend on Vajra. A compliance team that runs it twice gets the same answer twice.

The cost of this constraint is real: `HashMap` is banned from all externally-visible orderings (replaced by `BTreeMap`). Floating-point formatting uses `ryu` for platform independence. Every randomized algorithm is seeded. These costs are paid gladly.

### 3. Honest

Every inference is labeled as inference. Every score is decomposable. Every anomaly is explainable. Vajra never silently asserts a heuristic conclusion as truth.

When Vajra infers that a string is a date, it tells you the confidence level: *definite* (100% of values matched the DFA), *dominant* (>80%), *heuristic* (entropy-based), or *unclassified* (no inference applied). When it flags an anomaly, it shows the z-score, the median, the MAD, and the path. When it ranks an observation in an essence, `--explain` decomposes the score into its six contributing dimensions.

Magic is the enemy of trust. Vajra does not do magic.

### 4. Fast

Operational speed. Not batch-overnight speed. Seconds on typical payloads, minutes on gigabyte-scale files. Fast enough to use interactively in a terminal. Fast enough to gate a CI pipeline. Fast enough that reaching for Vajra is faster than opening the file.

The engine achieves this through `simd-json` for 2+ GB/s parsing throughput, O(n) single-pass analysis wherever possible, arena allocation for ephemeral analysis memory, and Rayon-based parallelism for batch operations.

### 5. Composable

The CLI, the Rust library, and the plugin system are each independently useful. Analyzers compose. Outputs chain. Profiles combine with formats and budgets.

`vajra stats` feeds `vajra essence`. `vajra fingerprint` feeds `vajra drift`. `vajra anomalies` can read from stdin in a pipeline. The library API exposes the same analyzers as the CLI, composable in Rust code without the CLI overhead.

### 6. Minimal Assumption

The core engine assumes nothing about the domain, the schema, or the purpose of the data. It analyzes structure, statistics, and deviation from population norms. It does not know what a "claim" is. It does not know what "E11.9" means. It does not know that `allowed_amount` should never be null.

Domain intelligence is real and valuable — but it enters through plugins (`vajra-domain-med`) and concern profiles (`--profile auditor`), never through hardcoded logic in the analysis pipeline.

This separation is what makes Vajra universal. The same engine that analyzes medical claims also analyzes IoT sensor payloads, financial transactions, API responses, and configuration files — because it never assumed it was analyzing any of them.

---

## What Vajra Is NOT

Precision requires boundaries. Vajra is not:

- **A replacement for jq.** jq transforms JSON. Vajra analyzes and reduces it. They are complementary, not competitive. Use jq to reshape; use Vajra to understand.

- **A probabilistic summarizer.** Every reduction Vajra performs is deterministic and explainable. There is no language model in the pipeline. There is no sampling. There is no "approximately."

- **A database or data store.** Vajra is ephemeral. It reads, analyzes, and emits. It does not persist data, cache results, or maintain state between runs.

- **A schema registry.** Vajra infers schema characteristics — it does not define or enforce them. It tells you what shape the data *has*, not what shape it *should* have.

- **A GUI or BI platform.** Vajra is a CLI and a library. It renders text, JSON, Markdown, and compact-AI output. Visualization is left to tools that specialize in it.

- **A data transformation tool.** Vajra never rewrites source data. It reads. It analyzes. It emits results. The input is sacred.

- **A validator or linter.** Vajra does not check against rules you define. It discovers what the data *is* and what deviates from what the data *normally is*. The difference is fundamental.

---

## The Category Vajra Creates

There is no existing category that accurately describes Vajra. The closest neighbors are:

**Structured-data observability.** Like application observability (metrics, traces, logs) but for the data itself. What is the shape of this payload? What changed since yesterday? What is anomalous in this batch?

**Semantic reduction.** Not summarization (which loses information probabilistically) but reduction (which compresses information deterministically, preserving all signal above a configurable threshold).

**Operational cognition tooling.** Tools that make the shape of complex data legible to the humans and AI systems that depend on it.

Vajra sits at the intersection of these three. It is the first tool built specifically to occupy this space.

---

## The Mantra

**Break noise. Preserve truth.**

Every decision in Vajra flows from these four words. Noise is representational redundancy, structural boilerplate, repeated motifs, and cognitive overhead. Truth is anomalies, deviations, relationships, and operational signal.

The essence is what remains when the noise is broken and the truth is preserved.

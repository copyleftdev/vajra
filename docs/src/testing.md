# Testing

Vajra's test suite is not an afterthought. It is a structural guarantee. 761 tests across 7 testing strategies ensure that every algorithm, every command, and every output contract works as specified — and continues to work as the codebase evolves.

---

## The Test Philosophy

1. **Every algorithm has a unit test that verifies it against known inputs with expected outputs.** No algorithm ships without a proof that it computes correctly.

2. **Every property that should hold universally is tested with random inputs.** Canonicalization is idempotent. Fingerprints are key-order-independent. Drift detection is symmetric. These are not checked on one example — they are checked on thousands of generated inputs.

3. **Every failure mode is tested.** Malformed JSON, deeply nested documents, pathologically wide objects, adversarial strings. If Vajra can encounter it in the wild, the fuzzer has already thrown it.

4. **Determinism is tested directly.** Same input, same config, 10 runs, byte-identical output. This runs in CI on every commit.

5. **Streaming and DOM modes are tested against each other.** They must agree within documented error bounds. If they diverge, the streaming approximation is broken.

---

## Test Categories

### Unit Tests

761 tests across all crates. Each primitive, each algorithm, each data structure has targeted tests with known inputs and expected outputs.

**Examples from each crate:**

**vajra-core:**
```rust
#[test]
fn canonicalization_sorts_keys_lexicographically() {
    let input = r#"{"b": 2, "a": 1, "c": 3}"#;
    let doc = Document::parse_str(input).unwrap();
    let canonical = doc.canonical_json();
    assert_eq!(canonical, r#"{"a":1,"b":2,"c":3}"#);
}

#[test]
fn path_extraction_normalizes_array_indices() {
    let input = r#"{"items": [{"id": 1}, {"id": 2}]}"#;
    let doc = Document::parse_str(input).unwrap();
    let paths = doc.trie().all_paths();
    assert!(paths.iter().any(|p| p.to_string() == "$.items[*].id"));
}

#[test]
fn malformed_json_returns_error_not_panic() {
    let input = r#"{"unclosed": "string"#;
    let result = Document::parse_str(input);
    assert!(result.is_err());
}
```

**vajra-stats:**
```rust
#[test]
fn shannon_entropy_of_uniform_distribution() {
    // 4 equally likely values -> entropy = 2.0 bits
    let values = vec!["a", "b", "c", "d"];
    let counts: BTreeMap<&str, u64> = values.iter()
        .map(|v| (*v, 25u64))
        .collect();
    let entropy = shannon_entropy(&counts);
    assert!((entropy - 2.0).abs() < 1e-10);
}

#[test]
fn mad_of_known_distribution() {
    let values = vec![1.0, 2.0, 3.0, 4.0, 5.0, 100.0];
    let median = 3.5;
    let mad = compute_mad(&values);
    // MAD = median(|1-3.5|, |2-3.5|, |3-3.5|, |4-3.5|, |5-3.5|, |100-3.5|)
    //     = median(2.5, 1.5, 0.5, 0.5, 1.5, 96.5) = 1.5
    assert!((mad - 1.5).abs() < 1e-10);
}

#[test]
fn ddsketch_quantiles_within_relative_accuracy() {
    let mut sketch = DDSketch::new(0.01); // 1% relative accuracy
    for v in &known_distribution {
        sketch.insert(*v);
    }
    let estimated_p50 = sketch.quantile(0.5).unwrap();
    let true_p50 = exact_median(&known_distribution);
    assert!((estimated_p50 - true_p50).abs() <= 0.01 * true_p50.abs());
}
```

**vajra-fingerprint:**
```rust
#[test]
fn path_set_fingerprint_is_key_order_independent() {
    let a = Document::parse_str(r#"{"x": 1, "y": 2}"#).unwrap();
    let b = Document::parse_str(r#"{"y": 2, "x": 1}"#).unwrap();
    let fp_a = FingerprintAnalyzer.analyze(&a).unwrap();
    let fp_b = FingerprintAnalyzer.analyze(&b).unwrap();
    assert_eq!(fp_a.path_set, fp_b.path_set);
}

#[test]
fn identical_subtrees_produce_identical_merkle_hashes() {
    let input = r#"{"items": [{"a": 1, "b": 2}, {"a": 3, "b": 4}]}"#;
    let doc = Document::parse_str(input).unwrap();
    let fp = FingerprintAnalyzer.analyze(&doc).unwrap();
    // Both array elements have the same structure -> same subtree hash
    assert_eq!(fp.subtree_hashes[0], fp.subtree_hashes[1]);
}
```

**vajra-anomaly:**
```rust
#[test]
fn mad_outlier_detection_flags_extreme_values() {
    let values: Vec<f64> = (0..100).map(|i| i as f64).collect();
    let mut values_with_outlier = values.clone();
    values_with_outlier.push(10_000.0);
    let report = AnomalyAnalyzer::detect_numeric_outliers(
        &values_with_outlier, 3.5
    );
    assert!(report.outliers.iter().any(|o| o.value == 10_000.0));
}
```

**vajra-drift:**
```rust
#[test]
fn jsd_is_symmetric() {
    let p = distribution_a();
    let q = distribution_b();
    let jsd_pq = jensen_shannon_divergence(&p, &q);
    let jsd_qp = jensen_shannon_divergence(&q, &p);
    assert!((jsd_pq - jsd_qp).abs() < 1e-10);
}

#[test]
fn jsd_is_zero_for_identical_distributions() {
    let p = distribution_a();
    let jsd = jensen_shannon_divergence(&p, &p);
    assert!(jsd.abs() < 1e-10);
}
```

---

### Property Tests

Using `proptest`, Vajra tests invariants that must hold for **all** valid inputs:

**Canonicalization idempotence:**
```rust
proptest! {
    #[test]
    fn canonicalize_is_idempotent(json in arb_json()) {
        let once = canonicalize(&json);
        let twice = canonicalize(&once);
        prop_assert_eq!(once, twice);
    }
}
```

**Fingerprint stability under key reordering:**
```rust
proptest! {
    #[test]
    fn fingerprint_stable_under_key_reorder(obj in arb_json_object()) {
        let original = fingerprint(&obj);
        let shuffled = shuffle_keys(&obj);
        let recomputed = fingerprint(&shuffled);
        prop_assert_eq!(original.path_set, recomputed.path_set);
    }
}
```

**Merkle hash determinism:**
```rust
proptest! {
    #[test]
    fn merkle_hash_deterministic(json in arb_json()) {
        let hash1 = merkle_subtree_hash(&json);
        let hash2 = merkle_subtree_hash(&json);
        prop_assert_eq!(hash1, hash2);
    }
}
```

**Drift symmetry:**
```rust
proptest! {
    #[test]
    fn structural_drift_is_symmetric(a in arb_json(), b in arb_json()) {
        let drift_ab = structural_drift(&a, &b);
        let drift_ba = structural_drift(&b, &a);
        prop_assert_eq!(drift_ab.added_paths, drift_ba.removed_paths);
        prop_assert_eq!(drift_ab.removed_paths, drift_ba.added_paths);
    }
}
```

**MinHash accuracy convergence:**
```rust
proptest! {
    #[test]
    fn minhash_jaccard_converges(
        a in arb_string_set(1..100),
        b in arb_string_set(1..100)
    ) {
        let true_jaccard = exact_jaccard(&a, &b);
        let estimated = minhash_jaccard(&a, &b, 128);
        // With 128 hashes, expected error < 0.1 at 95% confidence
        prop_assert!((true_jaccard - estimated).abs() < 0.15);
    }
}
```

**DDSketch relative error guarantee:**
```rust
proptest! {
    #[test]
    fn ddsketch_quantile_within_bounds(values in arb_f64_vec(10..1000)) {
        let mut sketch = DDSketch::new(0.01);
        for v in &values { sketch.insert(*v); }
        let estimated = sketch.quantile(0.5).unwrap();
        let exact = exact_median(&values);
        prop_assert!((estimated - exact).abs() <= 0.01 * exact.abs() + 1e-10);
    }
}
```

**Scoring determinism:**
```rust
proptest! {
    #[test]
    fn scoring_is_deterministic(json in arb_json(), profile in arb_profile()) {
        let score1 = compute_scores(&json, &profile);
        let score2 = compute_scores(&json, &profile);
        prop_assert_eq!(score1, score2);
    }
}
```

---

### Chaos Tests (Fuzzing)

Using `cargo-fuzz` and `AFL`, the fuzzer throws adversarial inputs at every entry point:

| Input Category | What It Tests |
|---|---|
| Truncated JSON | `{"key": "valu` — parser graceful failure |
| Unbalanced braces | `{{{}}` — parser error recovery |
| Invalid UTF-8 | Raw byte sequences — no undefined behavior |
| Depth 10,000+ nesting | `[[[[[...` — depth limit enforcement |
| 100,000+ keys per object | `{"k1":1,"k2":2,...}` — performance, memory |
| 1M identical array elements | `[1,1,1,...]` — motif detection, sketch behavior |
| Type chaos | Same path alternates string/number/null — instability detection |
| Adversarial strings | Null bytes, RTL markers, control characters, multi-byte Unicode |
| Near-max-size documents | At the streaming threshold boundary — mode switching |

**Target:** Zero panics. Zero undefined behavior. Graceful error on every input.

```bash
# Run the fuzzer
cd vajra-core
cargo fuzz run parse_json -- -max_total_time=3600
```

---

### Differential Tests

Two implementations of the same analysis must agree within documented bounds:

**DOM vs. Streaming:**
```rust
#[test]
fn dom_and_streaming_stats_agree() {
    let doc = Document::parse_file("corpus/claim.json").unwrap();
    let dom_stats = StatsAnalyzer.analyze(&doc).unwrap();

    let mut acc = StreamingStatsAccumulator::default();
    for event in stream_events("corpus/claim.json") {
        acc.on_event(&event.unwrap()).unwrap();
    }
    let stream_stats = acc.finalize().unwrap();

    // Path sets must be identical
    assert_eq!(dom_stats.paths.keys().collect::<Vec<_>>(),
               stream_stats.paths.keys().collect::<Vec<_>>());

    // CMS estimates within error bounds
    for (path, dom_ps) in &dom_stats.paths {
        let stream_ps = &stream_stats.paths[path];
        for (value, &exact_count) in &dom_ps.value_frequencies {
            let estimated = stream_ps.estimated_frequency(value);
            assert!(exact_count <= estimated);
            assert!(estimated <= exact_count + EPSILON * stream_stats.total_values);
        }
    }
}
```

**Exact quantiles vs. DDSketch:**
```rust
#[test]
fn ddsketch_within_relative_accuracy() {
    let values = load_test_values("corpus/charge_amounts.json");
    let mut sketch = DDSketch::new(0.01);
    for v in &values { sketch.insert(*v); }

    for q in &[0.01, 0.05, 0.25, 0.5, 0.75, 0.95, 0.99] {
        let exact = exact_quantile(&values, *q);
        let estimated = sketch.quantile(*q).unwrap();
        let relative_error = (estimated - exact).abs() / exact.abs();
        assert!(relative_error <= 0.01,
            "q={}: exact={}, estimated={}, error={}",
            q, exact, estimated, relative_error);
    }
}
```

---

### Determinism Tests

```rust
#[test]
fn ten_run_determinism() {
    let corpus = load_corpus("corpus/");
    for file in &corpus {
        let mut outputs = Vec::new();
        for _ in 0..10 {
            let output = run_vajra(&["essence", file, "--profile", "engineer", "--format", "json"]);
            outputs.push(output);
        }
        for i in 1..outputs.len() {
            assert_eq!(outputs[0], outputs[i],
                "Determinism violation on run {} for file {}", i, file);
        }
    }
}

#[test]
fn different_seeds_may_differ() {
    let output_seed0 = run_vajra(&["cluster", "corpus/", "--seed", "0", "--format", "json"]);
    let output_seed42 = run_vajra(&["cluster", "corpus/", "--seed", "42", "--format", "json"]);
    // May differ — that is fine. But within each seed, must be deterministic.
}

#[test]
fn same_seed_is_deterministic() {
    for seed in &["0", "42", "12345"] {
        let mut outputs = Vec::new();
        for _ in 0..10 {
            let output = run_vajra(&["cluster", "corpus/", "--seed", seed, "--format", "json"]);
            outputs.push(output);
        }
        for i in 1..outputs.len() {
            assert_eq!(outputs[0], outputs[i]);
        }
    }
}
```

---

### Golden Tests

For each profile-format combination, golden output files are committed to the repository:

```text
tests/golden/
├── claim_staff_text.golden
├── claim_staff_json.golden
├── claim_engineer_text.golden
├── claim_engineer_json.golden
├── claim_auditor_markdown.golden
├── claim_ai_compact.golden
├── claim_fraud_text.golden
├── drift_engineer_text.golden
├── anomalies_text.golden
└── ...
```

CI asserts byte-exact match between current output and golden files:

```rust
#[test]
fn golden_staff_text() {
    let output = run_vajra(&["essence", "corpus/claim.json", "--profile", "staff"]);
    let golden = std::fs::read_to_string("tests/golden/claim_staff_text.golden").unwrap();
    assert_eq!(output, golden, "Golden test failed: staff/text");
}
```

Golden files are updated explicitly — never auto-updated. When output changes intentionally (algorithm improvement, rendering change), the developer updates the golden files and the diff is reviewed in the PR.

This catches: rendering regressions, ordering instabilities, score drift from algorithm changes.

---

### Benchmark Tests

Using `criterion`, tracking performance across commits:

```rust
fn bench_parse_1mb(c: &mut Criterion) {
    let input = std::fs::read("benches/fixtures/1mb.json").unwrap();
    c.bench_function("parse_1mb", |b| {
        b.iter(|| Document::parse_bytes(black_box(&input)))
    });
}

fn bench_stats_1mb(c: &mut Criterion) {
    let doc = Document::parse_file("benches/fixtures/1mb.json").unwrap();
    c.bench_function("stats_1mb", |b| {
        b.iter(|| StatsAnalyzer.analyze(black_box(&doc)))
    });
}

fn bench_fingerprint_comparison(c: &mut Criterion) {
    let fp_a = /* precomputed */;
    let fp_b = /* precomputed */;
    c.bench_function("fingerprint_compare", |b| {
        b.iter(|| minhash_jaccard(black_box(&fp_a), black_box(&fp_b)))
    });
}
```

**Performance targets validated in CI:**

| Scenario | Target | Test |
|---|---|---|
| 1 MB JSON, full analysis | < 100 ms | `bench_full_1mb` |
| 100 MB JSON, full analysis | < 5 s | `bench_full_100mb` |
| 10,000 document batch | < 30 s | `bench_batch_10k` |
| Fingerprint comparison | < 1 us per pair | `bench_fingerprint_compare` |

Regressions > 10% fail the build.

---

## Running Everything

```bash
# All unit and integration tests
cargo test --workspace

# Property tests (may run longer)
cargo test --workspace -- --include-ignored proptest

# Benchmarks
cargo bench --workspace

# Fuzzing (runs until stopped)
cd vajra-core && cargo fuzz run parse_json

# Determinism check (manual)
for i in $(seq 1 10); do
  vajra essence test/claim.json --format json > "/tmp/run_$i.json"
done
md5sum /tmp/run_*.json
# All hashes must be identical
```

---

## The Invariant Catalog

These properties are tested across the suite. If any is violated, the build fails.

| Invariant | Test Type |
|---|---|
| Canonicalization is idempotent | Property test |
| Fingerprints are key-order-independent | Property test |
| Identical subtrees produce identical Merkle hashes | Property test |
| Structural drift is symmetric (with direction inversion) | Property test |
| MinHash Jaccard converges to true Jaccard | Property test |
| DDSketch quantiles within relative accuracy | Property + differential |
| CMS estimates within proven error bounds | Differential test |
| DOM and streaming produce consistent results | Differential test |
| 10 runs produce byte-identical output | Determinism test |
| No panics on any input | Fuzz test |
| No undefined behavior | Fuzz test |
| Golden output is byte-stable | Golden test |
| Performance within 10% of baseline | Benchmark |
| Mutation score > 85% | Mutation test |

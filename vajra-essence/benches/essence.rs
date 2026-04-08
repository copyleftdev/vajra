use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use vajra_anomaly::AnomalyAnalyzer;
use vajra_essence::{EngineerProfile, EssenceBuilder};
use vajra_fingerprint::FingerprintAnalyzer;
use vajra_stats::StatsAnalyzer;
use vajra_types::traits::Analyzer;

/// Generate a realistic JSON string with approximately `node_count` nodes.
fn generate_json(node_count: usize) -> String {
    let mut buf = String::from("{");
    let mut remaining = node_count.saturating_sub(1);
    let mut key_idx = 0;

    while remaining > 0 {
        if key_idx > 0 {
            buf.push(',');
        }
        match key_idx % 5 {
            0 => {
                let children = remaining.min(5);
                buf.push_str(&format!("\"obj_{}\":{{", key_idx));
                for i in 0..children.saturating_sub(1) {
                    if i > 0 {
                        buf.push(',');
                    }
                    match i % 4 {
                        0 => buf.push_str(&format!("\"s{}\":\"value_{}\"", i, i)),
                        1 => buf.push_str(&format!("\"n{}\":{}", i, i * 42)),
                        2 => buf.push_str(&format!("\"b{}\":true", i)),
                        _ => buf.push_str(&format!("\"x{}\":null", i)),
                    }
                }
                buf.push('}');
                remaining = remaining.saturating_sub(children);
            }
            1 => {
                let elems = remaining.min(8);
                buf.push_str(&format!("\"arr_{}\":[", key_idx));
                for i in 0..elems.saturating_sub(1) {
                    if i > 0 {
                        buf.push(',');
                    }
                    buf.push_str(&format!("{}", i * 7 + 3));
                }
                buf.push(']');
                remaining = remaining.saturating_sub(elems);
            }
            2 => {
                buf.push_str(&format!("\"str_{}\":\"hello_world_{}\"", key_idx, key_idx));
                remaining = remaining.saturating_sub(1);
            }
            3 => {
                buf.push_str(&format!("\"num_{}\":3.14159{}", key_idx, key_idx));
                remaining = remaining.saturating_sub(1);
            }
            _ => {
                let records = remaining.min(12) / 3;
                let records = records.max(1);
                buf.push_str(&format!("\"deep_{}\":[", key_idx));
                let mut used = 1;
                for r in 0..records {
                    if r > 0 {
                        buf.push(',');
                    }
                    buf.push_str(&format!("{{\"id\":{},\"val\":\"rec_{}\"}}", r, r));
                    used += 3;
                }
                buf.push(']');
                remaining = remaining.saturating_sub(used);
            }
        }
        key_idx += 1;
    }
    buf.push('}');
    buf
}

// Benchmark setup must abort on failure — there's no way to propagate
// errors from a Criterion benchmark function, so `expect` is appropriate here.
#[allow(clippy::expect_used)]
fn bench_essence(c: &mut Criterion) {
    let sizes = [
        ("10_nodes", 10),
        ("100_nodes", 100),
        ("1000_nodes", 1000),
        ("10000_nodes", 10_000),
    ];

    let profile = EngineerProfile;

    let prepared: Vec<(
        &str,
        vajra_types::Document,
        vajra_stats::StatsResult,
        vajra_anomaly::AnomalyReport,
        vajra_fingerprint::FingerprintResult,
    )> = sizes
        .iter()
        .map(|(name, size)| {
            let json = generate_json(*size);
            let doc = vajra_core::parse_str(&json).expect("bench setup: parse failed");
            let stats = StatsAnalyzer
                .analyze(&doc)
                .expect("bench setup: stats failed");
            let anomalies = AnomalyAnalyzer::default()
                .analyze(&doc)
                .expect("bench setup: anomaly failed");
            let fingerprint = FingerprintAnalyzer
                .analyze(&doc)
                .expect("bench setup: fingerprint failed");
            (*name, doc, stats, anomalies, fingerprint)
        })
        .collect();

    let mut group = c.benchmark_group("essence");
    for (name, doc, stats, anomalies, fingerprint) in &prepared {
        group.bench_with_input(
            BenchmarkId::new("full_essence", *name),
            &(doc, stats, anomalies, fingerprint),
            |b, (d, s, a, fp)| {
                b.iter(|| {
                    EssenceBuilder::new(black_box(d), &profile)
                        .with_stats(black_box(s))
                        .with_anomalies(black_box(a))
                        .with_fingerprint(black_box(fp))
                        .build()
                });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_essence);
criterion_main!(benches);

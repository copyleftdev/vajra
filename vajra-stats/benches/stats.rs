use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
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

fn bench_stats_analysis(c: &mut Criterion) {
    let sizes = [
        ("10_nodes", 10),
        ("100_nodes", 100),
        ("1000_nodes", 1000),
        ("10000_nodes", 10_000),
    ];

    let docs: Vec<(&str, vajra_types::Document)> = sizes
        .iter()
        .map(|(name, size)| {
            let json = generate_json(*size);
            let doc = vajra_core::parse_str(&json)
                .unwrap_or_else(|e| panic!("parse failed: {e}"));
            (*name, doc)
        })
        .collect();

    let mut group = c.benchmark_group("stats");
    for (name, doc) in &docs {
        group.bench_with_input(
            BenchmarkId::new("stats_analyze", *name),
            doc,
            |b, d| {
                b.iter(|| StatsAnalyzer.analyze(black_box(d)));
            },
        );
    }
    group.finish();
}

fn bench_entropy(c: &mut Criterion) {
    // Benchmark entropy on various distribution sizes
    let mut group = c.benchmark_group("entropy_primitive");
    for n in [10, 100, 1000, 10_000] {
        let counts: Vec<u64> = (1..=n).map(|i| i as u64).collect();
        group.bench_with_input(
            BenchmarkId::new("shannon_entropy", format!("{n}_values")),
            &counts,
            |b, c| {
                b.iter(|| vajra_stats::shannon_entropy_from_counts(black_box(c)));
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_stats_analysis, bench_entropy);
criterion_main!(benches);

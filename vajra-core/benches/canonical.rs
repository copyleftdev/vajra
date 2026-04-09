use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

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
fn bench_canonicalize(c: &mut Criterion) {
    let small = generate_json(10);
    let medium = generate_json(100);
    let large = generate_json(1000);
    let xlarge = generate_json(10_000);

    // Pre-parse into serde_json::Value for canonicalization benchmarks
    let inputs: Vec<(&str, serde_json::Value)> = [
        ("10_nodes", &small),
        ("100_nodes", &medium),
        ("1000_nodes", &large),
        ("10000_nodes", &xlarge),
    ]
    .into_iter()
    .map(|(name, json)| {
        let val: serde_json::Value = serde_json::from_str(json).expect("bench setup: parse failed");
        (name, val)
    })
    .collect();

    let mut group = c.benchmark_group("canonicalize");
    for (name, val) in &inputs {
        group.bench_with_input(BenchmarkId::new("canonicalize", *name), val, |b, v| {
            b.iter(|| vajra_core::canonicalize(black_box(v)));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_canonicalize);
criterion_main!(benches);

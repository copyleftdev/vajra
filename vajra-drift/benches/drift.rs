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

/// Generate a second, slightly different JSON for drift comparison.
fn generate_json_variant(node_count: usize) -> String {
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
                // Same structure but different key prefix to create drift
                buf.push_str(&format!("\"obj_{}\":{{", key_idx));
                for i in 0..children.saturating_sub(1) {
                    if i > 0 {
                        buf.push(',');
                    }
                    match i % 4 {
                        0 => buf.push_str(&format!("\"s{}\":\"different_{}\"", i, i)),
                        1 => buf.push_str(&format!("\"n{}\":{}", i, i * 99)),
                        2 => buf.push_str(&format!("\"b{}\":false", i)),
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
                    buf.push_str(&format!("{}", i * 13 + 7));
                }
                buf.push(']');
                remaining = remaining.saturating_sub(elems);
            }
            2 => {
                buf.push_str(&format!("\"str_{}\":\"other_world_{}\"", key_idx, key_idx));
                remaining = remaining.saturating_sub(1);
            }
            3 => {
                buf.push_str(&format!("\"num_{}\":2.71828{}", key_idx, key_idx));
                remaining = remaining.saturating_sub(1);
            }
            _ => {
                let records = remaining.min(12) / 3;
                let records = records.max(1);
                // Add an extra field to create structural drift
                buf.push_str(&format!("\"deep_{}\":[", key_idx));
                let mut used = 1;
                for r in 0..records {
                    if r > 0 {
                        buf.push(',');
                    }
                    buf.push_str(&format!(
                        "{{\"id\":{},\"val\":\"rec_{}\",\"extra\":true}}",
                        r, r
                    ));
                    used += 4; // object + 3 scalars
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

fn bench_drift(c: &mut Criterion) {
    let sizes = [
        ("10_nodes", 10),
        ("100_nodes", 100),
        ("1000_nodes", 1000),
        ("10000_nodes", 10_000),
    ];

    let doc_pairs: Vec<(&str, vajra_types::Document, vajra_types::Document)> = sizes
        .iter()
        .map(|(name, size)| {
            let json_a = generate_json(*size);
            let json_b = generate_json_variant(*size);
            let doc_a =
                vajra_core::parse_str(&json_a).unwrap_or_else(|e| panic!("parse failed: {e}"));
            let doc_b =
                vajra_core::parse_str(&json_b).unwrap_or_else(|e| panic!("parse failed: {e}"));
            (*name, doc_a, doc_b)
        })
        .collect();

    let mut group = c.benchmark_group("drift");
    for (name, doc_a, doc_b) in &doc_pairs {
        group.bench_with_input(
            BenchmarkId::new("full_drift", *name),
            &(doc_a, doc_b),
            |b, (a, b_doc)| {
                b.iter(|| vajra_drift::full_drift(black_box(a), black_box(b_doc)));
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_drift);
criterion_main!(benches);

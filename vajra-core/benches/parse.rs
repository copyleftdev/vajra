use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

/// Generate a realistic JSON string with approximately `node_count` nodes.
///
/// Produces a mix of objects, arrays, strings, numbers, booleans, and nulls
/// with 3-4 levels of nesting.
fn generate_json(node_count: usize) -> String {
    let mut buf = String::from("{");
    let mut remaining = node_count.saturating_sub(1); // root object is 1 node
    let mut key_idx = 0;

    while remaining > 0 {
        if key_idx > 0 {
            buf.push(',');
        }

        match key_idx % 5 {
            // Nested object with a few scalar fields
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
            // Array of numbers
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
            // String value
            2 => {
                buf.push_str(&format!("\"str_{}\":\"hello_world_{}\"", key_idx, key_idx));
                remaining = remaining.saturating_sub(1);
            }
            // Number value
            3 => {
                buf.push_str(&format!("\"num_{}\":3.14159{}", key_idx, key_idx));
                remaining = remaining.saturating_sub(1);
            }
            // Nested array of objects (deeper nesting)
            _ => {
                let records = remaining.min(12) / 3;
                let records = records.max(1);
                buf.push_str(&format!("\"deep_{}\":[", key_idx));
                let mut used = 1; // the array node
                for r in 0..records {
                    if r > 0 {
                        buf.push(',');
                    }
                    buf.push_str(&format!("{{\"id\":{},\"val\":\"rec_{}\"}}", r, r));
                    used += 3; // object + 2 scalars
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

fn bench_parse(c: &mut Criterion) {
    let small = generate_json(10);
    let medium = generate_json(100);
    let large = generate_json(1000);
    let xlarge = generate_json(10_000);

    let mut group = c.benchmark_group("parse");
    for (name, json) in [
        ("10_nodes", &small),
        ("100_nodes", &medium),
        ("1000_nodes", &large),
        ("10000_nodes", &xlarge),
    ] {
        group.bench_with_input(BenchmarkId::new("parse_str", name), json, |b, j| {
            b.iter(|| vajra_core::parse_str(black_box(j)));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_parse);
criterion_main!(benches);

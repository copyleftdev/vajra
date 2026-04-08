use criterion::{black_box, criterion_group, criterion_main, Criterion};
use vajra_stats::{CountMinSketch, DDSketch, SpaceSaving};

fn bench_ddsketch_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("ddsketch");

    group.bench_function("insert_1M", |b| {
        b.iter(|| {
            let mut sketch = DDSketch::new(0.01);
            for i in 0..1_000_000_u64 {
                sketch.add(black_box(i as f64));
            }
            sketch
        });
    });

    // Benchmark quantile query after 1M inserts
    group.bench_function("quantile_after_1M", |b| {
        let mut sketch = DDSketch::new(0.01);
        for i in 0..1_000_000_u64 {
            sketch.add(i as f64);
        }
        b.iter(|| {
            let _ = black_box(sketch.quantile(0.5));
            let _ = black_box(sketch.quantile(0.95));
            let _ = black_box(sketch.quantile(0.99));
        });
    });

    group.finish();
}

fn bench_cms_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("cms");

    group.bench_function("insert_1M", |b| {
        b.iter(|| {
            let mut cms = CountMinSketch::new(0.001, 0.01);
            for i in 0..1_000_000_u64 {
                let key = format!("item_{}", i % 10_000);
                cms.add(black_box(key.as_bytes()), 1);
            }
            cms
        });
    });

    // Benchmark estimate after 1M inserts
    group.bench_function("estimate_after_1M", |b| {
        let mut cms = CountMinSketch::new(0.001, 0.01);
        for i in 0..1_000_000_u64 {
            let key = format!("item_{}", i % 10_000);
            cms.add(key.as_bytes(), 1);
        }
        let queries: Vec<String> = (0..100).map(|i| format!("item_{}", i)).collect();
        b.iter(|| {
            for q in &queries {
                let _ = black_box(cms.estimate(q.as_bytes()));
            }
        });
    });

    group.finish();
}

fn bench_space_saving_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("space_saving");

    group.bench_function("insert_1M", |b| {
        b.iter(|| {
            let mut ss = SpaceSaving::new(100);
            for i in 0..1_000_000_u64 {
                let key = format!("item_{}", i % 10_000);
                ss.add(black_box(&key));
            }
            ss
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_ddsketch_insert,
    bench_cms_insert,
    bench_space_saving_insert
);
criterion_main!(benches);

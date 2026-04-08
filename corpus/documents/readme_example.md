# Vajra

**Break noise. Preserve truth.**

A high-performance Rust CLI that analyzes arbitrary JSON, extracts structural
and semantic signal, detects anomalies and drift, and emits compact
deterministic essences.

## Installation

```bash
cargo install vajra-cli
```

Or build from source:

```bash
git clone https://github.com/example/vajra.git
cd vajra
cargo build --release
```

## Quick Start

```bash
# Inspect a JSON file
vajra inspect data.json

# Statistical summary
vajra stats data.json

# Detect anomalies
vajra anomalies data.json

# Compare two versions
vajra drift v1.json v2.json
```

## Features

- **Zero-config analysis**: Auto-detects structure and schema
- **Multiple output formats**: Text, JSON, Markdown, Compact AI
- **Anomaly detection**: Numeric outliers, rare values, type instability
- **Schema drift**: Structural and distributional comparison
- **Domain plugins**: Medical coding (ICD-10, CPT, NPI, NDC)

## License

MIT OR Apache-2.0

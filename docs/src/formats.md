# Input Formats

Vajra reads more than JSON. It reads anything that can be interpreted as structured data — and it auto-detects the format so you do not have to tell it.

---

## Supported Formats

| Format | Extensions | Detection | Notes |
|---|---|---|---|
| JSON | `.json` | Content starts with `{` or `[` | Primary format. Full DOM and streaming support. |
| NDJSON | `.ndjson`, `.jsonl` | Multiple JSON objects separated by newlines | Each line is a separate document. Batch analysis native. |
| YAML | `.yaml`, `.yml` | Content starts with `---` or key-colon pattern | Multi-document YAML supported (separated by `---`). |
| CSV | `.csv` | Comma-separated with consistent column count | First row treated as headers. Each row becomes a JSON object. |
| TSV | `.tsv` | Tab-separated with consistent column count | Same as CSV but tab-delimited. |
| Markdown | `.md` | Markdown structure with tables or code blocks | Tables extracted as arrays of objects. Code blocks parsed if JSON/YAML. |
| PDF | `.pdf` | PDF magic bytes | Text extracted and parsed for structured content. |
| Gzip | `.gz`, `.json.gz` | Gzip magic bytes (`1f 8b`) | Decompressed transparently. Inner format auto-detected. |
| Zstd | `.zst`, `.json.zst` | Zstd magic bytes | Decompressed transparently. Inner format auto-detected. |
| HTTP URL | `http://`, `https://` | URL scheme prefix | Fetched via blocking HTTP GET. Response body auto-detected. |
| Source Code | `.rs`, `.py`, `.js`, `.ts`, `.go`, `.java`, `.c`, `.cpp`, `.rb` | File extension matches known language | Parsed via tree-sitter into AST. Requires `vajra-source` feature. |
| Git Repository | (directory) | Directory contains `.git/` | Reads commit history directly. See flags below. |
| V8 CPU Profile | `.cpuprofile` | File extension | Parses V8 `.cpuprofile` JSON into analyzable structure. |
| strace Summary | — | Content contains `% time` header | Parses `strace -c` summary output into structured records. |
| Stdin | `-` | Explicit `-` argument | Content auto-detected from first bytes. |

---

## Auto-Detection Logic

When no `--input-format` is specified, Vajra detects the format in this order:

1. **Check the argument.** If it is `-`, read from stdin. If it starts with `http://` or `https://`, fetch via HTTP. If it is a directory containing `.git/`, treat as a git repository.

2. **Check the extension.** `.json` -> JSON. `.ndjson`/`.jsonl` -> NDJSON. `.yaml`/`.yml` -> YAML. `.csv` -> CSV. `.tsv` -> TSV. `.md` -> Markdown. `.pdf` -> PDF. `.cpuprofile` -> V8 CPU Profile. `.rs`/`.py`/`.js`/`.go`/etc. -> Source Code (via tree-sitter).

3. **Check for compression.** If the extension is `.gz` or `.zst`, decompress and re-detect the inner format from the next extension (e.g., `.json.gz` -> decompress -> JSON).

4. **Check content.** If the extension is ambiguous or missing, read the first bytes:
   - Starts with `{` or `[` after whitespace -> JSON
   - Multiple `{...}\n` sequences -> NDJSON
   - Starts with `---` or matches `key: value` pattern -> YAML
   - Consistent comma-separated columns -> CSV
   - PDF magic bytes (`%PDF`) -> PDF
   - Contains `% time` column header -> strace summary

5. **Fall back to JSON.** If nothing else matches, attempt JSON parsing.

---

## Format Override

Force a specific format with `--input-format`:

```bash
vajra inspect data.txt --input-format json
vajra stats records.log --input-format ndjson
vajra inspect data.bin --input-format yaml
```

This overrides all auto-detection. Useful when files have nonstandard extensions.

---

## Format Details

### JSON

The primary format. Parsed by `simd-json` in DOM mode (full random access, rich analysis) or streaming mode (bounded memory, SAX-style events).

```bash
vajra inspect claim.json
```

```bash
echo '{"patient": "Martinez", "status": "active"}' | vajra inspect -
```

### NDJSON (Newline-Delimited JSON)

Each line is an independent JSON document. Natural format for logs, event streams, and batch data.

```bash
vajra anomalies claims.ndjson
```

NDJSON records are aggregated into a single array for analysis. Commands like `stats`, `anomalies`, `invariants`, and `essence` compute across all records as a unified population.

Example input:

```
{"claim_id": "C001", "status": "adjudicated", "amount": 285.00}
{"claim_id": "C002", "status": "denied", "amount": 0.00}
{"claim_id": "C003", "status": "adjudicated", "amount": 47250.00}
```

### YAML

Single-document and multi-document YAML both supported. Parsed via `serde_yaml` and converted to Vajra's internal document model.

```bash
vajra inspect config.yaml
```

Multi-document YAML (separated by `---`):

```yaml
---
claim_id: C001
status: adjudicated
amount: 285.00
---
claim_id: C002
status: denied
amount: 0.00
```

```bash
vajra anomalies multi_claims.yaml
```

### CSV

The first row is treated as column headers. Each subsequent row becomes a JSON object with header names as keys.

```bash
vajra stats claims.csv
```

Example input:

```csv
claim_id,status,charge_amount,allowed_amount
C001,adjudicated,285.00,210.00
C002,denied,125.00,
C003,adjudicated,890.00,675.00
```

Vajra converts this to:

```json
[
  {"claim_id": "C001", "status": "adjudicated", "charge_amount": "285.00", "allowed_amount": "210.00"},
  {"claim_id": "C002", "status": "denied", "charge_amount": "125.00", "allowed_amount": ""},
  {"claim_id": "C003", "status": "adjudicated", "charge_amount": "890.00", "allowed_amount": "675.00"}
]
```

Empty cells are preserved as empty strings, allowing missingness analysis to detect them.

### TSV

Identical to CSV but tab-delimited. Same header-to-object conversion.

```bash
vajra stats data.tsv
vajra stats data.txt --input-format tsv
```

### Markdown

Vajra extracts structured content from Markdown files:

- **Tables** are parsed into arrays of objects (headers become keys)
- **JSON/YAML code blocks** are parsed as embedded documents

```bash
vajra inspect report.md
```

### PDF

Text is extracted from PDF files and parsed for any structured content (embedded tables, JSON fragments, structured text patterns).

```bash
vajra inspect document.pdf
```

PDF support depends on the `pdf-extract` crate. Complex layouts may lose structure during extraction.

### Source Code

Vajra can analyze source code from any language supported by tree-sitter. The source file is parsed into a concrete syntax tree (CST), converted to a JSON structure, and analyzed through the full Vajra pipeline — entropy, anomalies, fingerprinting, drift, motifs, and essence all work on code.

```bash
vajra inspect main.rs                       # auto-detect Rust
vajra stats app.py                          # auto-detect Python
vajra drift v1/server.go v2/server.go       # code structural drift
vajra essence lib.rs --profile engineer     # code essence
vajra inspect main.rs --lang rust           # explicit language
vajra inspect code.txt --input-format source --lang python  # override format + language
```

**Supported languages** (each enabled by a feature flag, all on by default):

| Language | Extensions | Feature Flag |
|---|---|---|
| Rust | `.rs` | `rust` |
| Python | `.py`, `.pyi` | `python` |
| JavaScript | `.js`, `.mjs`, `.cjs`, `.jsx` | `javascript` |
| TypeScript | `.ts`, `.tsx`, `.mts`, `.cts` | `typescript` |
| Go | `.go` | `go` |
| Java | `.java` | `java` |
| C | `.c`, `.h` | `c` |
| C++ | `.cpp`, `.cc`, `.cxx`, `.hpp` | `cpp` |
| Ruby | `.rb` | `ruby` |

**What Vajra reveals on code:**

| Analysis | What It Finds |
|---|---|
| Entropy of AST node types | Structural diversity — boilerplate vs complex code |
| Rarity of node types | Unusual constructs — `goto`, `unsafe`, `eval` |
| Nesting depth anomalies | Complexity hotspots |
| Fingerprint comparison | Structural clones across files |
| Drift between versions | Added functions, removed classes, changed signatures |
| Motifs | Repeated structural patterns — copy-paste code |

Source code analysis requires the `vajra-source` crate (included by default). The companion `vajra-domain-source` plugin adds recognizers for naming conventions (snake_case, camelCase, PascalCase) and code structure relationships.

#### Semantic Paths

The `--semantic-paths` flag maps tree-sitter node kinds to human-readable labels in the output. Instead of raw AST node names like `function_item` or `impl_item`, you see `function` and `implementation`.

```bash
vajra inspect main.rs --semantic-paths
```

Without `--semantic-paths`:

```text
$.program.function_item[0].identifier         "process_record"
$.program.function_item[0].parameters.parameter[0]   "record: &Record"
$.program.impl_item[0].identifier             "Pipeline"
```

With `--semantic-paths`:

```text
$.program.function[0].name                    "process_record"
$.program.function[0].parameters.param[0]     "record: &Record"
$.program.implementation[0].name              "Pipeline"
```

Covers 9 languages: Rust, Python, JavaScript, TypeScript, Go, Java, C, C++, and Ruby.

### Git Repository

When the input is a directory containing a `.git/` subdirectory, Vajra reads the commit history directly — no export step required.

```bash
vajra stats ./my-repo
vajra governance ./my-repo
vajra score ./my-repo
```

Each commit becomes a JSON record with exactly these fields:

| Field | Description |
|---|---|
| `hash` | Abbreviated commit hash |
| `author_name` | Author name as recorded by git |
| `author_email` | Author email as recorded by git |
| `date` | Commit date, ISO 8601 |
| `subject` | First line of the commit message |

`governance`, `score` and `compare` resolve `--author-field` and `--message-field`
against the fields the records actually carry, so git input needs no flags —
they select `$.author_name` and `$.subject` automatically and say so on stderr.
Pass the flag explicitly to override; an explicit selector is never second-guessed.

**Flags:**

| Flag | Description | Default |
|---|---|---|
| `--git-limit <N>` | Maximum number of commits to read | 500 |
| `--git-branch <branch>` | Branch to read from | current HEAD |

```bash
vajra stats ./my-repo --git-limit 1000 --git-branch main
```

Auto-detection is based on the presence of `.git/` in the input directory. To override, use `--input-format git`.

### V8 CPU Profile

Vajra parses `.cpuprofile` files produced by V8-based tools (Chrome DevTools, Node.js `--prof`). The profile's call tree is converted to a flat array of records with function name, source location, hit count, and self/total time.

```bash
vajra stats profile.cpuprofile
vajra anomalies profile.cpuprofile
```

Auto-detected by the `.cpuprofile` extension.

### strace Summary

Vajra parses the summary table produced by `strace -c`. Each syscall row becomes a record with fields for time percentage, seconds, calls, errors, and syscall name.

```bash
strace -c ls 2>&1 | vajra stats -
vajra stats strace_output.txt --input-format strace
```

Auto-detected when content contains the `% time` column header characteristic of `strace -c` output.

---

### Compressed Files (Gzip, Zstd)

Compression is transparent. Vajra decompresses on the fly and auto-detects the inner format.

```bash
vajra inspect claims.json.gz
vajra stats archive.json.zst
```

This works with any inner format — `claims.ndjson.gz`, `data.yaml.zst`, `report.csv.gz`.

### HTTP URLs

Vajra fetches the URL via blocking HTTP GET and analyzes the response body.

```bash
vajra inspect https://api.example.com/v1/claims/12345
vajra stats https://data.example.com/feed.ndjson
```

The response content type and body are used for format detection. No authentication headers are supported in the current version — for authenticated endpoints, fetch with `curl` and pipe to stdin:

```bash
curl -H "Authorization: Bearer $TOKEN" https://api.example.com/data | vajra inspect -
```

### Stdin

The `-` argument reads from standard input. Format is auto-detected from the content.

```bash
cat claim.json | vajra inspect -
curl https://api.example.com/data | vajra stats -
jq '.claims[]' data.json | vajra anomalies -
zcat claims.json.gz | vajra inspect -
```

---

## Multi-Document Formats

NDJSON and multi-document YAML naturally contain multiple documents. NDJSON records are now aggregated into a single array, so all commands — including `stats`, `anomalies`, `invariants`, and `essence` — compute across all records as a unified population.

```bash
vajra anomalies claims.ndjson          # analyzes all lines as a batch
vajra stats claims.ndjson              # computes stats across all records
```

---

## Directory Input

When the input is a directory path, Vajra discovers all supported files:

```bash
vajra batch ./claims/                  # processes all files in the directory
vajra cluster ./claims/                # clusters all files in the directory
```

Subdirectories are not traversed recursively by default.

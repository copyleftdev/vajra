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
| Stdin | `-` | Explicit `-` argument | Content auto-detected from first bytes. |

---

## Auto-Detection Logic

When no `--input-format` is specified, Vajra detects the format in this order:

1. **Check the argument.** If it is `-`, read from stdin. If it starts with `http://` or `https://`, fetch via HTTP.

2. **Check the extension.** `.json` -> JSON. `.ndjson`/`.jsonl` -> NDJSON. `.yaml`/`.yml` -> YAML. `.csv` -> CSV. `.tsv` -> TSV. `.md` -> Markdown. `.pdf` -> PDF.

3. **Check for compression.** If the extension is `.gz` or `.zst`, decompress and re-detect the inner format from the next extension (e.g., `.json.gz` -> decompress -> JSON).

4. **Check content.** If the extension is ambiguous or missing, read the first bytes:
   - Starts with `{` or `[` after whitespace -> JSON
   - Multiple `{...}\n` sequences -> NDJSON
   - Starts with `---` or matches `key: value` pattern -> YAML
   - Consistent comma-separated columns -> CSV
   - PDF magic bytes (`%PDF`) -> PDF

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

Every line becomes a separate document in the analysis. Commands like `anomalies` and `invariants` treat the lines as a population.

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

NDJSON and multi-document YAML naturally contain multiple documents. When fed to single-document commands (`inspect`, `stats`, `fingerprint`), Vajra analyzes the first document. When fed to multi-document commands (`anomalies`, `invariants`, `batch`), all documents are analyzed as a population.

To explicitly analyze all documents from a multi-document format:

```bash
vajra anomalies claims.ndjson          # analyzes all lines as a batch
vajra stats claims.ndjson              # analyzes the first line only
```

---

## Directory Input

When the input is a directory path, Vajra discovers all supported files:

```bash
vajra batch ./claims/                  # processes all files in the directory
vajra cluster ./claims/                # clusters all files in the directory
```

Subdirectories are not traversed recursively by default.

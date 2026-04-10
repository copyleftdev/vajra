# Domain Plugins

Core Vajra is domain-agnostic. It analyzes structure, statistics, and deviation from norms — without knowing what the data represents. Domain intelligence enters through plugins that extend the engine without contaminating it.

A plugin does not change what Vajra computes. It enriches what Vajra knows.

---

## The Plugin Architecture

Plugins contribute four kinds of extensions:

1. **Type recognizers** — pattern matchers that identify domain-specific value types (ICD-10 codes, NPIs, SWIFT codes)
2. **Concern profiles** — custom scoring weight vectors and rendering templates
3. **Relationship hints** — domain knowledge about which fields form logical groups
4. **Custom renderers** — domain-specific essence rendering templates

Plugins cannot modify the core analysis pipeline, access the filesystem beyond their own configuration, make network calls, or mutate the input document. They are additive. They are isolated.

---

## The VajraPlugin Trait

```rust
pub trait VajraPlugin: Send + Sync {
    /// Plugin identifier.
    fn name(&self) -> &str;

    /// Plugin version string.
    fn version(&self) -> &str;

    /// Additional type recognizers beyond the core DFA bank.
    /// These run alongside the core recognizers during semantic lifting.
    fn type_recognizers(&self) -> Vec<Box<dyn TypeRecognizer>> {
        vec![]
    }

    /// Additional concern profile definitions.
    /// These appear alongside built-in profiles in `vajra profiles`.
    fn concern_profiles(&self) -> Vec<Box<dyn ConcernProfile>> {
        vec![]
    }

    /// Field relationship heuristics.
    /// Example: "code + description + system = coded concept"
    fn relationship_hints(&self) -> Vec<RelationshipHint> {
        vec![]
    }

    /// Custom rendering templates for essence output.
    fn renderers(&self) -> Vec<Box<dyn EssenceRenderer>> {
        vec![]
    }
}
```

Every method has a default implementation that returns empty. A plugin can implement only the capabilities it needs.

---

## TypeRecognizer

Type recognizers extend Vajra's semantic lifting layer. They match raw string values against domain-specific patterns.

```rust
pub trait TypeRecognizer: Send + Sync {
    /// The name of the recognized type (e.g., "ICD-10-CM", "CPT", "NPI").
    fn type_name(&self) -> &str;

    /// Returns true if the value matches this type's pattern.
    fn matches(&self, value: &str) -> bool;

    /// Optional confidence level for the match.
    fn confidence(&self, value: &str) -> f64 {
        if self.matches(value) { 1.0 } else { 0.0 }
    }
}
```

Type recognizers run during Layer 4 (Semantic Lifting) of the engine pipeline. They are evaluated after the core DFA bank, allowing domain-specific patterns to augment — not override — the core type inference.

---

## RelationshipHint

Relationship hints tell Vajra that certain field combinations form logical groups:

```rust
pub struct RelationshipHint {
    /// Fields that form a logical group when co-located.
    pub field_patterns: Vec<String>,

    /// Name for this relationship.
    pub name: String,

    /// Description of what the group represents.
    pub description: String,
}
```

Example from the medical plugin:

```rust
RelationshipHint {
    field_patterns: vec![
        "code".to_string(),
        "system".to_string(),
        "display".to_string(),
    ],
    name: "coded-concept".to_string(),
    description: "A coded value with its coding system and human-readable display".to_string(),
}
```

When Vajra finds `code`, `system`, and `display` as sibling keys in an object, the medical plugin's relationship hint identifies this as a coded concept — not three independent strings.

---

## The Medical Plugin: vajra-domain-med

The medical plugin is the reference implementation. It demonstrates every plugin capability.

### Type Recognizers

| Recognized Type | Pattern | Example Values |
|---|---|---|
| ICD-10-CM | `[A-Z][0-9]{2}(\.[0-9A-Z]{1,4})?` | `E11.9`, `J44.1`, `M54.5` |
| ICD-10-PCS | `[0-9A-HJ-NP-Z]{7}` | `0SG00ZJ` |
| CPT | `[0-9]{5}` (with known range validation) | `99213`, `99214`, `27447` |
| HCPCS | `[A-V][0-9]{4}` | `J0129`, `G0438` |
| NDC | `[0-9]{4,5}-[0-9]{3,4}-[0-9]{1,2}` | `0069-0770-01` |
| NPI | `[0-9]{10}` (with Luhn check) | `1234567893` |
| Denial Reason | `(CO\|PR\|OA\|PI\|CR)-[0-9]{1,3}` | `CO-45`, `PR-1`, `OA-23` |

### Relationship Hints

| Hint | Fields | Meaning |
|---|---|---|
| Coded Concept | `code`, `system`, `display` | A value from a terminology system |
| Service Line | `procedure_code`, `charge_amount`, `service_date`, `status` | A line item on a claim |
| Patient Identity | `patient.id`, `patient.name`, `patient.dob` | Patient demographic group |
| Provider Identity | `provider.npi`, `provider.name`, `provider.taxonomy` | Provider identification group |
| Adjudication | `allowed_amount`, `paid_amount`, `status`, `adjustment` | Payment determination group |

### What It Enables

With the medical plugin loaded, `vajra inspect` on a medical claim produces:

```text
=== Domain Type Recognition ===
  $.claims[*].diagnosis[*].code           E11.9      ICD-10-CM
  $.claims[*].diagnosis[*].code           J44.1      ICD-10-CM
  $.claims[*].service_lines[*].procedure_code  99213  CPT
  $.claims[*].provider.npi                1234567890 NPI
  $.claims[*].service_lines[*].adjustment.reason  CO-45  Denial Reason
```

Without the plugin, those values are just strings. With it, they are clinically meaningful codes.

---

## Building Your Own Plugin

### Step 1: Create a Crate

```bash
cargo new vajra-domain-finance --lib
```

### Step 2: Depend on vajra-types

```toml
# Cargo.toml
[dependencies]
vajra-types = { version = "0.1", path = "../vajra-types" }
```

### Step 3: Implement the Trait

```rust
use vajra_types::traits::{VajraPlugin, TypeRecognizer, RelationshipHint};

pub struct FinancePlugin;

impl VajraPlugin for FinancePlugin {
    fn name(&self) -> &str { "finance" }
    fn version(&self) -> &str { "0.1.0" }

    fn type_recognizers(&self) -> Vec<Box<dyn TypeRecognizer>> {
        vec![
            Box::new(SwiftCodeRecognizer),
            Box::new(IbanRecognizer),
            Box::new(CurrencyCodeRecognizer),
        ]
    }

    fn relationship_hints(&self) -> Vec<RelationshipHint> {
        vec![
            RelationshipHint {
                field_patterns: vec![
                    "amount".to_string(),
                    "currency".to_string(),
                ],
                name: "monetary-value".to_string(),
                description: "Amount with its currency denomination".to_string(),
            },
        ]
    }
}

struct SwiftCodeRecognizer;

impl TypeRecognizer for SwiftCodeRecognizer {
    fn type_name(&self) -> &str { "SWIFT/BIC" }

    fn matches(&self, value: &str) -> bool {
        let len = value.len();
        (len == 8 || len == 11)
            && value[..4].chars().all(|c| c.is_ascii_uppercase())
            && value[4..6].chars().all(|c| c.is_ascii_uppercase())
            && value[6..8].chars().all(|c| c.is_ascii_alphanumeric())
    }
}
```

### Step 4: Register the Plugin

Static plugins are compiled into the binary at build time by adding the crate to `vajra-cli`'s dependencies.

Dynamic plugins are loaded at runtime via `libloading` from the plugin directory (default: `~/.vajra/plugins/`).

---

## Error Isolation

Plugins run in an isolation boundary. If a plugin panics or returns an error:

1. The panic is caught at the plugin boundary (via `std::panic::catch_unwind`).
2. Core analysis continues without the plugin's contributions.
3. The plugin failure is recorded in the output's provenance metadata.
4. A diagnostic message is emitted to stderr.

```text
vajra: plugin "finance" failed during type recognition: index out of bounds
vajra: continuing analysis without finance plugin contributions
```

No plugin failure can crash Vajra. No plugin can corrupt the core analysis. The isolation is structural, not aspirational.

---

## Plugin Constraints

A plugin **may:**
- Register type recognizers, profiles, relationship hints, and renderers
- Read its own configuration files
- Use any safe Rust code internally

A plugin **may not:**
- Modify the core analysis pipeline
- Access the filesystem beyond its own config directory
- Make network calls
- Mutate the input document
- Introduce nondeterminism (all plugin methods must be deterministic)

---

## Shipped Plugins

Six domain plugins ship with Vajra, all enabled by default via feature flags:

| Domain | Plugin | Type Recognizers | Hints |
|---|---|---|---|
| Medical / EDI | `vajra-domain-med` | ICD-10, CPT, HCPCS, NDC, NPI, Diagnosis Code | 6 (claim service line, diagnosis, patient, provider, adjudication, denial) |
| Security | `vajra-domain-sec` | CVE, IPv4, IPv6, CIDR, MAC, SHA-256, SHA-1, MD5, JWT, MITRE ATT&CK Technique, MITRE Tactic, CVSS | 6 (network flow, alert classification, vulnerability, auth, process execution, DNS) |
| DevOps | `vajra-domain-devops` | Container ID, Semver, Git SHA, Docker Image, AWS ARN, GCP Resource, CIDR, Cron, K8s Namespace, Terraform Resource | 6 (K8s pod spec, deployment metadata, service endpoint, Terraform, CI pipeline, container spec) |
| Source Code | `vajra-domain-source` | snake_case, camelCase, PascalCase, SCREAMING_SNAKE, import paths, source file paths | 6 (function definition, class definition, import statement, parameter list, conditional, loop) |
| Encoding | `vajra-domain-encoding` | Base64, Base64URL, hex, URL-encoded, HTML entities, Unicode escapes, PEM, data URI, quoted-printable, MIME encoded word, Punycode, double-encoded, mixed-encoding | 3 (content+encoding, transfer encoding, encoded/decoded pairs) |
| GitHub | `vajra-domain-github` | PR number, issue number, GitHub username, repo slug, commit SHA, branch name, label, milestone, review state, merge method | 7 (pull request, issue, review, commit, release, workflow run, discussion) |

### Feature Flags

```toml
# vajra-cli/Cargo.toml
[features]
default = ["medical", "security", "devops", "source", "encoding", "github"]
medical = ["vajra-domain-med"]
security = ["vajra-domain-sec"]
devops = ["vajra-domain-devops"]
source = ["vajra-source", "vajra-domain-source"]
encoding = ["vajra-domain-encoding"]
github = ["vajra-domain-github"]
all-plugins = ["medical", "security", "devops", "source", "encoding", "github"]
```

Build without a plugin: `cargo build --no-default-features --features security,devops`

---

## The Security Plugin: vajra-domain-sec

The security plugin recognizes types commonly found in SIEM events, vulnerability scans, threat intelligence feeds, and network flow data.

### Type Recognizers

| Recognized Type | Pattern | Example Values |
|---|---|---|
| CVE ID | `CVE-YYYY-NNNNN` | `CVE-2024-3400`, `CVE-2023-44487` |
| IPv4 | Dotted-quad, each octet 0-255 | `192.168.1.1`, `10.0.0.1` |
| IPv6 | Full, compressed, mixed notation | `2001:db8::1`, `::1` |
| CIDR | IPv4/prefix (0-32) | `10.0.0.0/8`, `192.168.1.0/24` |
| MAC Address | Colon or hyphen separated | `aa:bb:cc:dd:ee:ff` |
| SHA-256 | 64 lowercase hex chars | `e3b0c44298fc1c14...` |
| SHA-1 | 40 lowercase hex chars | `da39a3ee5e6b4b0d...` |
| MD5 | 32 lowercase hex chars | `d41d8cd98f00b204...` |
| JWT | `eyJ...\.eyJ...\.sig` | JSON Web Tokens |
| MITRE ATT&CK Technique | `T\d{4}(.\d{3})?` | `T1059`, `T1059.001` |
| MITRE ATT&CK Tactic | `TA\d{4}` | `TA0001`, `TA0040` |
| CVSS Vector | `CVSS:3.x/AV:.../...` | Full CVSS v3 vector strings |

---

## The DevOps Plugin: vajra-domain-devops

The DevOps plugin recognizes types in Kubernetes manifests, Terraform state, CI/CD pipeline output, Docker configurations, and cloud infrastructure JSON.

### Type Recognizers

| Recognized Type | Pattern | Example Values |
|---|---|---|
| Container ID | 12 or 64 lowercase hex chars | `a1b2c3d4e5f6` |
| Semver | `v?MAJOR.MINOR.PATCH(-pre)?(+build)?` | `v1.2.3`, `1.0.0-beta.1` |
| Git SHA | 7-12 or 40 lowercase hex chars | `a1b2c3d`, full 40-char SHA |
| Docker Image | `[registry/]repo:tag` or `repo@sha256:digest` | `nginx:latest`, `gcr.io/proj/img:v1` |
| AWS ARN | `arn:aws:service:region:account:resource` | `arn:aws:s3:::my-bucket` |
| GCP Resource | `projects/*/...` or `organizations/*/...` | `projects/my-proj/topics/t` |
| CIDR Block | IPv4/prefix (0-32) | `10.0.0.0/16` |
| Cron Expression | 5-field cron pattern | `0 */6 * * *` |
| K8s Namespace | DNS-1123 labels, known system namespaces | `kube-system`, `my-app-staging` |
| Terraform Resource | `provider_type.name` | `aws_instance.web` |

---

## The Source Code Plugin: vajra-domain-source

The source code plugin recognizes patterns in the JSON trees produced by `vajra-source` (tree-sitter CST-to-JSON output). It works alongside `vajra-source`, which handles the parsing.

### Type Recognizers

| Recognized Type | Pattern | Example Values |
|---|---|---|
| snake_case identifier | `[a-z][a-z0-9]*(_[a-z0-9]+)+` | `my_function`, `get_value` |
| camelCase identifier | `[a-z]...[A-Z]...` | `myFunction`, `getValue` |
| PascalCase identifier | `[A-Z][a-zA-Z0-9]+` | `MyClass`, `HttpClient` |
| SCREAMING_SNAKE_CASE | `[A-Z][A-Z0-9]*(_[A-Z0-9]+)+` | `MAX_SIZE`, `HTTP_STATUS` |
| Import path | `mod::path` or `pkg.Class` or `@scope/pkg` | `std::collections::HashMap` |
| Source file path | Path ending in `.rs`, `.py`, `.go`, etc. | `src/main.rs`, `lib/utils.py` |

### Relationship Hints

| Hint | Pattern | Meaning |
|---|---|---|
| Function definition | name + parameters + body | A function or method |
| Class definition | name + body + inheritance | A class or struct |
| Import statement | path + optional alias | A use/import declaration |
| Parameter list | type + name pairs | Function parameters |
| Conditional block | condition + consequence + alternative | An if/else construct |
| Loop block | condition/iterator + body | A for/while loop |

---

## The Encoding Plugin: vajra-domain-encoding

The encoding plugin detects data encodings embedded in JSON string values. It identifies Base64, hex, URL encoding, HTML entities, PEM certificates, and more — including adversarial patterns like double encoding and mixed encoding used for evasion.

### Type Recognizers (3 Tiers)

**Tier 1 — Definite confidence (structural markers, near-zero false positives):**

| Recognized Type | Pattern | Example Values |
|---|---|---|
| PEM block | `-----BEGIN ...-----` prefix/suffix | Certificates, private keys |
| Data URI | `data:mime;base64,...` | Embedded images, payloads |
| MIME encoded word | `=?charset?B/Q?...?=` | Email header encoding |
| Punycode | `xn--` prefix | Internationalized domain names |

**Tier 2 — Dominant confidence (strong patterns, low false positives):**

| Recognized Type | Pattern | Example Values |
|---|---|---|
| URL encoded | 2+ `%XX` sequences + trial decode | `hello%20world%21` |
| Quoted-printable | 3+ `=XX` sequences | MIME email encoding |
| HTML entity | 2+ `&...;` entities | `&lt;script&gt;` |
| Unicode escape | 2+ `\uXXXX` or `\xNN` | `\u0048\u0065` |
| Base64URL | 16+ chars, URL-safe alphabet | API tokens, URL-safe data |

**Tier 3 — Heuristic (aggressive false positive gating):**

| Recognized Type | Detection | Security Signal |
|---|---|---|
| Base64 | 24+ chars, div-by-4, trial decode, entropy gate | Obfuscated payloads, exfiltration |
| Hex encoded | 32+ chars, excludes known hash lengths | Shellcode, binary blobs |
| Double encoded | Decode reveals another encoding | Evasion technique (`%253C` → `%3C` → `<`) |
| Mixed encoding | 2+ encoding types in one value | Obfuscation, WAF bypass |

### Layer Peeling API

Beyond type recognition, the plugin provides `detect_encoding_layers()` for recursive analysis:

```rust
use vajra_domain_encoding::detect_encoding_layers;

let layers = detect_encoding_layers("%2548ello%2520world", 5);
// Returns: [url_encoded(depth=0), url_encoded(depth=1)]
```

Bounded at depth 5, decode capped at 4KB per layer. Catches `base64(url(hex(payload)))`.

---

## The GitHub Plugin: vajra-domain-github

The GitHub plugin recognizes types commonly found in GitHub API responses, webhook payloads, and exported repository data (PRs, issues, commits, reviews, releases, workflow runs).

### Type Recognizers

| Recognized Type | Pattern | Priority | Confidence | Example Values |
|---|---|---|---|---|
| PR Number | `#\d+` or bare integer in PR context | 10 | 0.90 | `#142`, `1587` |
| Issue Number | `#\d+` or bare integer in issue context | 10 | 0.90 | `#23`, `456` |
| GitHub Username | `[a-zA-Z0-9](-?[a-zA-Z0-9]){0,38}` | 20 | 0.75 | `copyleftdev`, `octocat` |
| Repo Slug | `owner/repo` pattern | 15 | 0.85 | `copyleftdev/vajra`, `rust-lang/rust` |
| Commit SHA | 7-40 hex chars in commit context | 10 | 0.95 | `a1b2c3d`, full 40-char SHA |
| Branch Name | Ref-like strings with `/` separators | 25 | 0.70 | `main`, `feature/cascade-cmd` |
| Label | Known label patterns (bug, enhancement, etc.) | 30 | 0.65 | `bug`, `good first issue` |
| Milestone | Version-like or sprint-like strings | 30 | 0.60 | `v1.0`, `Sprint 12` |
| Review State | One of: approved, changes_requested, commented, dismissed | 5 | 1.00 | `approved`, `changes_requested` |
| Merge Method | One of: merge, squash, rebase | 5 | 1.00 | `squash`, `rebase` |

### Relationship Hints

| Hint | Field Patterns | Meaning |
|---|---|---|
| Pull Request | `number`, `title`, `state`, `author`, `base`, `head` | A pull request record |
| Issue | `number`, `title`, `state`, `labels`, `assignees` | An issue record |
| Review | `author`, `state`, `body`, `submitted_at` | A PR review |
| Commit | `sha`, `message`, `author`, `date` | A commit record |
| Release | `tag_name`, `name`, `published_at`, `assets` | A release record |
| Workflow Run | `name`, `status`, `conclusion`, `run_number` | A CI workflow run |
| Discussion | `title`, `author`, `category`, `answer` | A GitHub discussion |

---

## Future Plugin Domains

The architecture supports any domain:

| Domain | Plugin | Type Recognizers |
|---|---|---|
| Financial | `vajra-domain-finance` | SWIFT, IBAN, CUSIP, currency codes |
| Telecom | `vajra-domain-telecom` | E.164 numbers, IMSI, CDR fields |
| IoT / Sensor | `vajra-domain-iot` | Sensor types, unit patterns, device IDs |

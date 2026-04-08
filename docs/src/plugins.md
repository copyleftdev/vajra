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

## Future Plugin Domains

The architecture supports any domain. The medical plugin is first because it validates the interface:

| Domain | Plugin | Type Recognizers |
|---|---|---|
| Medical / EDI | `vajra-domain-med` | ICD-10, CPT, HCPCS, NDC, NPI |
| Financial | `vajra-domain-finance` | SWIFT, IBAN, CUSIP, currency codes |
| Telecom | `vajra-domain-telecom` | E.164 numbers, IMSI, CDR fields |
| IoT / Sensor | `vajra-domain-iot` | Sensor types, unit patterns, device IDs |
| Cloud / DevOps | `vajra-domain-cloud` | ARN, GCP resource IDs, K8s metadata |

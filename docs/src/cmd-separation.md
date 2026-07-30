# separation

`separation` answers the first question anyone asks of a labelled corpus: **which fields actually distinguish the classes, and how well?**

Every ingredient already exists elsewhere in vajra — this command assembles them into one ranked, comparable report, and reports the context needed to read it honestly.

---

## Usage

```bash
vajra separation <input> --label-field <field> [flags]
```

**Arguments:**

| Argument | Description |
|---|---|
| `<input>` | Path to a JSON file, NDJSON batch, or `-` for stdin |

**Flags:**

| Flag | Description | Default |
|---|---|---|
| `--label-field <f>` | Field holding the ground-truth label (`label` or `$.label`) | required |
| `--base-rate <p>` | Assumed population prevalence of the positive class | off |
| `--positive-class <v>` | Which label value is positive | first by name |
| `--top-k <N>` | Report only the N strongest features (0 = all) | `0` |
| `--format <fmt>` | Output format: `text`, `json`, `markdown`, `compact-ai` | `text` |
| `--input-format <fmt>` | Override auto-detected input format | auto |
| `--quiet` | Suppress progress output | off |

---

## Rank by mutual information

`mutual_information` — I(field; label), in bits — is the ranking key, and the only column comparable across field types. It is symmetric, so it does not depend on which side you call the predictor, and it is measured against **one shared denominator**: H(label) over every labelled record.

That last point matters. A field present on only some records has its own local class balance, so measuring its MI against *that* subset would give it a different achievable ceiling than every other feature. Instead, absence is treated as a value (`(absent)`), which keeps the denominator identical for all features — and is more correct anyway, since absence is usually informative.

Numeric fields are quantile-binned before the entropy measures, for the reason described in [`invariants`](./cmd-invariants.md): a near-unique continuous field otherwise "determines" the label trivially. `binned` records where this applied, `coverage` records how often the field was present.

## `auc` and `separation` only where they are defined

| Column | Defined when |
|---|---|
| `auc` | The field is numeric **and** the label has exactly two classes |
| `separation` = \|2·AUC − 1\| | Same |
| `mutual_information`, `relationship_strength`, `conditional_entropy` | Always |

AUC needs an ordering. Unordered categories have none, and a three-class label has no single ROC curve — so these columns are `null` rather than filled with a comparable-looking number that means nothing.

`separation` equals \|Cliff's delta\|, so it is directly comparable to the `effect_size` reported by [`drift`](./cmd-drift.md).

## `--base-rate`: what the field would actually cost

A separation score says how much signal a field carries. It does not say what acting on it would deliver — and a balanced corpus flatters every rule.

With `--base-rate`, each feature also reports the single rule maximising Youden's J, and the precision that rule would give at the assumed prevalence:

```console
$ vajra separation features.ndjson --label-field label \
    --positive-class malicious --base-rate 0.0001
```

| feature | rule | TPR | FPR | precision @ 1e-4 |
|---|---|---|---|---|
| `has_repository` | `< 1` | 0.7824 | 0.1388 | **0.00056** |
| `has_install_hook` | `>= 1` | 0.5925 | 0.0833 | **0.00071** |
| `has_description` | `< 1` | 0.4074 | 0.0972 | **0.00042** |

All three look strong on the corpus and all three would produce well over a thousand false alarms per true detection at real prevalence. That gap is the point of the flag.

When a field predicts the *negative* class, the **complement** rule is reported (`< 1` above rather than `>= 1`), with the rates swapped — otherwise the output would hand you a rule worse than doing the opposite.

## Choosing the positive class

`--positive-class` decides what TPR, FPR and precision *mean*. The default is the first class by name, purely for determinism; set it explicitly whenever the direction matters. `mutual_information` is unaffected, being symmetric.

---

## Example: JSON Output

```json
{
  "label_field": "label",
  "labelled_records": 288,
  "classes": { "benign": 72, "malicious": 216 },
  "baseline_entropy": 0.8112781244591328,
  "binary": true,
  "positive_class": "malicious",
  "base_rate": 0.0001,
  "features": [
    {
      "path": "$.has_repository",
      "kind": "numeric",
      "count": 288,
      "distinct_values": 2,
      "coverage": 1.0,
      "binned": false,
      "mutual_information": 0.2448,
      "relationship_strength": 0.3018,
      "conditional_entropy": 0.5665,
      "auc": 0.1782,
      "separation": 0.6435,
      "operating_point": {
        "rule": "< 1",
        "tpr": 0.7824,
        "fpr": 0.1388,
        "youden_j": 0.6435,
        "precision_at_base_rate": 0.00056
      }
    }
  ]
}
```

---

## Scale

This command accumulates the full labelled corpus in memory: quantile cuts need the value distribution, and AUC needs per-class value vectors, so neither can be computed in a single bounded pass. Memory grows with records x fields.

That matches [`invariants`](./cmd-invariants.md), which has the same shape for the same reason, but it does mean `separation` is not usable on a corpus larger than memory. There is no `--streaming` mode yet; a sketch-based variant (the crate already carries DDSketch) would be the way to add one.

Also note `--base-rate` is only meaningful for a two-class label — with more classes there is no single decision rule to price, and the command says so rather than printing an empty table.

---

## Watch for identifier-like fields

A near-unique **string** field — a primary key, a UUID, a filename — will report MI equal to the baseline entropy, because each distinct value maps to one label. That is an artefact, not a finding. `distinct_values` and `coverage` are reported so it is visible; binning fixes the numeric version of the same trap but cannot fix the categorical one.

---

## Pairs Well With

- [`invariants`](./cmd-invariants.md) — unsupervised cross-field relationships, same entropy machinery
- [`drift`](./cmd-drift.md) — `effect_size` there is the same \|Cliff's delta\| as `separation` here
- [`stats`](./cmd-stats.md) — per-field distributions behind these scores

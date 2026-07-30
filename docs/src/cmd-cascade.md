# cascade

`cascade` detects temporal cause-effect chains in event data. Given a stream of timestamped events grouped by entity, it identifies sequences where one event type triggers another — and measures how reliably that pattern holds.

Where `anomalies` finds single-record outliers, `cascade` finds multi-record temporal patterns: event A happens to entity X, then event B follows within a window.

---

## Usage

```bash
vajra cascade <input> [flags]
```

**Arguments:**

| Argument | Description |
|---|---|
| `<input>` | Path to a JSON/NDJSON file, `-` for stdin, or an HTTP URL |

**Flags:**

| Flag | Description | Default |
|---|---|---|
| `--entity-field <path>` | JSONPath to the entity identifier (e.g., `'$.author'`) | required |
| `--time-field <path>` | JSONPath to the timestamp field (e.g., `'$.date'`) | required |
| `--event-field <path>` | JSONPath to the event type field (e.g., `'$.type'`) | required |
| `--response-values <vals>` | Comma-separated list of event values that count as responses (e.g., `fix,revert`) | required |
| `--format <fmt>` | Output format: `text`, `json`, `markdown`, `compact-ai` | `text` |
| `--input-format <fmt>` | Override auto-detected input format | auto |
| `--quiet` | Suppress progress output | off |

---

## What It Reports

### Cascade Rate

The fraction of trigger events that are followed by a response event from the same entity within the detection window. A high cascade rate means the cause-effect pattern is reliable.

### Self-Fix Rate

The fraction of cascades where the same entity that caused the trigger also produced the response. Measures whether entities clean up their own problems.

### Hot Entities

Entities that appear disproportionately in cascade chains. These are the nexus points — the authors, services, or components that most frequently participate in cause-and-effect sequences.

### Cascade Chains

The full chain detail: trigger event, response event, entity, timestamps, and time delta between cause and effect.

---

## Algorithm

O(n log n). Records are grouped by entity using a BTreeMap (ordered map), sorted by timestamp within each group, then scanned linearly to detect trigger-response pairs. The BTreeMap ensures deterministic iteration order regardless of input ordering.

---

## Example: Commit Cascade Analysis

```bash
vajra cascade commits.ndjson \
  --entity-field '$.author' \
  --time-field '$.date' \
  --event-field '$.type' \
  --response-values 'fix,revert'
```

```text
=== Cascade Report ===
Records: 1,247
Entities: 34
Trigger events: 312
Response events: 89

Cascade rate:  0.285 (89 of 312 triggers followed by a response)
Self-fix rate: 0.742 (66 of 89 responses by the same entity)

Hot entities:
  alice       23 cascades (25.8%)
  bob         14 cascades (15.7%)
  charlie      9 cascades (10.1%)

Cascade chains (top 5 by frequency):
  bug -> fix        62 occurrences, median delta: 2.3 days
  bug -> revert     18 occurrences, median delta: 0.4 days
  regression -> fix  9 occurrences, median delta: 4.1 days
```

---

## Example: JSON Output

```bash
vajra cascade commits.ndjson \
  --entity-field '$.author' \
  --time-field '$.date' \
  --event-field '$.type' \
  --response-values 'fix,revert' \
  --format json
```

```json
{
  "cascade_rate": 0.3333333333333333,
  "total_events": 3,
  "total_cascades": 1,
  "self_fix_rate": 0.0,
  "cascades": [
    {
      "entity": "a.rs",
      "trigger": { "author": "", "time": "2025-01-01", "value": "feat: add" },
      "response": { "author": "", "time": "2025-01-02", "value": "fix: repair" },
      "same_author": false
    }
  ],
  "hot_entities": [
    { "entity": "a.rs", "total": 2, "cascades": 1, "cascade_ratio": 0.5 }
  ]
}
```

`self_fix_rate` is the share of cascades where trigger and response share an author. A low rate means someone else is cleaning up.

---

## When to Use It

- **Incident response analysis.** Which errors lead to fixes, and how quickly? Which lead to reverts?
- **Developer workflow.** Who introduces bugs and who fixes them? Is there a self-fix pattern?
- **Service dependency.** Event A in service X triggers event B in service Y — cascade reveals the coupling.
- **Repository health.** Measure how reliably bugs get resolved and how long the resolution takes.

---

## Pairs Well With

- [`stats`](./cmd-stats.md) — statistical profile of the event fields before cascade analysis
- [`anomalies`](./cmd-anomalies.md) — unusual cascade chains (an entity that never self-fixes) are anomaly candidates
- [`invariants`](./cmd-invariants.md) — cascade patterns are temporal invariants; invariants discovers structural ones
- [`essence`](./cmd-essence.md) — cascade metrics feed into essence generation for project health assessments

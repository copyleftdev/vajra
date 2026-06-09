---
title: "Stop Feeding Agents Raw Data"
subtitle: "Agents do not need bigger piles of context. They need better instruments."
tags: ai, rust, agents, data, cli
cover_image: https://raw.githubusercontent.com/copyleftdev/vajra/fix/source-property-runtime/articles/assets/stop-feeding-agents-raw-data-cover.png
published: false
---

# Stop Feeding Agents Raw Data

I used to think the problem was the agent.

I would hand it a large JSON export and ask a reasonable question: what changed, what looks risky, what should we investigate before release?

It would find something. It always found something.

But it missed fields. It over-indexed on irrelevant values. It hallucinated patterns in JSON that weren't there. It noticed one dramatic-looking record and ignored the boring distribution that made the record meaningful. So I tried the usual fixes: stricter prompts, longer instructions, bigger context windows, more examples.

The real problem was simpler.

I was handing the agent noisy context.

Not bad data. Bad context.

Raw structured data is full of repetition, incidental shape, schema noise, nulls, IDs, timestamps, nested boilerplate, and fields that only mean something in aggregate. Humans struggle with that. Agents do too.

If you paste a huge API response, GitHub export, log dump, CSV, or source tree into a model and ask it to "find what matters," you are asking the model to do two jobs at once:

1. Measure the data.
2. Reason about the measurements.

Those are different jobs.

Vajra is my attempt to split them apart.

Before the model reasons, Vajra measures. Before the agent plans, Vajra reduces the raw input into evidence.

## The Agent Context Problem

The default agent workflow is still too close to copy and paste.

Give the agent a repository. Give it a JSON dump. Give it a pile of logs. Give it a directory of semi-structured files. Then ask it to infer structure, identify anomalies, detect drift, preserve evidence, and decide what to do next.

That sounds powerful, but it is a bad division of labor.

Agents are useful for interpretation, synthesis, planning, and tradeoff analysis. They are not the best tool for counting every path in a nested document, computing entropy across values, comparing two distributions, detecting type instability, or producing a repeatable fingerprint of a shape.

Those jobs should be done by boring software.

Reliable software.

Software that returns the same answer every time.

That is the layer I wanted in front of the agent.

## What Vajra Does

Vajra is a Rust CLI and library for analyzing structured data. It started from a simple frustration with large JSON, then grew into a signal layer for things agents routinely ingest:

- structured data like JSON, NDJSON, YAML, CSV, and TSV
- operational artifacts like Markdown, PDF text, CPU profiles, and strace logs
- repositories, GitHub exports, and source code parsed through tree-sitter

The goal is not fuzzy summarization. The goal is stable measurement.

For example:

```bash
vajra inspect payload.json
vajra stats events.ndjson
vajra anomalies batch.json
vajra drift baseline.json current.json
vajra essence payload.json --profile ai --format compact-ai --budget 500 --redact
vajra inspect src/main.rs --input-format source --lang rust --semantic-paths
vajra governance commits.json
```

That gives you paths, types, fingerprints, entropy, cardinality, outliers, rarity, schema drift, source-code structure, contributor concentration, and compact essences that are designed to be read by humans or agents.

The important word is stable.

Same input. Same config. Same version. Same result.

That matters more than it sounds.

If an agent is going to call a tool, update a plan, make a recommendation, and then call the tool again after a change, the tool cannot behave like a mood ring. It has to be an instrument.

## Raw Data Is Not Context

Here is the mistake I kept making.

I treated "available to the model" as equivalent to "usable by the model."

They are not the same.

A 5 MB JSON export may fit in a context window. That does not mean it is good context. Most of those tokens may be repeated object structure, duplicated IDs, common timestamps, low-signal values, or fields whose meaning only appears after aggregate analysis.

Vajra turns that raw input into a smaller set of measured claims.

Instead of giving an agent every record and hoping it notices that one path is unstable, give it a measured signal shaped like this:

```text
Path: $.claims[*].status
Dominant type: string
Cardinality: 7
Entropy: 1.82
Rare values: reversed_manual, pending_external_review
Null rate: 0.00
```

Instead of asking it to eyeball schema drift between two payloads, give it a drift report shaped like this:

```text
Removed path: $.member.coverage.plan_id
Added path: $.member.coverage.policy_ref
Type changed: $.claim.amount string -> number
Distribution drift: $.claim.status high
Severity: critical
```

Instead of making it inspect a full repository export, give it project-health signals such as:

```text
Bus factor: 2
Contributor concentration: high
One-commit contributor rate: 0.61
Most active author share: 0.44
Velocity trend: declining
```

These examples are representative shapes, not a promise that every command emits exactly these lines. The point is the workflow: let the tool compute the measurements, then let the agent reason from them.

Now the agent can do the work it is actually useful for.

It can reason about consequences. It can ask whether the drift is intentional. It can propose a migration plan. It can decide what evidence belongs in a report. It can open an issue with the relevant fields instead of a blob of raw data.

## Measure, Then Reason

The agentic workflow I want looks like this:

```text
raw data / repo / logs / API export
    -> deterministic analysis
    -> compact evidence bundle
    -> agent reasoning
    -> action plan or code change
    -> re-analysis
    -> verification
```

That loop is different from the usual paste-and-hope loop.

The agent has fewer reasons to infer measurements from raw records. It gets to reason from measured facts.

Vajra can sit before the model and decide what deserves attention. It compresses without pretending to be creative. It surfaces anomalies without pretending every anomaly is automatically important. It preserves paths and evidence so the agent can point back to the data.

That is the difference between a summary and an instrument.

A summary says, "Something changed."

An instrument says, "This path changed, this value distribution moved, this type is unstable, and here is the evidence."

## A Concrete Agent Workflow

Imagine an agent responsible for reviewing a GitHub project before a release.

The naive version gets a large export of issues, pull requests, commits, and releases. It tries to infer project health from raw records. It may notice recent failures, but miss contributor concentration. It may summarize issue titles, but miss stale ownership patterns. It may produce a confident report with weak evidence.

The Vajra version starts by measuring first:

```bash
vajra ingest-github owner/repo --output .vajra/repo
vajra governance .vajra/repo/commits.json --format json > .vajra/governance.json
vajra core-team .vajra/repo/commits.json --format json > .vajra/core-team.json
vajra anomalies .vajra/repo/issues.json --format json > .vajra/issue-anomalies.json
vajra essence .vajra/repo/issues.json --profile ai --format compact-ai --redact > .vajra/issues.essence.json
```

Now the agent does not need to start by reading everything.

It reads the measurements.

Instead of producing a vague release memo, it can flag specific evidence-backed risks:

- ownership concentrated in two maintainers
- unusual issue clusters before release
- repeated labels or states that deserve triage
- fields that changed shape across exports
- anomalies that need business context before they are treated as problems

If the agent recommends action, the action can be checked.

Did the generated report cite the right paths? Did the relevant anomaly disappear or become explainable? Did a migration remove the type instability? Did the same input produce the same output after a refactor?

That is the kind of feedback loop agents need.

## Why Determinism Matters

Agentic systems are already probabilistic at the reasoning layer. The tools around them should not add unnecessary randomness.

If a tool summarizes differently every time, the agent's plan changes for reasons that are hard to debug. If a score cannot be decomposed, the agent cannot explain it. If a drift report is not stable, CI cannot trust it. If a fingerprint changes because object keys were ordered differently, the tool is measuring formatting noise instead of structure.

Vajra is built around boring constraints:

- deterministic ordering
- stable fingerprints
- explicit paths
- explainable scores
- profile-driven reduction
- no silent mutation of source data

It is not glamorous. It is what makes the output usable as agent context.

Agents do not just need more context. They need context they can trust.

## What This Does Not Solve

This layer does not make judgment disappear.

Vajra can measure structure and distributions. It does not decide whether an anomaly is business-important. It can flag contributor concentration. It cannot know whether that concentration is a temporary release push, an organizational risk, or just how a small project works.

Compact essences are evidence bundles. They are not replacements for source data when auditability, legal review, or incident response requires the original records.

And if sensitive data is involved, it should be redacted or minimized before it is sent to any model or external service. That is why the CLI supports `--redact`, but the operational policy still belongs to you.

The point is not to remove human or agent judgment.

The point is to stop wasting judgment on work that deterministic tools can do better.

## Where This Goes

Serious agent systems will not be built from prompts alone.

They need toolchains that produce reproducible intermediate artifacts. They need ways to shrink context without losing evidence. They need stable measurements before interesting reasoning.

That is the bet behind Vajra.

Not that agents should know everything.

That agents should be handed better instruments.

Because when you stop feeding agents raw data, you give them a better job:

reasoning from evidence.

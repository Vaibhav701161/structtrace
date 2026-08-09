# StructTrace

> Your schema passed. Did the answer?

[![CI](https://github.com/Vaibhav701161/structtrace/actions/workflows/ci.yml/badge.svg)](https://github.com/Vaibhav701161/structtrace/actions/workflows/ci.yml)
[![Rust 1.87+](https://img.shields.io/badge/Rust-1.87%2B-000000?logo=rust)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-155eef.svg)](LICENSE)
[![No telemetry](https://img.shields.io/badge/telemetry-none-087a55.svg)](SECURITY.md)

StructTrace is paired regression testing for stable-contract structured extraction pipelines. It
evaluates the same golden cases against a baseline and candidate while a caller-facing JSON Schema
remains fixed. It separates structural validity from deterministic task correctness, preserves
every failure in the denominator, and produces local evidence plus a multi-state CI release gate.

**Release status:** private release candidate. Core local engineering checks are passing; public
binary installation, clean-browser performance across the OS matrix, and independent-user
validation are still open release gates. The repository does not claim public readiness.

```text
matched cases        baseline + candidate       one evidence bundle
─────────────        ────────────────────       ───────────────────
input only        -> baseline + candidate   -> strict output capture
expected + eval   ------------------------> deterministic scoring
                                             -> paired transitions
                                             -> report, gate, replay
```

The execution boundary is deliberate: adapters can receive the case input and explicitly
model-visible metadata, but never the golden `expected` value or evaluation-only metadata.
Only the evaluation engine can access labels.

## Hero workflow: invoice extraction migration

The primary example compares a baseline and candidate invoice extractor under one unchanged output
contract. The candidate repairs missing currency and vendor fields but introduces financially wrong
tax, total, and line-item values that still satisfy the schema.

| Evidence | What StructTrace checks |
|---|---|
| Structure | strict JSON and the unchanged invoice schema |
| Identity | exact invoice number, Unicode-normalized vendor, and canonical date |
| Finance | currency, exact-decimal fields, and arithmetic invariants |
| Detail | keyed line-item matching with missing, extra, and changed items |
| Deployment | sample size, scored coverage, errors, regressions, and uncertainty |

```bash
structtrace demo invoice --open
structtrace init invoice-extractor --preset extraction
```

The bundled 120-case release scenario records 90/120 baseline passes and 75/120 candidate passes,
with 30 baseline-only and 15 candidate-only transitions. Its release gate is `FAILED`, not merely
underpowered. The generated 12-case extraction project remains an inspectable onboarding fixture.
Its exact hotspot acceptance test requires:

| Pointer | Regressions | Improvements | Candidate failures |
|---|---:|---:|---:|
| `/total` | 2 | 0 | 2 |
| `/tax` | 1 | 0 | 1 |
| `/line_items` | 1 | 0 | 1 |
| `/vendor_name` | 0 | 1 | 0 |
| `/currency` | 0 | 2 | 0 |

## What the report gives you

- strict whole-output JSON parsing, with surrounding prose treated as failure;
- external JSON Schema validation with remote retrieval disabled;
- deterministic extraction, classification, and tool-argument evaluators;
- valid-but-wrong counts as a first-class result;
- the complete paired 2x2 transition matrix;
- candidate-minus-baseline percentage-point effect;
- exact McNemar test and seeded paired bootstrap interval;
- field-level regression and improvement hotspots;
- mean, median, and p95 latency, retries, token use, and user-priced cost;
- a filterable case explorer with JSON-aware diffs;
- independent release-gate rules with stable exit codes;
- SQLite storage, BLAKE3-bound portable artifacts, and deterministic score replay.

The offline report contains no CDN assets, analytics, telemetry, or remote runtime dependency.
Large runs use a searchable case index, 50-case lazy-loaded chunks, and 25-case pagination rather
than embedding every case into the summary page. A one-file export is produced only below its
configured size ceiling. The checked-in [scale validation](docs/src/report-scale-validation.md)
covers 1,000 paired nested invoice records and states the measured envelope and remaining browser
validation gap.

## Install

StructTrace is currently a private release candidate. No public binary release exists yet, so the
repository does not present the checked-in installers as a working public install path. Install
from source with stable Rust 1.87 or newer:

```bash
git clone https://github.com/Vaibhav701161/structtrace.git
cd structtrace
cargo install --path crates/structtrace-cli --locked
structtrace --help
structtrace doctor
```

The release workflow and checksum-verifying installers are release-candidate assets. They become
supported only after clean Linux, macOS, and Windows virtual-machine installation checks have been
recorded and a versioned GitHub release is actually published.

## Five-minute workflow

```bash
structtrace init my-structured-output-check
cd my-structured-output-check

# Inspect the generated dataset, schema, outputs, evaluators, and thresholds.
structtrace run
structtrace report latest --open
structtrace gate latest
structtrace replay latest
```

Choose the integration closest to your application:

```bash
structtrace init my-check --template recorded
structtrace init my-check --template command
structtrace init my-check --template python
structtrace init my-check --template openai-compatible
structtrace init my-check --preset extraction
```

Initialization refuses to overwrite existing StructTrace files.

## Execution sources

| Adapter | Status | Use it when | Execution behavior |
|---|---|---|---|
| Recorded JSONL | Stable | outputs already exist | no process, Python, or provider required |
| Command | Beta | application is in any language | bounded versioned JSONL over stdin/stdout; no shell |
| Python callable | Beta | application exposes a Python function | bounded bridge; exceptions retained as failures |
| OpenAI-compatible | Experimental | comparing models or request settings | explicit endpoint, bounded total case deadline and opt-in backoff |

All adapters receive a label-free case view and produce the same `VariantOutput` envelope before entering the same storage, evaluation, report, gate, and replay path. Adapter errors, timeouts, malformed responses, nonzero exits, and missing outputs never shrink the denominator.

## Evaluators and outcomes

Built-in deterministic evaluators cover:

- exact JSON equality;
- one or multiple JSON Pointer comparisons;
- enum/classification accuracy;
- Unicode-normalized strings and calendar-aware canonical dates;
- arbitrary-length exact integers and exact-decimal numeric tolerance;
- keyed array matching with missing/extra item evidence and financial invariants;
- required fields;
- tool selection;
- selected tool arguments;
- experimental custom command and Python evaluators for user-defined deterministic checks.

Custom evaluators use persistent, bounded workers by default and require an implementation version
that is bound into replay receipts. They remain beta until the release benchmark and independent
workload validation are complete; built-in evaluators are the private-alpha stable path. The
checked-in [1,000-case by two-variant scale check](benchmarks/external-evaluator-1000/README.md)
retains all 2,000 receipts and exercises worker shutdown with the normal test suite.

Evaluators are composed into named `all_of` or `any_of` outcomes. The user must choose one primary semantic or executable outcome; StructTrace refuses to infer correctness from schema validity.

```yaml
evaluators:
  - id: exact_priority
    kind: json_pointer_exact
    pointer: /priority
    expected_pointer: /priority
  - id: exact_team
    kind: json_pointer_exact
    pointer: /assigned_team
    expected_pointer: /assigned_team

outcomes:
  routing_correct:
    all_of: [exact_priority, exact_team]

analysis:
  primary_outcome: routing_correct
```

The machine-readable configuration schema is at [schemas/structtrace.schema.json](schemas/structtrace.schema.json).

## Release gates

Rules are evaluated independently. A schema improvement cannot hide a semantic regression, and a correctness improvement is still shown when an operational threshold fails.

```yaml
gate:
  min_cases: 100
  min_primary_scored_rate: 0.99
  max_primary_evaluator_error_rate: 0.01
  max_primary_not_applicable_rate: 0.0
  max_primary_unscored_rate: 0.0
  max_primary_regression_pp: 1.0
  max_valid_but_wrong_increase_pp: 0.5
  min_candidate_schema_validity: 1.0
  max_error_rate: 0.0
  max_timeout_rate: 0.0
  latency:
    max_p95_increase_percent: 25
    min_coverage: 1.0
  cost:
    max_average_increase_percent: 20
    min_coverage: 1.0
```

```bash
structtrace gate latest                 # human output
structtrace gate latest --format json   # automation
structtrace gate latest --format github # Actions annotations
structtrace gate latest --verify replay # high-assurance CI
```

The default gate verifies the manifest-bound `summary.json` hash before applying the stored
decision. `--verify replay` additionally reconstructs the run from retained source artifacts.
Operational rules compare matched case observations and enforce their configured measurement
coverage. Gate exits distinguish a quality failure (`10`), no configured decision (`11`),
insufficient evidence (`12`), and gate evaluation error (`13`). Malformed input, execution
failure, artifact corruption, and protocol failure use separate codes.

## Durable and replayable by design

Each run lives under `.structtrace/runs/<ULID>/` and contains:

```text
manifest.json              BLAKE3 provenance and lifecycle
run.sqlite3                versioned durable store
inputs/                    retained config, dataset, schema, outputs
cases.jsonl                complete paired case records
external-evaluator-receipts.jsonl  hash-bound external evaluator facts, when used
discordances.jsonl         regression and valid-but-wrong slice
summary.json / summary.md  machine and human summaries
logs/                      separated adapter diagnostics
report/index.html          offline report
report/case-index.json     redacted search and filter index
report/cases/              lazy 50-case display chunks
```

`structtrace replay` reconstructs scores from the retained dataset and baseline and candidate
output JSONL, verifies cross-artifact consistency, and recomputes parsing, schema validation,
built-in evaluators, outcomes, valid-but-wrong classification, transitions, McNemar, bootstrap
intervals, and every gate rule. It does not rerun a model or reproduce provider generation.
Side-effecting external evaluator programs are not re-executed: their definition-, request-, and
response-bound receipts are verified instead. Local
hashes establish integrity and consistency, not cryptographic authorship. Resume is allowed only
when all bound experimental inputs still match.

## Privacy boundary

StructTrace sends no telemetry and performs no automatic uploads. Provider credentials are read only from named environment variables; secret values are never written into resolved configuration or manifests.

```yaml
storage:
  retain_raw_outputs: false
  redaction:
    json_pointers:
      - /input/customer_email
      - /input/phone
```

Raw retention is enforced before portable output artifacts are written. Full provider-response
retention defaults to off. Report redaction is fail-closed, includes model-visible metadata in its
source, and removes matching echoes from parsed output, evaluator evidence, provider response
bodies, retry records, and search indexes. The report server binds to a random loopback-only
address. Opening a completed report serves its immutable finalized bundle; the optional one-file
export copies a finalized size-limited derivative. `--export-share` creates a new aggregate-only
directory with every case-level value omitted.

Output, subprocess diagnostics, and report embedding are bounded independently. Defaults are conservative and every setting has a compiled hard ceiling:

```yaml
limits:
  max_output_bytes_per_case: 4194304
  max_stderr_bytes_per_process: 1048576
  max_report_raw_bytes_per_case: 262144
  max_report_total_bytes: 268435456
  max_single_file_report_bytes: 10485760
```

Oversized command, Python, or provider output fails closed and remains in the denominator. Report truncation changes only the HTML view, never the scored artifact.

## Research foundation, without universal claims

The offline research fixture reproduces three normalized accepted paired matrices from the
Contract Sensitivity Lab evidence chain:

| Study | Baseline | Candidate | Candidate-only | Baseline-only |
|---|---:|---:|---:|---:|
| Corrected Qwen | 18/49 | 24/49 | 9 | 3 |
| Canonical Llama | 92/150 | 82/150 | 6 | 16 |
| Executable tool pilot | 26/30 | 24/30 | 1 | 3 |

This fixture verifies the published transition counts and statistics; it is not a replay of the
original raw model-generation artifacts. The point is not that one representation universally
wins. The same class of contract-preserving change produced different effects across evaluated
systems. StructTrace exists so teams measure the effect on their own model, prompt, schema,
backend, and workload.

## Architecture

```text
structtrace-cli
      │
      ├── structtrace-engine ── SQLite, lifecycle, resume, replay
      │          │
      │          ├── structtrace-adapters ── recorded / command / Python / OpenAI
      │          ├── structtrace-core ────── config, schema, evaluators, statistics
      │          └── structtrace-report ──── bounded offline bundle and loopback serving
      │
      └── stable exit codes and human / JSON / GitHub output
```

Important semantic and product decisions are recorded in [DECISIONS.md](DECISIONS.md).

## Development

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --workspace --release
```

The test suite uses deterministic fixtures and local mock servers. It requires no external API key, model provider, GPU, or network service.

## Documentation

The documentation book covers configuration, integrations, metrics, reports, CI, privacy, replay, and troubleshooting. Start at [docs/src/introduction.md](docs/src/introduction.md) while the static site is being built.

External usability evidence is deliberately tracked separately from automated correctness. The [release-candidate validation protocol](docs/src/release-candidate-validation.md) defines the tasks, privacy boundary, retained evidence, and acceptance gate; it currently makes no claim that outside validation has occurred.

## Product boundaries

StructTrace does not migrate or rewrite schemas, optimize prompts, execute tool calls, choose a
winning representation, or guarantee model quality. Its stable scope is paired regression testing
while the caller-facing output contract stays fixed. It does not require an LLM judge for that
workflow. Version 1 uses fixed baseline-then-candidate execution and variant-level resume;
interleaved live-provider scheduling and case-level paid-call resume remain explicit future work.

## Contributing and security

See [CONTRIBUTING.md](CONTRIBUTING.md) for the development contract and [SECURITY.md](SECURITY.md) for the local security boundary and responsible disclosure process.

StructTrace is licensed under the [MIT License](LICENSE).

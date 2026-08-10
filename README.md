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

The primary human workflow is now a local browser product backed by that same Rust engine:

```bash
structtrace open
```

Drop golden, baseline, and candidate JSONL, JSON, or CSV files; confirm mappings visually; select
deterministic correctness rules; choose the authority of the evidence; and inspect the exact paired
regressions. StructTrace generates the reproducible project and CI check after the visual setup.
The UI is capability-protected, loopback-only, offline after installation, and has no login,
telemetry, CDN, or Node.js runtime. See the [local UI guide](docs/src/local-ui.md).

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

The execution boundary is deliberate: adapters receive an opaque transport token, case input, and
explicitly model-visible metadata, but never the dataset ID, golden `expected` value, or
evaluation-only metadata. OpenAI prompt templates receive no identifier. Only the evaluation
engine retains the token-to-dataset-ID mapping and can access labels.

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

The bundled demo is deliberately an honest 12-case workflow demonstration: both variants pass
9/12, with three regressions and three improvements. Its gate is `INSUFFICIENT EVIDENCE` because a
small fixture must not be presented as release evidence. StructTrace uses an explicit evidence-unit
definition, excludes operational metadata from the default fingerprint, and refuses a release
decision when repeated observations in one unit conflict. No input row is selected by position.
Its exact hotspot acceptance test requires:

| Pointer | Regressions | Improvements | Candidate failures |
|---|---:|---:|---:|
| `/total` | 2 | 0 | 2 |
| `/tax` | 1 | 0 | 1 |
| `/line_items` | 1 | 0 | 1 |
| `/subtotal` | 1 | 0 | 1 |
| `/vendor_name` | 0 | 1 | 0 |
| `/currency` | 0 | 2 | 0 |

## What the report gives you

- strict whole-output JSON parsing, with surrounding prose treated as failure;
- external JSON Schema validation with remote retrieval disabled;
- deterministic extraction, classification, and tool-argument evaluators;
- valid-but-wrong counts as a first-class result;
- a complete-denominator deployment-success view and separate jointly scored semantic view;
- an order-invariant paired 2x2 matrix over explicit evidence units;
- conflict-aware exact McNemar testing and a bounded seeded paired bootstrap interval;
- field-level regression and improvement hotspots;
- mean, median, and p95 latency, retries, token use, and user-priced cost;
- a filterable case explorer with JSON-aware diffs;
- independent release-gate rules with stable exit codes;
- SQLite storage, BLAKE3-bound portable artifacts, and deterministic score replay.

The offline report contains no CDN assets, analytics, telemetry, or remote runtime dependency.
Large runs use a searchable case index, 50-case lazy-loaded chunks, and 25-case pagination rather
than embedding every case into the summary page. A one-file export is produced only below its
configured size ceiling. The checked-in [scale validation](docs/src/report-scale-validation.md)
covers both 1,000 paired nested invoice reports and a complete 10,000-case recorded run/replay
measurement, and states the remaining ingestion and browser-validation limits.

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

Run `structtrace doctor --strict` after initializing a project. Strict doctor is static: it
validates configuration and bounded retained inputs without importing or executing application
code. `--handshake` imports Python workers and resolves callables without business cases.
`--execute-cases N` is the deliberate, side-effecting local execution check. OpenAI-compatible
variants are never called by doctor.

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

Existing retained outputs can be onboarded without inventing correctness semantics:

```bash
structtrace init comparison --from-outputs \
  --dataset data.jsonl --baseline baseline.jsonl --candidate candidate.jsonl \
  --schema schema.json \
  --dataset-id-pointer /document_id \
  --dataset-input-pointer /payload \
  --dataset-expected-pointer /ground_truth \
  --output-id-pointer /record_id \
  --output-value-pointer /result \
  --field-evaluator /vendor_name=normalized_string \
  --field-evaluator /invoice_date=canonical_date:iso,dmy_slash \
  --field-evaluator /total=decimal_exact \
  --keyed-array '/line_items=/sku;/description:normalized_string,/quantity:exact_integer,/amount:decimal_tolerance:0.01' \
  --financial-invariants --gate-mode regression
```

The importer accepts canonical StructTrace envelopes and ordinary JSONL such as
`{"record_id":"invoice-1","result":{...}}`. It validates and snapshots every bounded source,
normalizes ordinary output rows, and writes `ONBOARDING.md` with expected/baseline/candidate field
coverage and observed types. Its field union comes from the external schema and all three observed
sources, so a field newly omitted by the candidate remains visible. In a terminal, omitted paths
and field semantics are prompted; automation uses the explicit flags above.

Supported guided choices include exact JSON or pointers, normalized strings, canonical dates,
exact integers, exact/tolerant decimals, keyed arrays with per-field comparators, and opt-in invoice
financial invariants. Suggestions never become semantic truth silently. A new imported workload
defaults to a Regression gate; Release mode still requires the complete safe release profile.

Bundled demos and research fixtures are isolated from production history. `latest` always means
the latest completed production run; `latest-demo`, `latest-research`, and `latest-any` are
explicit opt-in selectors.

Manage retained runs with `structtrace runs list`, `runs show`, `runs latest --kind production`,
`runs archive`, and confirmed inactive-run deletion. Archives include a BLAKE3 receipt for every
manifest-bound copied file; unbound debug files and secrets are excluded from verified archives.
Verified archives preserve the run's retained evidence and are owner-only on Unix; they are not a
substitute for the aggregate-only `report --export-share` derivative.

## Execution sources

| Adapter | Status | Use it when | Execution behavior |
|---|---|---|---|
| Recorded JSONL | Stable candidate | outputs already exist | no process, Python, or provider required |
| Command | Beta | application is in any language | bounded versioned JSONL over stdin/stdout; no shell |
| Python callable | Beta | application exposes a Python function | bounded bridge; exceptions retained as failures |
| OpenAI-compatible | Experimental | comparing models or request settings | explicit endpoint, bounded total case deadline and opt-in backoff |

All adapters receive a label-free case view and produce the same `VariantOutput` envelope before entering the same storage, evaluation, report, gate, and replay path. Adapter errors, timeouts, malformed responses, nonzero exits, and missing outputs never shrink the denominator.

For live execution, StructTrace snapshots model-facing schemas before either variant starts and
hash-binds only configured implementation inputs under explicit size/count ceilings. It rechecks
those schema and implementation hashes before finalization. External evaluator
`implementation.sources` and `implementation.digest` participate in the same provenance boundary.

## Evaluators and outcomes

Built-in deterministic evaluators cover:

- exact JSON equality;
- one or multiple JSON Pointer comparisons;
- enum/classification accuracy;
- Unicode-normalized strings and calendar-aware canonical dates;
- arbitrary-length exact integers and exact-decimal numeric tolerance;
- keyed array identity matching with nested normalized, integer, decimal, and date field evidence;
- exact-decimal invoice financial invariants with path-specific diagnostics;
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
  mode: release
  min_cases: 100
  min_unique_cases: 100
  max_duplicate_case_rate: 0.01
  min_primary_fully_evaluated_rate: 0.99
  max_primary_component_error_rate: 0.01
  max_primary_component_not_applicable_rate: 0.0
  max_primary_component_unscored_rate: 0.0
  max_deployment_regression_pp: 1.0
  min_candidate_deployment_success_rate: 0.95
  min_candidate_parse_validity: 1.0
  min_candidate_schema_validity: 1.0
  max_candidate_valid_but_wrong_rate: 0.02
  max_error_rate: 0.0
  max_timeout_rate: 0.0
  latency:
    max_p95_increase_percent: 25
    min_coverage: 1.0
  cost:
    max_average_increase_percent: 20
    min_coverage: 1.0
```

Deployment automation must call `structtrace release-check latest`. It performs complete replay and
returns zero only for an explicitly authorized Release-mode decision.
Advisory and regression passes remain useful analysis results but can never authorize release.

```bash
structtrace gate latest                 # human output
structtrace gate latest --format json   # automation
structtrace gate latest --format github # Actions annotations
structtrace gate latest --verify replay # high-assurance CI
structtrace release-check latest        # deployment authorization only
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
logs/                      optional bounded process diagnostics; off by default
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
when all bound experimental inputs still match, including local entry-source files, dependency
lockfiles, the Git commit, and the dirty-tree fingerprint.

## Privacy boundary

StructTrace sends no telemetry and performs no automatic uploads. Provider credentials are read
only from named environment variables; credential values are never written into resolved
configuration or manifests. User-controlled process logs are off by default.

```yaml
storage:
  retain_raw_outputs: false
  process_logs:
    mode: off
    max_total_bytes: 4194304
  redaction:
    json_pointers:
      - /input/customer_email
      - /input/phone
```

Raw retention changes persisted presentation only; stable scoring facts, statistics, and gates are
retention-invariant. Full provider-response retention defaults to off. Report redaction is fail-closed, includes model-visible metadata in its
source, and removes matching echoes from parsed output, evaluator evidence, provider response
bodies, retry records, and search indexes. The report server binds to a random loopback-only
address and requires a fresh 256-bit capability path for every asset. Opening a completed report serves its immutable finalized bundle; the optional one-file
export copies a finalized size-limited derivative. `--export-share` creates a new aggregate-only
directory with every case-level value omitted.

Output, subprocess diagnostics, report embedding, bootstrap samples, and total bootstrap work are
bounded independently. Defaults are conservative and every setting has a compiled hard ceiling:

```yaml
limits:
  max_config_bytes: 1048576
  max_dataset_bytes: 268435456
  max_recorded_output_bytes: 536870912
  max_schema_bytes: 16777216
  max_cases: 10000
  max_jsonl_line_bytes: 16777216
  max_replay_artifact_bytes: 536870912
  max_output_bytes_per_case: 4194304
  max_stderr_bytes_per_process: 1048576
  max_report_raw_bytes_per_case: 262144
  max_report_total_bytes: 268435456
  max_single_file_report_bytes: 10485760
```

Oversized command, Python, or provider output fails closed and remains in the denominator. Report truncation changes only the HTML view, never the scored artifact.
The default case ceiling is the measured 10,000-case v1 envelope. The 100,000-case compiled ceiling
is opt-in and is not advertised as measured capacity. Dataset and recorded-output sources are
whole-file but bounded in v1, so memory still scales with the configured source-byte limits; the
tool does not claim streaming or million-row ingestion.

| Clean-tree Linux measurement | Wall time | Peak RSS |
|---|---:|---:|
| 10,000-case run | 153.08 s | 325,640 KiB |
| Complete replay | 2.16 s | 288,124 KiB |

The exact commit, binary/lockfile digests, source sizes, command results, and artifact size are in
[`benchmarks/recorded-output-10000/result.json`](benchmarks/recorded-output-10000/result.json).

## Research foundation, without universal claims

The offline research command produces three separate normalized runs and a non-inferential index
from the Contract Sensitivity Lab evidence chain:

| Study | Baseline | Candidate | Candidate-only | Baseline-only |
|---|---:|---:|---:|---:|
| Corrected Qwen | 18/49 | 24/49 | 9 | 3 |
| Canonical Llama | 92/150 | 82/150 | 6 | 16 |
| Executable tool pilot | 26/30 | 24/30 | 1 | 3 |

These fixtures verify the published per-study transition counts and statistics; they are not a
replay of the original raw model-generation artifacts. StructTrace deliberately calculates no
pooled effect or cross-study release gate. The same class of contract-preserving change produced
different effects across evaluated systems, so teams must measure their own model, prompt, schema,
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
workflow. The stable-candidate path uses fixed baseline-then-candidate execution and variant-level resume;
interleaved live-provider scheduling and case-level paid-call resume remain explicit future work.

## Contributing and security

See [CONTRIBUTING.md](CONTRIBUTING.md) for the development contract and [SECURITY.md](SECURITY.md)
for responsible disclosure. Product expectations are explicit in the [support](SUPPORT.md),
[compatibility](COMPATIBILITY.md), [deprecation](DEPRECATION.md), and [roadmap](ROADMAP.md)
policies. Format upgrades are documented in the [migration guide](docs/src/migrations.md).

StructTrace is licensed under the [MIT License](LICENSE).

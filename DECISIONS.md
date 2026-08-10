# StructTrace engineering decisions

This file records decisions that materially define observable behavior.

## Separate product repository

StructTrace is implemented independently from Contract Sensitivity Lab. The
research repository is evidence and methodology, not a runtime dependency.

## Rust core with optional subprocess bridges

The evaluation engine, storage, metrics, report generation, and CLI are Rust.
Python and arbitrary application languages integrate through a versioned JSONL
subprocess protocol. This keeps recorded-output and demo workflows free of a
Python dependency.

## Strict parsing is the scored default

A model output passes JSON parsing only when the entire trimmed output is one
JSON value. Diagnostic recovery may be displayed later, but cannot alter the
scored parse outcome.

## Complete denominator

Every dataset case remains in each variant denominator. Missing output, adapter
failure, timeout, parse failure, schema failure, and evaluator error are retained
as unsuccessful outcomes and remain separately observable.

## JSON Schema references are local by default

The validator is compiled without HTTP or filesystem retrieval features. Remote
references fail closed. Explicit local reference support will be added only with
path confinement and artifact hashing.

## Exact numeric semantics

JSON numbers retain arbitrary-precision lexical representations. Exact integer
comparison never converts through binary floating point. Tolerance evaluation
uses decimal arithmetic.

## SQLite plus finalized portable artifacts

SQLite is the durable execution store. Completed runs also emit versioned JSON
and JSONL artifacts for inspection, replay, and long-term portability.

## One scoring path for every variant adapter

Recorded files, command processes, Python callables, and OpenAI-compatible
endpoints normalize into the same output envelope. Model execution may differ,
but parsing, schema validation, evaluators, outcomes, paired statistics, storage,
reports, gates, and replay do not. The original adapter configuration remains in
the manifest.

## Retries are opt-in evidence

Provider retries default to zero. When enabled, retry attempts are retained per
case and included in operational summaries. StructTrace does not silently retry
deterministic application commands or evaluator failures.

## Pricing is user-supplied

StructTrace never infers provider prices. Exact decimal cost is computed only
when the user supplies input and output prices and a currency. Mixed currencies
are not aggregated.

## Resume is hash-locked at the variant boundary

Execution checkpoints bind exact and normalized configuration, dataset, schema,
variant, evaluator, outcome, bootstrap, and gate definitions. A completed
baseline or candidate is reused byte-for-byte after interruption. A changed
input refuses resume. Partially completed variants are rerun as a unit so a
persistent process keeps its defined lifecycle.

## External evaluator replay uses retained receipts

Custom evaluator programs may represent deterministic execution with local side
effects. Initial scoring executes them under an explicit timeout and retains the
result and diagnostics. Replay recomputes built-in evaluators and outcome
composition from that hash-bound receipt instead of silently re-executing a
potential side effect.

## Retention is presentation-only

Scoring uses a stable normalized observation that excludes raw formatting,
prompts, provider envelopes, evaluator prose, and retry response bodies.
Removing those values may reduce display and forensic fidelity, but cannot
change parsing status, evaluator states, evidence classification, statistics,
or the release decision. A retained parse-error marker preserves the original
strict-parse outcome when raw text is intentionally discarded.

## Report redaction removes repeated echoes

Configured case-envelope JSON Pointers identify sensitive values. Report
generation redacts both the selected location and equal values echoed in model
output or evaluator details. Bounded offline reports use a small summary, a redacted search index,
and lazy case chunks; one-file derivatives exist only below a configured limit. Reports are served
only on loopback beneath a fresh capability URL, so knowing the port is insufficient. The dedicated
share export omits all case-level content.

## Release gates are multi-state evidence decisions

No configured rules means `NOT_CONFIGURED`, not pass. Any deployment decision requires explicit
minimum case count, scored coverage, and evaluator error/not-applicable/unscored ceilings. A run
that lacks those safeguards is `INSUFFICIENT_EVIDENCE` even when its observed point estimate looks
favorable. Quality failure, missing evidence, and gate execution error retain distinct states and
exit codes.

## Stable product scope keeps the external contract fixed

StructTrace measures a baseline and candidate against one unchanged caller-facing schema. It does
not claim to migrate schemas or execute model-proposed tools. Recorded output is stable; local
command and Python process integrations are beta; direct provider execution is experimental.

## Evidence units are explicit and conflicts fail closed

Captured rows are descriptive execution observations, not automatically independent evidence.
The default evidence-unit fingerprint includes input, expected output, and model-visible metadata
while excluding arbitrary operational metadata. Users may declare a grouping pointer or explicit
include-list. Repeated groups with disagreeing retained status or evidence are not collapsed to the
first row: they are excluded from inference and force `INSUFFICIENT_EVIDENCE`. Primary report cards,
paired statistics, evaluator counts, hotspots, and the gate share the same evidence-unit population.

## Dataset identity never crosses the live application boundary

Dataset IDs remain evaluator and report identifiers. Live command and Python protocols receive a
deterministic opaque execution token that StructTrace maps back after validation. Python callables
receive no identifier, and OpenAI templates expose neither dataset IDs nor transport tokens.

## Run kinds isolate examples from production history

Every manifest and SQLite run row records `production`, `demo`,
`research_fixture`, or `test`. The default `latest` selector resolves only a
completed production run. Explicit `latest-demo`, `latest-research`, and
`latest-any` selectors prevent bundled examples from silently becoming release
evidence.

## Live execution inputs are snapshotted and bounded

Before either variant runs, StructTrace captures every model-facing schema and
hash-binds its exact bytes into the checkpoint and manifest. Only configured
executables, Python entry modules, declared `implementation.sources`, declared
digests, relevant lockfiles, and interpreter identity participate in the
implementation fingerprint. The fingerprint and model-schema hashes are
recomputed after execution; a change refuses finalization.

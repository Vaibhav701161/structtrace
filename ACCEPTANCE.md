# StructTrace acceptance record

This file separates locally observed evidence from checks that require external
infrastructure or users. It is not a substitute for CI logs or user feedback.

## Locally verified

| Requirement | Evidence |
|---|---|
| Locked clean installation | `cargo install --path crates/structtrace-cli --locked --root <isolated-prefix>` completed |
| Installed command surface | installed `structtrace --help` and offline `structtrace doctor --format json` passed |
| Support-ticket demo | installed binary reproduced baseline 10/12 and candidate 8/12 |
| Accepted-research demo | installed binary reproduced aggregate 136/229 and 130/229 plus exact per-study matrices |
| Accepted-research replay | zero artifact, row-score, and summary mismatches |
| Recorded workflow | real-binary init, run, report export, failed gate exit 10, and replay passed |
| CI gate output | real-binary GitHub mode appended a Markdown metrics table to `$GITHUB_STEP_SUMMARY` and emitted rule annotations |
| Artifact tamper detection | modifying a finalized report caused replay to return artifact-failure exit code 4 |
| Python adapter | generated template ran end to end with 2/2 versus 1/2 |
| Command adapter | generated persistent protocol ran end to end with 2/2 versus 1/2 |
| OpenAI-compatible adapter | local mock-server tests cover content, usage, exact cost, provider errors, malformed responses, and opt-in retries |
| Custom evaluators | command protocol test and end-to-end Python evaluator scoring/replay passed |
| Forced resume | process killed during candidate; resumed ULID reused baseline exactly once and replayed with zero mismatches |
| Failed lifecycle | an error after run allocation records a failure event and leaves durable state `failed`, never `complete` |
| Example projects | recorded, Python, command, document extraction, and tool calling ran and replayed with zero mismatches |
| Privacy | property tests cover pointer redaction and echo removal; retention test removes raw and provider envelopes |
| Bounded resources | configurable output, stderr, and report-embedding limits have enforced hard ceilings; command/Python and provider output-limit paths fail closed |
| Configuration safety | runtime validation enforces paths, pointers, callables, timeouts, concurrency, retries, token limits, tolerances, pricing, gates, and report-filter constraints independently of editor tooling |
| Documentation | mdBook built 42 HTML pages; 1,067 generated local references checked with zero missing targets |
| Formatting and linting | `cargo fmt --all --check` and warnings-denied Clippy passed |
| Test suite | 76 deterministic and property-based tests pass without a provider credential, external model, GPU, or network service |

## Defined but awaiting remote evidence

| Requirement | Current state |
|---|---|
| Linux CI | workflow defined; local Linux checks pass |
| macOS build and test | workflow defined; requires GitHub Actions runner evidence |
| Windows build and test | workflow defined; requires GitHub Actions runner evidence |
| Tagged release archives | workflow defined for Linux, Intel/Apple Silicon macOS, and Windows; not published |
| Private-user validation | not started; requires consenting external users and cannot be simulated honestly |

## Material boundaries

- StructTrace does not sandbox user-authorized commands, Python callables, or
  custom evaluators.
- Resume checkpoints completed variants. An interrupted in-progress variant is
  rerun as a unit to preserve persistent-process semantics.
- Replay uses hash-bound custom-evaluator receipts rather than re-executing
  potentially side-effecting evaluator code.
- Disabling raw-output retention limits reconstruction of malformed original
  text.
- Historical research matrices are separate studies and are not pooled into a
  universal effect.

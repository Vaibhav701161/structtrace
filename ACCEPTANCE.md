# StructTrace acceptance record

This file separates locally observed evidence from checks that require external
infrastructure or users. It is not a substitute for CI logs or user feedback.

## Locally verified

| Requirement | Evidence |
|---|---|
| Locked clean installation | `cargo install --path crates/structtrace-cli --locked --root <isolated-prefix>` completed |
| Installed command surface | installed `structtrace --help` and offline `structtrace doctor --format json` passed |
| Support-ticket demo | installed binary reproduced baseline 10/12 and candidate 8/12 |
| Normalized research fixture | installed binary reproduced aggregate 136/229 and 130/229 plus exact per-study matrices; this is not raw research-artifact replay |
| Research-fixture replay | zero artifact, cross-artifact, row-score, and summary mismatches |
| Recorded workflow | real-binary init, run, bounded report export, aggregate-only share export, insufficient-evidence exit 12, and replay passed |
| CI gate output | real-binary GitHub mode appended a Markdown metrics table to `$GITHUB_STEP_SUMMARY` and emitted rule annotations |
| Artifact tamper detection | modifying a finalized report caused replay to return artifact-failure exit code 4 |
| Gate integrity | default gate rejects a manifest hash mismatch; high-assurance `--verify replay` is available |
| Label isolation | command, Python, and OpenAI tests prove expected values and evaluation-only metadata do not cross the variant boundary |
| Evaluator semantics | adversarial tests cover missing pointers, per-field hotspot facts, evaluator errors, not-applicable results, and nonzero exits |
| Independent replay | replay reconstructs rows from retained dataset plus baseline/candidate inputs and detects cross-artifact tampering even after ordinary hashes are updated |
| External evaluator receipts | replay verifies definition-, request-, and response-bound facts without re-executing side-effecting programs |
| Immutable finalized report | serving/export verifies every manifest-bound report asset; redaction fails closed and strips provider/retry echoes |
| Python adapter | generated template ran end to end with 2/2 versus 1/2 |
| Command adapter | generated persistent protocol ran end to end with 2/2 versus 1/2 |
| OpenAI-compatible adapter | local mock-server tests cover content, usage, exact cost, provider errors, malformed responses, and opt-in retries |
| Custom evaluators | command protocol test and end-to-end Python evaluator scoring/replay passed |
| Forced resume | process killed during candidate; resumed ULID reused baseline exactly once and replayed with zero mismatches |
| Failed lifecycle | an error after run allocation records a failure event and leaves durable state `failed`, never `complete` |
| Multi-state gate | empty gates are not configured; evaluator errors, low coverage, not-applicable rows, unscored rows, and small samples cannot authorize deployment |
| Example projects | recorded, Python, command, document extraction, and tool-selection/argument fixtures ran through the shared scoring path |
| Invoice hero workflow | 12 nested invoices produced 9/12 versus 9/12 with six discordances, schema validity 10/12 versus 12/12, valid-but-wrong 1 versus 3, and an insufficient-evidence gate |
| Privacy | property tests cover input/label/model-visible metadata redaction and echo removal; a whole-finalized-run scan proves a provider-error secret is absent from JSONL, SQLite, reports, and logs |
| Process lifecycle | adversarial tests bound persistent EOF shutdown, inherited reader pipes, per-case timeouts, and Unix descendant process termination |
| Bounded resources | configurable output, stderr, per-case report display, complete report, and single-file limits have enforced hard ceilings; adapter/provider limit paths fail closed |
| Report scale | 1,000 nested invoice pairs generate a shell under 512 KiB, 20 lazy chunks, a total bundle under 16 MiB, and no oversized single-file derivative; isolated generation took 1.76 s with 90,428 KiB test-process RSS locally |
| Configuration safety | runtime validation enforces paths, pointers, callables, timeouts, concurrency, retries, token limits, tolerances, pricing, gates, and report-filter constraints independently of editor tooling |
| Documentation | mdBook built 43 HTML pages locally, including the explicit scale envelope and release-candidate user protocol |
| Formatting and linting | `cargo fmt --all --check` and warnings-denied Clippy passed |
| Test suite | 112 deterministic and property-based tests pass without a provider credential, external model, GPU, or network service |

## Defined but awaiting remote evidence

| Requirement | Current state |
|---|---|
| Linux CI | workflow defined; local Linux checks pass |
| macOS build and test | workflow defined; requires GitHub Actions runner evidence |
| Windows build and test | workflow defined; requires GitHub Actions runner evidence |
| Tagged release archives | workflow defines static Linux musl, Intel/Apple Silicon macOS, Windows, checksums, and attestations; not published |
| Prebuilt installer validation | shell and PowerShell installers and per-asset checksum verification are implemented; end-to-end validation awaits the first tagged release |
| Private-user validation | not started; requires consenting external users and cannot be simulated honestly |
| Browser performance matrix | clean-browser open/filter measurements remain unrecorded across release operating systems |

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

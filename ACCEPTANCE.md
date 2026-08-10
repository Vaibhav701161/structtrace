# StructTrace acceptance record

This file separates locally observed evidence from checks that require external
infrastructure or users. It is not a substitute for CI logs or user feedback.
The reproducible `scripts/record-local-acceptance.sh` command writes exact commit, toolchain,
platform, command exit status, and log hashes to `acceptance/local-release-audit.json`; prose in
this file is not treated as machine evidence.

## Locally verified

| Requirement | Evidence |
|---|---|
| Locked clean installation | `cargo install --path crates/structtrace-cli --locked --root <isolated-prefix>` completed |
| Installed command surface | installed `structtrace --help` and offline `structtrace doctor --format json` passed |
| Support-ticket demo | installed binary reproduced baseline 10/12 and candidate 8/12 |
| Normalized research fixtures | installed binary created three separate per-study runs with exact Qwen, Llama, and tool-call matrices plus a non-inferential index; no pooled gate or effect is produced |
| Research-fixture replay | each separate run had zero artifact, cross-artifact, row-score, and summary mismatches |
| Recorded workflow | real-binary init, run, bounded report export, aggregate-only share export, insufficient-evidence exit 12, and replay passed |
| CI gate output | real-binary GitHub mode appended a Markdown metrics table to `$GITHUB_STEP_SUMMARY` and emitted rule annotations |
| Artifact tamper detection | modifying a finalized report caused replay to return artifact-failure exit code 4 |
| Gate integrity | default gate rejects a manifest hash mismatch; high-assurance `--verify replay` is available |
| Label isolation | ID pointers join all overlap checks; live adapters receive stimulus-derived opaque tokens; Python callables and OpenAI templates receive no dataset ID; strict doctor fails model-visible expected-value leakage while treating original IDs as opaque |
| Evaluator semantics | adversarial tests cover missing pointers, externally supplied field facts, the four-valued `any_of` truth table, evaluator errors, not-applicable results, and nonzero exits |
| Independent replay | replay reconstructs rows from retained dataset plus baseline/candidate inputs and detects cross-artifact tampering even after ordinary hashes are updated |
| External evaluator receipts | replay verifies definition-, request-, and response-bound facts without re-executing side-effecting programs |
| Immutable finalized report | serving/export verifies every manifest-bound report asset and rejects unbound extras or symlinks; the loopback server requires a random 256-bit URL capability and rejects foreign Host/Origin/Referer values; responses are no-store |
| Python adapter | one persistent async loop serves the worker lifetime; user stdout is captured away from protocol stdout; dataclasses, Pydantic-style models, enums, paths and mappings normalize; exceptions are sanitized; malformed/nonserializable cases remain isolated |
| Command adapter | generated persistent protocol ran end to end; strict envelopes reject unknown fields and contradictory output/error states |
| OpenAI-compatible adapter | local mock-server tests cover content, usage, exact cost, provider errors, malformed responses, and opt-in retries |
| Custom evaluators | command protocol test and end-to-end Python evaluator scoring/replay passed |
| Forced resume | process killed during candidate; resumed ULID reused baseline exactly once and replayed with zero mismatches |
| Failed lifecycle | an error after run allocation records a failure event and leaves durable state `failed`, never `complete` |
| Multi-state gate | empty gates are not configured; evaluator errors, low coverage, not-applicable rows, unscored rows, and small samples cannot authorize deployment |
| Example projects | recorded, Python, command, document extraction, and tool-selection/argument fixtures ran through the shared scoring path |
| Invoice hero workflow | 12 nested invoices produced 9/12 versus 9/12 with six discordances, schema validity 10/12 versus 12/12, valid-but-wrong 1 versus 3, and an insufficient-evidence gate |
| Evidence independence | singleton, exact-duplicate, repeated-trial and label-conflict groups are explicit; raw formatting and retention do not define scored equality; repeated trials cannot select an arbitrary representative and block independent inference |
| Report denominators | descriptive all-row totals are separated from independent evidence-unit results; headline cards, paired matrix, evaluator counts, hotspots, and gate use the same non-conflicting evidence-unit population |
| Golden-answer routing | equal, root, and parent/child overlaps across ID, input, expected, model-visible metadata, and evaluation-only metadata are rejected |
| Bootstrap safety | samples are capped at 1,000,000 and samples × evidence units at 100,000,000 before result allocation or resampling |
| Date and array policy | DMY plus MDY is refused without an ambiguity policy; keyed-array evaluators require non-empty field semantics; report hotspot paths normalize array indices to `*` |
| Financial diagnosis | line amount, subtotal, tax, and total invariant tests assert exact paths, states, values, and messages |
| Local artifact security | Unix run directories are `0700`, SQLite/artifacts are `0600`, and replay refuses symlinked manifest artifacts |
| Source ingestion and replay | configuration, dataset, output, schema, case-count, and JSONL-line limits are enforced; replay reuses retained limits and file hashes stream through BLAKE3 |
| Durable provenance | SQLite cases retain input, expected, model-visible metadata, evaluation metadata, and source line, matching the portable case envelope |
| Execution-input provenance | configured implementation sources are bounded and hash-bound; evaluator declarations participate in replay receipts; model-facing schemas are captured before execution, manifest-bound, resume-bound, and rechecked with implementation inputs before finalization |
| Privacy | exact, aggressive-text, and custom-pattern policies are explicit; property tests cover echo removal; a whole-finalized-run scan proves a provider-error secret is absent from retained artifacts |
| Process lifecycle | adversarial tests bound persistent EOF shutdown, inherited reader pipes, per-case timeouts, and Unix descendant process termination |
| Bounded resources | output, stderr, report, provider, bootstrap-sample, and bootstrap-work ceilings fail closed |
| Report scale | 1,000 nested invoice pairs generate a shell under 512 KiB, 20 lazy chunks, a total bundle under 16 MiB, and no oversized single-file derivative; isolated generation took 1.76 s with 90,428 KiB test-process RSS locally |
| Configuration safety | runtime validation enforces paths, pointers, callables, timeouts, concurrency, retries, token limits, tolerances, pricing, gates, and report-filter constraints independently of editor tooling |
| Documentation | mdBook built 43 HTML pages locally, including the explicit scale envelope and release-candidate user protocol |
| Formatting and linting | `cargo fmt --all --check` and warnings-denied Clippy passed |
| Test suite | `cargo test --workspace --all-features` passes without a provider credential, external model, GPU, or network service; generated local evidence and CI output tied to the commit are authoritative |

## Defined but awaiting remote evidence

| Requirement | Current state |
|---|---|
| Linux CI | workflow defined; local Linux checks pass |
| macOS build and test | workflow defined; requires GitHub Actions runner evidence |
| Windows build and test | workflow defined; requires GitHub Actions runner evidence |
| Tagged release archives | workflow defines static Linux musl, Intel/Apple Silicon macOS, Windows, checksums, and attestations; not published |
| Prebuilt installer validation | shell and PowerShell installers and per-asset checksum verification are implemented; end-to-end validation awaits the first tagged release |
| Private-user validation | not started; requires consenting external users and cannot be simulated honestly |
| Five external installations | **PENDING** |
| Three unaided complete workflows | **PENDING** |
| Two distinct extraction workloads | **PENDING** |
| Non-maintainer dataset/evaluator design | **PENDING** |
| Repeat run after a real candidate change | **PENDING** |
| Real deployment decision | **PENDING** |
| Misleading gates / privacy incidents | **PENDING external observation**; no claim can be made before the alpha |
| Browser performance matrix | clean-browser open/filter measurements remain unrecorded across release operating systems |
| Windows descendant containment | command/Python execution remains beta until Job Object tests pass on a real Windows runner |
| Interleaved live-provider execution | OpenAI-compatible execution remains experimental; paired-interleaved scheduling and per-case paid-run resume are not complete |

## Material boundaries

- StructTrace does not sandbox user-authorized commands, Python callables, or
  custom evaluators.
- Resume checkpoints completed variants. An interrupted in-progress variant is
  rerun as a unit to preserve persistent-process semantics.
- Command/Python process-tree containment is verified on Unix; Windows Job Object
  containment is still an explicit public-release blocker.
- Replay uses hash-bound custom-evaluator receipts rather than re-executing
  potentially side-effecting evaluator code.
- Disabling raw-output retention limits reconstruction of malformed original
  text.
- Historical research matrices are separate studies and are not pooled into a
  universal effect.

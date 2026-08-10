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
| Protected research portal | the non-inferential index and all three verified study reports share one random capability-protected loopback server; the index contains only relative study links and no `file://` URL |
| Research-fixture replay | each separate run had zero artifact, cross-artifact, row-score, and summary mismatches |
| Recorded workflow | real-binary init, run, bounded report export, aggregate-only share export, insufficient-evidence exit 12, and replay passed |
| CI gate output | real-binary GitHub mode appended a Markdown metrics table to `$GITHUB_STEP_SUMMARY` and emitted rule annotations |
| Artifact tamper detection | modifying a finalized report caused replay to return artifact-failure exit code 4 |
| Gate integrity | default gate rejects a manifest hash mismatch; high-assurance `--verify replay` is available |
| Label isolation | ID pointers join all overlap checks; live adapters receive run-scoped, ordinal-specific opaque tokens; repeated stimuli have distinct tokens and unrelated runs cannot correlate them; Python callables and OpenAI templates receive no dataset ID |
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
| Multi-state gate | empty gates are not configured; evaluator errors, low joint coverage, not-applicable rows, unscored rows, small samples, schema failure, parse failure, and vacuous thresholds cannot authorize deployment |
| Gate intent | advisory never authorizes; regression requires complete evidence safeguards and a relative quality rule; release requires the safe deployment, parse, schema, valid-but-wrong, and evidence profile and is the only authorizing mode; CI can require authorization explicitly |
| Authorization-only CI | `release-check` always performs replay and exits zero only when the resulting Release gate sets `deployment_authorized`; official deployment snippets use this command |
| Outcome health | composed logical truth and required-component health are stored separately; mixed failure plus error/not-applicable states remain visible and block fully evaluated evidence |
| Strict JSON | one recursive parser rejects duplicate keys in schemas, datasets, recorded and raw outputs, worker protocols, evaluator responses, provider envelopes, and replayed artifacts across compare, run, doctor, and replay |
| Process logs | default retention is off; sanitized logs redact configured literals and header-shaped values under one total budget; truncation is marked and share exports omit logs |
| Run management | kind-aware list/show/latest, inactive-run deletion with confirmation and symlink confinement, and manifest-allowlisted verified archives that exclude unbound files are available |
| Guided recorded onboarding | `init --from-outputs` accepts canonical or ordinary JSONL, supports nondefault dataset/output mappings, validates and snapshots all four inputs, reports schema/expected/baseline/candidate field coverage and types so omitted candidate fields remain selectable, generates built-in evaluator semantics only when explicitly selected, and produces a strict-doctor/run/replay-valid project |
| Local browser product | `structtrace open` serves an embedded offline UI through a random loopback capability; the visual invoice demo and a separately mapped recorded-output comparison completed through the real engine, produced immutable artifacts, and retained insufficient-evidence authority honestly |
| Local UI security | Missing capabilities return 404, foreign Host returns 421, API responses are no-store with CSP/frame/referrer/MIME protections, browser requests carry bounded content rather than arbitrary file paths, and inactivity never stops an active run |
| Local UI accessibility | TypeScript and component tests pass; Chromium and Firefox Playwright plus axe find no WCAG A/AA violations on first launch or the real invoice decision screen; WebKit remains defined in CI and awaits a runner with its required system libraries |
| Example projects | recorded, Python, command, document extraction, and tool-selection/argument fixtures ran through the shared scoring path |
| Invoice hero workflow | 12 nested invoices produced 9/12 versus 9/12 with six discordances, schema validity 10/12 versus 12/12, valid-but-wrong 1 versus 3, and an insufficient-evidence gate |
| Evidence independence | singleton, exact-duplicate, repeated-trial and label-conflict groups are explicit; scoring occurs before retention; strict parse receipts preserve raw-disabled results; paired coverage is the true fully evaluated intersection; repeated trials block independent inference |
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
| Recorded workflow scale | Clean commit `0cd51fe` completed the deterministic 10,000-case release-binary run in 153.08 s at 325,640 KiB peak RSS and complete replay in 2.16 s at 288,124 KiB; exact inputs, commands, artifact size, and digests are in `benchmarks/recorded-output-10000/result.json`, while the 100,000-case hard ceiling remains explicitly unmeasured |
| Release archive validation | The Linux release binary was archived, extracted into a clean temporary directory, then passed `--version`, doctor, invoice demo, and full replay; the same validator is mandatory for every target in the release workflow |
| Parser and artifact fuzzing | strict JSON, adapter protocol JSON, and imported manifest/paired-case artifact targets completed local smoke runs without a crash |
| Configuration safety | runtime validation enforces paths, pointers, callables, timeouts, concurrency, retries, token limits, tolerances, pricing, gates, and report-filter constraints independently of editor tooling |
| Documentation | mdBook built successfully locally, including the browser workflow, explicit scale envelope, and release-candidate user protocol |
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
| Browser performance matrix | Chromium and Firefox functional/accessibility checks now pass locally; clean-browser 1,000/10,000-row open/filter measurements and WebKit evidence remain unrecorded across release operating systems |
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
- Disabling raw-output retention records a hash-bound strict parse receipt but intentionally
  prevents raw-byte reconstruction of malformed original text.
- Historical research matrices are separate studies and are not pooled into a
  universal effect.

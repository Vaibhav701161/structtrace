# Changelog

All user-visible artifact, protocol, metric, and command changes are recorded
here.

## Unreleased

- Advance configuration to version 3, portable artifacts to version 9, reports to version 4, and
  SQLite metadata to version 5. Version 2 runs are never silently reinterpreted.
- Make scoring retention-invariant: strict parsing, external evaluation, outcomes, statistics, and
  gates are frozen from an immutable capture before raw/provider/prompt retention is applied.
  Raw-disabled artifacts retain a versioned strict-parse receipt and do not claim raw replay.
- Separate structural, semantic, and deployment success. Complete-denominator comparisons and
  release rules now use deployment success; semantic-only inference requires both outcomes to be
  fully evaluated.
- Calculate paired evaluation coverage as the case-level intersection, reject vacuous release
  profiles, expose gate mode in machine output, and add `--require-release-authorization` for CI.
- Use strict duplicate-key schema parsing in direct comparison, archive only manifest-bound files,
  and discover onboarding pointers across schema, expected, baseline, and candidate evidence.

- Advance configuration to version 2, portable artifacts to version 8, reports to version 3,
  SQLite to version 4, and command/Python/evaluator protocols to version 3.
- Add explicit advisory, regression, and release gate modes. Only a complete release gate with an
  absolute semantic floor can authorize deployment.
- Store outcome truth separately from required-component health, and gate on fully evaluated,
  error, not-applicable, and unscored component rates.
- Reject duplicate object keys recursively at dataset, output, protocol, provider, evaluator,
  configuration, and replay boundaries.
- Apply retained output and schema limits consistently across initial execution, resume, replay,
  and strict doctor.
- Harden Python variant and evaluator bridges for non-finite numbers, mapping keys, common Python
  values, reserved fields, response consistency, and clean persistent-loop shutdown.
- Make user process logs off by default; add bounded sanitized and explicit full-sensitive modes.
- Stream report case records into atomic 50-case chunks, preflight one-file size budgets, and
  stream hash-verified server assets from disk.
- Serve all normalized research studies from one capability-protected loopback portal without a
  pooled effect or release gate.
- Add safe run listing, inspection, kind-aware latest selection, inactive-run deletion, and
  hash-receipted directory archives.

- Advance the artifact format to version 7 and subprocess protocols to version 2. Evidence was
  classified as singleton, exact duplicate, repeated trial, or label conflict with separate
  stimulus, reference, scored-observation, and operational hashes.
- Make retention and raw formatting presentation-only for evidence classification; remove
  arbitrary case-ID representatives and block independent gates for repeated-trial evidence.
- Isolate production, demo, research-fixture, and test histories; `latest` now means the latest
  completed production run, with explicit selectors for other kinds.
- Snapshot and manifest-bind model-facing schemas before execution; bound configured source
  fingerprinting, include evaluator implementations, and refuse finalization after mid-run changes.
- Harden Python workers with one persistent event loop, exclusive protocol stdout, common Python
  value normalization, explicit envelopes, protected fields, and sanitized failure fingerprints.
- Require random report capability URLs, loopback Host/Origin/Referer checks, no-store responses,
  CSP headers, and same-origin resource policy.
- Separate primary-outcome field hotspots from all-evaluator diagnostics while retaining evaluator
  identity and pass, fail, error, not-applicable, and unscored states.
- Replace dataset IDs at live adapter boundaries with opaque execution tokens and remove IDs from
  Python callable and OpenAI prompt contexts; make strict doctor fail expected-leaf and ID leakage.
- Cap bootstrap samples and total resampling work; split normalized research evidence into three
  non-pooled runs; reject ambiguous DMY/MDY policies and empty keyed-array field specifications.
- Support async Python callables, per-case serialization/protocol errors, sanitized exceptions,
  custom-evaluator field facts, strict output envelopes, and normalized array hotspot paths.
- Stream file hashing, reuse configured replay limits, complete SQLite case provenance, reject
  unbound report assets, add report CSP, and make browser-open failure non-fatal.
- Add explicit implementation source/digest bindings and exact versus aggressive text-redaction
  policies with custom literal patterns.
- Remove the pseudo-replicated 120-row invoice demo; the twelve unique fixtures now remain an
  explicitly insufficient-evidence workflow demonstration.
- Reject overlapping dataset pointers, unsafe per-case external evaluators, symlinked artifacts,
  oversized sources, and resume attempts after local implementation changes.
- Harden Unix run storage permissions and add strict doctor checks for duplicate evidence, golden
  echoes, callable imports, source bounds, and storage safety.
- Add normalized, exact-integer, decimal-tolerance, and canonical-date field comparators inside
  keyed arrays; correct subtotal invariant attribution.

- Establish the independent StructTrace Rust workspace.
- Define artifact format version 3, report format version 2, and variant protocol version 1.
- Add recorded, command, Python, and OpenAI-compatible paired execution.
- Add strict parsing, external-schema validation, deterministic evaluators,
  composed outcomes, valid-but-wrong analysis, exact McNemar, and paired
  bootstrap intervals.
- Add versioned custom command and Python evaluator execution.
- Add SQLite run storage, portable artifacts, BLAKE3 manifests, hash-locked
  resume, and full artifact replay.
- Add independent semantic, structural, reliability, latency, and cost gates.
- Add a bounded offline report with structured diffs, redacted search, evaluator filters,
  pagination, lazy case chunks, and size-limited optional single-file export.
- Add raw-output retention controls and report JSON Pointer redaction.
- Add configurable output, stderr, provider-envelope, and report-embedding
  bounds with enforced hard ceilings.
- Mark allocated runs failed on ordinary errors and defer the durable complete
  state until report, checkpoint, hashes, and final manifest all succeed.
- Append GitHub-format release-gate metrics to the Actions job summary.
- Add offline support-ticket and normalized research fixtures, runnable examples,
  mdBook documentation, cross-platform CI, and release packaging.
- Isolate golden expected values and evaluation-only metadata from every variant
  adapter; add explicit model-visible metadata and strict prompt templates.
- Correct missing-pointer, per-field hotspot, valid-but-wrong, and nonzero process
  exit semantics while preserving errors and not-applicable outcomes separately.
- Reconstruct replay from independently retained dataset and variant artifacts,
  verify cross-artifact consistency, and distinguish built-in recomputation from
  hash-bound external evaluator receipt verification.
- Capture immutable configuration and dataset source bytes at run start, verify
  summary hashes before gating, and add `gate --verify replay`.
- Make completed reports immutable and report redaction fail closed, with provider
  response and retry echoes stripped when raw retention is disabled.
- Compare operational gates on matched observations with explicit coverage,
  select the latest completed run by default, and reject unsupported extra variants.
- Support unauthenticated local OpenAI-compatible endpoints and bounded retry
  backoff with numeric `Retry-After` handling.
- Add checksum-verifying shell and PowerShell installers plus release build
  provenance attestations. No public binary release has been published yet.
- Replace Boolean gates with `PASSED`, `FAILED`, `NOT_CONFIGURED`,
  `INSUFFICIENT_EVIDENCE`, and `ERROR`, plus mandatory sample-size and scoring-coverage
  safeguards for any configured deployment decision.
- Sanitize provider error bodies, include model-visible metadata in redaction, bound total provider
  deadlines and process-tree shutdown, and add whole-run adversarial secret scans.
- Add an aggregate-only share export that omits every case-level input, label, output, prompt, and
  metadata value.
- Establish stable-contract invoice extraction as the primary workflow and explicitly label
  recorded, command/Python, and provider adapters as stable, beta, and experimental.

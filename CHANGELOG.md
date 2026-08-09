# Changelog

All user-visible artifact, protocol, metric, and command changes are recorded
here.

## Unreleased

- Establish the independent StructTrace Rust workspace.
- Define artifact format version 2 and variant protocol version 1.
- Add recorded, command, Python, and OpenAI-compatible paired execution.
- Add strict parsing, external-schema validation, deterministic evaluators,
  composed outcomes, valid-but-wrong analysis, exact McNemar, and paired
  bootstrap intervals.
- Add versioned custom command and Python evaluator execution.
- Add SQLite run storage, portable artifacts, BLAKE3 manifests, hash-locked
  resume, and full artifact replay.
- Add independent semantic, structural, reliability, latency, and cost gates.
- Add a self-contained offline report with structured diffs and case filters.
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

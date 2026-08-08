# Changelog

All user-visible artifact, protocol, metric, and command changes are recorded
here.

## Unreleased

- Establish the independent StructTrace Rust workspace.
- Define artifact format version 1 and variant protocol version 1.
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
- Add offline support-ticket and accepted-research demos, runnable examples,
  mdBook documentation, cross-platform CI, and release packaging.

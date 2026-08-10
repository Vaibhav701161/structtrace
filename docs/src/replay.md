# Replay and artifact integrity

```bash
structtrace replay latest
structtrace replay <run-id>
structtrace replay --research-fixture
```

Replay first verifies every manifest-bound artifact hash and refuses unsafe paths. It independently
reads the retained dataset and baseline and candidate output JSONL, reconstructs each paired case,
and checks the reconstruction against the derived case artifact. For raw-disabled runs it verifies
the retained strict-parse receipts and replays the canonical parsed view; it does not claim raw-byte
replay. It then recomputes strict
parsing, schema errors, built-in deterministic evaluator results, outcomes, primary passes,
valid-but-wrong flags, paired transitions, effect estimates, exact McNemar, paired bootstrap
intervals, structural success, semantic success, deployment success, and release rules.

Side-effecting custom command and Python evaluators are deliberately not re-executed. StructTrace
instead verifies receipts bound to the evaluator definition, exact request, parsed response fact,
case, and variant. Replay reports built-in results recomputed, external receipts verified, external
programs re-executed, artifact hash mismatches, cross-artifact mismatches, row-score mismatches,
and summary mismatches separately. A successful replay requires zero mismatches in every category.

`--research-fixture` creates and verifies three separate normalized transition-matrix runs from the
accepted research record. It calculates no pooled effect or gate and does not claim to replay the
original raw Qwen, Llama, or tool-call generation artifacts.

Local hashes establish run integrity and internal consistency, not authorship. Signed manifests
are not currently part of the format.

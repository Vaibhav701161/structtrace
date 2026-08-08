# Replay and artifact integrity

```bash
structtrace replay latest
structtrace replay <run-id>
structtrace replay --accepted-research
```

Replay first verifies every manifest-bound artifact hash and refuses unsafe paths. It then recomputes strict parsing, schema errors, all deterministic evaluator results, outcomes, primary passes, valid-but-wrong flags, paired transitions, effect estimates, exact McNemar, paired bootstrap interval, and release rules.

The result reports artifact mismatches, row-score mismatches, summary mismatches, and missing or incompatible artifacts separately. A successful replay requires zero mismatches in every category.

When raw retention is disabled, some invalid-output distinctions cannot be reconstructed. That limitation is explicit; replay does not fabricate missing raw evidence.

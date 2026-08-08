# Reading the report

Start with the primary outcome and release-gate state, then inspect the structural-versus-semantic table. If schema validity improved while correctness fell, the valid-but-wrong row shows how much failure moved behind a valid contract.

Use the paired transition matrix to compare candidate-only improvements with baseline-only regressions. Field hotspots identify evaluator pointers associated with those transitions. The case explorer filters baseline-only, candidate-only, both-fail, valid-but-wrong, parse failures, schema failures, and adapter errors.

Case detail shows input, expected value, raw and parsed outputs, JSON-aware changes, schema errors, evaluator evidence, case metadata, adapter metadata, and latency. Rendered prompts appear inside adapter metadata only when `report.include_prompts: true`; the default is false. Operational measurements are descriptive unless their gate rule is configured.

The final section records dataset, schema, normalized configuration, binary target, and artifact-format provenance. The report is generated locally and has no external assets.

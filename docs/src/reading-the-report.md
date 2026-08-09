# Reading the report

Start with the primary outcome and release-gate state, then inspect the structural-versus-semantic table. If schema validity improved while correctness fell, the valid-but-wrong row shows how much failure moved behind a valid contract.

Check the evidence-independence table before interpreting the paired estimate. Captured rows,
evidence units, inference denominator, duplicate groups, conflicting groups, and the configured
inference-unit definition explain exactly which population supports the gate. The separate
descriptive row totals make no independence claim. Use the independent paired transition matrix to compare candidate-only improvements with baseline-only
regressions. Field hotspots identify evaluator pointers associated with those transitions. The case
explorer searches redacted IDs and metadata, filters outcome, validity, adapter, evaluator-error,
not-applicable, and unscored states, and paginates 25 cases at a time.

Case detail shows input, expected value, raw and parsed outputs, JSON-aware changes, schema errors,
evaluator evidence, case metadata, adapter metadata, and explicit execution panels for status,
timeout or adapter error, latency, retries, token usage, cost, and finish reason. Evaluator errors
and not-applicable results remain distinct from semantic false. Rendered prompts appear inside
adapter metadata only when `report.include_prompts: true`; the default is false. Operational
measurements are descriptive unless their gate rule is configured; coverage and matched-pair
counts are shown separately.

The final section records the primary outcome, row and unique-case counts, semantic exclusion
reasons, exact McNemar result, bootstrap
confidence/sample count/seed, fixed execution schedule, variant definitions, dataset, schema,
normalized configuration, binary target, and artifact-format provenance. The report is generated
locally and has no external assets. Case bodies live in hash-bound 50-case chunks, so opening the
summary does not parse the full run.

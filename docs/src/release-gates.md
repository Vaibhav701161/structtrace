# Release gates

Release rules are independent and explicit. Evidence safeguards cover total rows, unique semantic
cases, exact duplicate rate, primary scored coverage, evaluator errors, not-applicable outcomes,
and unscored rows. Quality and
operational limits cover primary-outcome regression, valid-but-wrong growth, candidate schema
validity, adapter errors, timeouts, p95 latency, and average cost.

An empty gate is `NOT_CONFIGURED`, never passed. Once any release criterion is configured, all
evidence safeguards are required; missing or inadequate coverage is `INSUFFICIENT_EVIDENCE`.
If a run has both inadequate evidence and a directly observed quality-rule failure, the headline is
`FAILED`; every rule remains visible, including the evidence warning. A known regression is never
hidden behind a weaker evidence label.

```yaml
gate:
  min_cases: 100
  min_unique_cases: 100
  max_duplicate_case_rate: 0.01
  min_primary_scored_rate: 0.99
  max_primary_evaluator_error_rate: 0.01
  max_primary_not_applicable_rate: 0.0
  max_primary_unscored_rate: 0.0
  max_primary_regression_pp: 1.0
```

```bash
structtrace gate latest
structtrace gate latest --format json
structtrace gate latest --format github
structtrace gate latest --verify replay
```

Exit code `0` means the completed run passed all configured rules. Exit codes `10`, `11`, `12`,
and `13` mean `FAILED`, `NOT_CONFIGURED`, `INSUFFICIENT_EVIDENCE`, and gate `ERROR`, respectively.
Input, runtime, artifact, and protocol failures use distinct exit codes.

By default, the gate verifies the manifest-bound `summary.json` hash before applying the stored
decision. `--verify replay` requires complete artifact reconstruction and score replay first.
Local hashes establish integrity and consistency, not cryptographic authorship.

The report shows total rows, unique semantic cases, duplicate groups, and the largest duplicate
group. Exact semantic duplicates remain inspectable but contribute only one unit to paired
inference, quality/error rates, operational coverage, and evidence gates. The complete-denominator deployment metric and the jointly scored semantic estimate are
reported separately so an adapter crash is never mislabeled as proof of semantic error.

Latency and cost comparisons use only matched cases where both variants have a measurement. Each
operational rule also has `min_coverage`, which defaults to `1.0`. Insufficient coverage fails
closed. Raw observation counts, matched counts, and matched aggregates remain visible. An
unconfigured rule is clearly labelled not evaluated. StructTrace never uses a latency regression
to hide a correctness improvement or a schema improvement to hide a semantic regression.

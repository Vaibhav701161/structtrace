# Release gates

Release rules are independent and explicit. Evidence safeguards cover total rows, unique semantic
cases, exact duplicate rate, primary scored coverage, evaluator errors, not-applicable outcomes,
and unscored rows. Quality and
operational limits cover deployment-success regression, valid-but-wrong growth, candidate schema
validity, adapter errors, timeouts, p95 latency, and average cost.

Gate mode is explicit. `advisory` can analyze incomplete rules but never authorizes deployment.
`regression` requires every evidence safeguard and at least one relative quality rule; passing it
means only that configured regression limits passed. `release` additionally requires absolute
deployment, parse, schema, evidence-health, and valid-but-wrong floors and is the only mode that
may set `deployment_authorized=true`.

An empty gate is `NOT_CONFIGURED`, never passed. Incomplete `regression` or `release` definitions
are rejected during configuration validation and strict doctor. Missing observed coverage is
`INSUFFICIENT_EVIDENCE`. When quality and evidence both fail, human and GitHub output use the
composite `DO NOT DEPLOY: quality failed and evidence is insufficient` headline and machine output
retains both failure arrays. The stable exit status remains evidence-insufficient in that composite
case so existing automation keeps its documented code.

```yaml
gate:
  mode: release
  min_cases: 100
  min_unique_cases: 100
  max_duplicate_case_rate: 0.01
  min_primary_fully_evaluated_rate: 0.99
  max_primary_component_error_rate: 0.01
  max_primary_component_not_applicable_rate: 0.0
  max_primary_component_unscored_rate: 0.0
  max_deployment_regression_pp: 1.0
  min_candidate_deployment_success_rate: 0.95
  min_candidate_parse_validity: 1.0
  min_candidate_schema_validity: 1.0
  max_candidate_valid_but_wrong_rate: 0.02
```

```bash
structtrace gate latest
structtrace gate latest --format json
structtrace gate latest --format github
structtrace gate latest --verify replay
structtrace gate latest --require-release-authorization
```

By default, exit code `0` means the configured advisory, regression, or release rules passed; the
mode and `deployment_authorized` remain explicit. CI deployment jobs must use
`--require-release-authorization`, which returns `0` only for an authorized release-mode gate.
Exit codes `10`, `11`, `12`,
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

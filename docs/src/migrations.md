# Configuration and artifact migration

StructTrace configuration version 3 and artifact version 9 introduce explicit structural,
semantic, and deployment-success results. They also change the complete-denominator paired metric
and release-gate inputs. Old completed runs are therefore never silently reinterpreted.

## Configuration version 2 to 3

Change `version: 2` to `version: 3`. Regression gates may keep semantic-only rules, but release
gates must use the safe deployment profile:

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

Run `structtrace doctor --strict` after editing. Thresholds that merely exist but offer no useful
protection are rejected.

## Existing runs

Artifact versions before 9 cannot be promoted in place because retained data may not prove the new
deployment-success and retention-invariance semantics. Keep them as historical evidence and rerun
the comparison from the original dataset, schema, configuration, baseline, and candidate inputs.
Replay refuses unsupported artifact versions rather than guessing.

SQLite metadata upgrades to version 5 when a compatible run store is opened. This metadata update
does not convert an older portable artifact into artifact version 9.

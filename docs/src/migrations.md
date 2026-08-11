# Configuration and artifact migration

StructTrace configuration version 3 and artifact versions 9 through 11 introduce explicit structural,
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

Artifact version 11 adds an authoritative primary-outcome display projection, explicit
array-aware hotspot aggregation pointers, and optional paired effects and deployment bootstrap
intervals. A missing estimate now serializes as `null` instead of a fabricated exact zero.
Reference preflight also runs before recorded-output loading or live adapter execution.

Artifact version 10 replaced the ambiguous case-level `transition` string with typed
`deployment_transition` and optional `semantic_transition` fields. Replay recomputes and verifies
both. Version 9 runs remain readable only as historical evidence and must be regenerated before
they can be replay-verified or promoted under current semantics.

Artifact versions before 9 cannot be promoted in place because retained data may not prove the new
deployment-success and retention-invariance semantics. Keep them as historical evidence and rerun
the comparison from the original dataset, schema, configuration, baseline, and candidate inputs.
Replay refuses unsupported artifact versions rather than guessing.

SQLite metadata upgrades to version 5 when a compatible run store is opened. This metadata update
does not convert an older portable artifact into artifact version 11.

## Local project revision format 1

StructTrace Local projects created before revision format 1 remain visible as legacy projects, but
they are not treated as authoritative committed revisions. Opening and completing a new comparison
creates the first immutable `revisions/<revision-id>` directory and atomically writes
`current-revision.json`. Legacy mutable files are never silently relabelled as verified evidence.

An accepted-baseline record from the earlier standalone-file design is not migrated by inference.
The source release run must still exist, use the current artifact format, authorize deployment,
and pass complete replay before the candidate can be promoted into an accepted revision. This
explicit refusal prevents older partial receipts from acquiring provenance they never stored.

Revision format 1 receipts bind the normalized project configuration, golden data, caller schema,
baseline and candidate bytes, source run, parent revision, and accepted-baseline provenance when
present. Unknown future receipt fields or formats fail closed until a compatible StructTrace
version performs an explicit migration.

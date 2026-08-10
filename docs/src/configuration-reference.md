# Configuration reference

The top-level fields are `version`, `project`, `storage`, `limits`, `dataset`, `schema`, `variants`, `evaluators`, `outcomes`, `analysis`, `gate`, and `report`.

Version `3` is the only accepted configuration version. `variants.baseline` and
`variants.candidate` are required and are the only accepted variant keys. Evaluator IDs
must be unique. Every outcome must define exactly one non-empty `all_of` or `any_of` list, and
every referenced evaluator must exist. `analysis.primary_outcome` must name a configured outcome.

Bootstrap settings are deterministic:

```yaml
analysis:
  primary_outcome: semantic_correct
  bootstrap:
    samples: 10000
    confidence: 0.95
    seed: 17
```

Samples are capped at 1,000,000. At runtime, `samples × evidence units` is capped at
100,000,000 resampling operations before allocation begins.

Unknown keys, unsupported dataset formats, invalid confidence levels, and missing cross-references fail before execution. See `schemas/structtrace.schema.json` for the complete machine-readable shape and the generated `structtrace.yaml` for a working configuration.

Runtime validation enforces the same operational boundaries even when the YAML file is not processed by an editor-side JSON Schema validator. Empty paths and executable names, malformed JSON Pointers or Python callables, zero or excessive timeouts, invalid provider concurrency/retry/token limits, negative prices or tolerances, non-finite gate values, and unsupported report filters are rejected before an adapter is invoked.

Resource limits are configurable within hard safety ceilings:

```yaml
limits:
  max_config_bytes: 1048576
  max_dataset_bytes: 268435456
  max_recorded_output_bytes: 536870912
  max_schema_bytes: 16777216
  max_cases: 10000
  max_jsonl_line_bytes: 16777216
  max_replay_artifact_bytes: 536870912
  max_output_bytes_per_case: 4194304
  max_stderr_bytes_per_process: 1048576
  max_report_raw_bytes_per_case: 262144
  max_report_total_bytes: 268435456
  max_single_file_report_bytes: 10485760
```

Process-log retention is configured separately under `storage.process_logs`. It defaults to
`off`, has a run-wide byte ceiling, and does not alter any score, evidence group, statistic, or gate.

The output limit applies to command, Python, and OpenAI-compatible adapter content. Standard error beyond its retained limit is drained but not stored. The report limit truncates only the shareable HTML view; scored artifacts remain unchanged according to the storage-retention policy. Zero values and values above the compiled hard ceilings fail configuration validation before execution.

Dataset field pointers are required to be disjoint across `id`, `input`, `expected`,
`model_visible_metadata`, and evaluation-only `metadata`. Equal, parent/child, and root overlaps
are rejected before ingestion so configuration cannot accidentally route a golden answer into a
variant request.

Define the independent statistical unit explicitly when the default canonical fingerprint is not
appropriate:

```yaml
dataset:
  path: data/golden.jsonl
  evidence_unit:
    pointer: /metadata/document_id
```

Alternatively use `evidence_unit.include` with normalized-case pointers. The default includes
`/input`, `/expected`, and `/model_visible_metadata`; arbitrary evaluation metadata is excluded so
trace IDs and timestamps cannot manufacture independent evidence.

Latency and cost gate blocks accept `min_coverage` in `[0, 1]`, defaulting to `1.0`. Their
comparisons are computed only from independent evidence units with matched observations for both variants.

OpenAI-compatible `base_url` must be an `http` or `https` API root with a nonempty host. Userinfo,
passwords, query strings, fragments, file URLs, and a complete `/chat/completions` path are rejected.
Credentials are named through `api_key_env` only. The focused profile accepts finite temperatures
from `0` through `2` inclusive.

Command/Python variants and external evaluators may declare provenance inputs:

```yaml
implementation:
  sources: [app.py, rules.json]
  digest: optional-owner-supplied-immutable-digest
```

At most 256 explicit source files are accepted. Sources must be regular non-symlink files and are
subject to per-file and aggregate fingerprint bounds. Unrelated files elsewhere in a repository do
not participate in the run identity. Cross-field rules that JSON Schema cannot safely express,
including JSON Pointer overlap, evaluator/outcome references, and unique evaluator IDs, remain
runtime checks.

Report search indexes case IDs and redacted metadata by default. Opt in specific case fields without
indexing the entire payload:

```yaml
report:
  search_pointers:
    - /metadata/vendor
    - /expected/invoice_number
```

At most 32 valid JSON Pointers are accepted. Values pass through report redaction before indexing,
and each indexed value is bounded.

# Recorded-output format

Recorded comparison is the smallest integration surface. Each JSONL row uses the stable variant envelope:

```json
{
  "case_id": "ticket-001",
  "status": "ok",
  "raw_output": "{\"team\":\"billing\"}",
  "latency_ms": 412,
  "usage": {"input_tokens": 83, "output_tokens": 12},
  "cost": {"amount": "0.000142", "currency": "USD"},
  "metadata": {},
  "retries": []
}
```

`raw_output` is the strict parsing source. A supplied `parsed_output` is a convenience and never hides invalid retained raw text. Duplicate IDs and unknown IDs fail validation. Missing known IDs are materialized as failures so the denominator cannot shrink.

`structtrace compare --dataset ... --baseline ... --candidate ... --schema ...` overrides those four
paths but intentionally still loads the current `structtrace.yaml` for project identity,
evaluators, outcomes, gate, retention, redaction, and limits. It is not a four-file zero-config
command. Run `structtrace init --template recorded` first, then review the generated evaluator and
outcome definitions. Configure `kind: recorded` directly for a repeatable checked-in workflow.

For existing matched artifacts, guided initialization validates and snapshots all sources while
requiring you to declare correctness explicitly:

```bash
structtrace init comparison --from-outputs \
  --dataset data.jsonl --baseline baseline.jsonl --candidate candidate.jsonl \
  --schema schema.json --correctness-pointer /team --gate-mode regression
```

Use `--exact-json` instead of pointers only when complete-object equality really is the application
definition. The command never infers semantics from the schema.

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

Use `structtrace compare` for a one-off comparison or configure `kind: recorded` for repeatable runs.

# Privacy and redaction

StructTrace has no telemetry and no automatic upload path. The report is self-contained, and its server binds only to a random `127.0.0.1` port.

Credentials are read from configured environment variables. Manifests retain the variable name and presence, never the value. Authorization headers and secret values are not logged.

Disable portable raw-output retention and configure shareable-report redaction:

```yaml
storage:
  retain_raw_outputs: false
  redaction:
    json_pointers:
      - /input/customer_email
      - /input/phone
```

Raw retention is enforced before output JSONL and paired case artifacts are finalized. Full
provider-response retention defaults to false. Report redaction fails closed: if a safe report
view cannot be constructed, report creation returns an error rather than falling back to the
original value. Selected values and their echoes in output, evaluator evidence, provider response
bodies, retry records, and filters are replaced with `[REDACTED]`. Disabling raw retention reduces
case-level debugging.

`limits.max_report_raw_bytes_per_case` separately bounds how much retained raw output can be embedded for each variant in the HTML report. Truncation is display-only and is marked explicitly in the case view.

A finalized report belongs to the completed evidence bundle. Opening, serving, or exporting it
never regenerates the report in place.

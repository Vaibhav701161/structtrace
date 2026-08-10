# Privacy and redaction

StructTrace has no telemetry and no automatic upload path. The offline report server binds only to
a random `127.0.0.1` port and requires a fresh 256-bit capability path for every asset. It rejects
foreign Host, Origin, and Referer values and sends no-store, no-sniff, CSP, no-referrer, and
same-origin resource-policy headers.

On Unix, the storage root and run directories are hardened to `0700`; SQLite and finalized artifact
files are hardened to `0600`. `structtrace doctor --strict` checks existing storage permissions.
Replay and report verification reject symlinked manifest artifacts and require canonical targets to
remain beneath the run directory. Windows deployments must apply an equivalent user-only ACL.

Credentials are read from configured environment variables. Manifests retain the variable name and presence, never the value. Authorization headers and secret values are not logged.

Disable portable raw-output retention and configure shareable-report redaction:

```yaml
storage:
  retain_raw_outputs: false
  redaction:
    text_mode: exact_structured
    json_pointers:
      - /input/customer_email
      - /input/phone
    custom_patterns: [tenant-secret-prefix]
```

Raw retention changes persisted presentation and forensic detail only. Stable parse facts and
retention-independent evaluator requests preserve scores, evidence classification, and gates. Full
provider-response retention defaults to false. Report redaction fails closed: if a safe report
view cannot be constructed, report creation returns an error rather than falling back to the
original value. The redaction source includes input, expected values, model-visible metadata, and
evaluation-only metadata. Selected values and their echoes in output, evaluator evidence, provider
response bodies, retry records, search indexes, and filters are replaced with `[REDACTED]`.
Provider HTTP error bodies are never copied into user-facing error messages when response retention
is disabled. Disabling raw retention reduces case-level debugging.

`exact_structured` redacts typed values exactly and replaces distinctive text echoes while avoiding
destructive replacement of every `0`, `1`, `true`, or short number. Select `aggressive_textual` to
replace short scalar substrings too, and use `custom_patterns` for known tenant-specific secret
forms. The aggressive mode can remove innocent text; the exact mode cannot guarantee removal of a
short secret embedded inside a longer sentence. This tradeoff is explicit rather than implied.

`limits.max_report_raw_bytes_per_case` separately bounds how much retained raw output can be embedded for each variant in the HTML report. Truncation is display-only and is marked explicitly in the case view.

A finalized report belongs to the completed evidence bundle. Opening, serving, or exporting it
never regenerates the report in place.

For review outside the trusted run environment, export a deliberately aggregate-only derivative:

```bash
structtrace report latest --export-share structtrace-share
```

The new directory contains summary statistics, gate rules, provenance, and no case inputs,
expected values, outputs, prompts, adapter metadata, or evaluation metadata. It is a derivative,
not a substitute for the hash-bound local evidence bundle.

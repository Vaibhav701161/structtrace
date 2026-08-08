# Security policy

Report security issues privately to the repository owner. Do not include API
keys, private datasets, raw customer outputs, or other sensitive material in a
public issue.

StructTrace is local-first and sends no telemetry. Network traffic occurs only
for model endpoints explicitly configured by the user. Commands are executed as
an executable plus argument array, never through a shell by default.

## Trust boundary

Variant commands, Python callables, and custom evaluators are user-authorized
local code. They run with the permissions of the StructTrace process. Review
configuration from untrusted repositories before invoking `structtrace run`.
StructTrace does not sandbox user executables.

JSON Schema remote retrieval is disabled. The local report server binds to a
random `127.0.0.1` port and does not expose a public listener. Generated reports
contain no external scripts, CDN assets, analytics, or telemetry.

Variant output, provider response envelopes, subprocess standard error, and
report-embedded raw values are bounded. User-configurable limits cannot exceed
compiled hard ceilings. Oversized values fail closed or are explicitly
truncated in the display-only report view; they are never silently scored from
partial text.

## Credentials

OpenAI-compatible credentials are read from the environment variable named by
`api_key_env`. StructTrace persists only the variable name and a presence flag.
It does not persist the credential, bearer header, or complete request headers.

Do not put secret values directly into prompts, model identifiers, command
arguments, or configuration fields. Those fields are evidence and may appear in
the local run bundle.

## Sensitive output

Use `storage.retain_raw_outputs: false` when original output text must not enter
portable artifacts. Configure `storage.redaction.json_pointers` before exporting
or sharing reports. Redaction is a defensive control, not a substitute for
reviewing a report generated from sensitive data.

Run directories are local files and inherit filesystem permissions from the
user and host. Encrypt or delete them according to the organization’s data
retention policy.

## Integrity

Completed artifacts are BLAKE3-bound in the manifest. `structtrace replay`
rejects unsafe manifest paths and reports changed or missing files. Preserve a
corrupt run as evidence and create a new run; do not edit a completed run in
place.

## Reporting a vulnerability

Include the StructTrace version, operating system, minimal reproduction, and
security impact. Remove credentials, proprietary schemas, customer inputs, and
raw model outputs before sending the report.

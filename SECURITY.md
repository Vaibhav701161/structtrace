# Security policy

Report security issues through a [private GitHub security advisory](https://github.com/Vaibhav701161/structtrace/security/advisories/new).
Do not include API
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
random `127.0.0.1` port and serves every verified asset below a fresh 256-bit
capability URL. It validates the loopback Host and rejects foreign Origin and
Referer values. Responses disable caching and MIME sniffing and carry CSP and
same-origin resource headers. Generated reports
contain no external scripts, CDN assets, analytics, or telemetry. Case bodies are
lazy-loaded from bounded local chunks, and an aggregate-only share export omits all case data.

Variant output, provider response envelopes, subprocess standard error, and
report-embedded raw values are bounded. Process logs are off by default. Sanitized retention is
literal and header-pattern based and cannot promise detection of every possible secret; explicit
`full_sensitive` retention may contain credentials or private data. User-configurable limits cannot exceed
compiled hard ceilings. Oversized values fail closed or are explicitly
truncated in the display-only report view; they are never silently scored from
partial text.

Command and Python subprocesses run in bounded process trees where the operating system supports
it. Case timeouts and persistent shutdown deadlines terminate descendants and bound reader joins.
Provider retries and backoff are contained by one total per-case deadline. HTTP error bodies are
not copied into user-facing errors when provider-response retention is disabled.

## Credentials

OpenAI-compatible credentials are read from the environment variable named by
`api_key_env`. StructTrace persists only the variable name and a presence flag.
It does not persist the credential, bearer header, or complete request headers.

If process logs are enabled, use `sanitized` with tenant-specific `custom_patterns` and scan the
final run directory before sharing. Aggregate-only share exports exclude the logs directory. Never
enable `full_sensitive` for a run whose artifacts may leave the trusted host.

Do not put secret values directly into prompts, model identifiers, command
arguments, or configuration fields. Those fields are evidence and may appear in
the local run bundle.

## Sensitive output

Use `storage.retain_raw_outputs: false` when original output text must not enter
portable artifacts. Configure `storage.redaction.json_pointers` before opening a case-level report.
For sharing, prefer `structtrace report latest --export-share <directory>`, which contains
aggregates and provenance but no case-level values. Redaction is a defensive control, not a
substitute for reviewing an artifact generated from sensitive data.

Run directories are local files and inherit filesystem permissions from the
user and host. Encrypt or delete them according to the organization’s data
retention policy.

On Unix, StructTrace creates run directories as `0700` and files as `0600`.
Equivalent restrictive Windows ACL enforcement is not yet verified, so command
and Python execution remain Beta on Windows.

## Integrity

Completed artifacts are BLAKE3-bound in the manifest. `structtrace replay`
rejects unsafe manifest paths and reports changed or missing files. Preserve a
corrupt run as evidence and create a new run; do not edit a completed run in
place.

## Reporting a vulnerability

Include the StructTrace version, operating system, minimal reproduction, and
security impact. Remove credentials, proprietary schemas, customer inputs, and
raw model outputs before sending the report.

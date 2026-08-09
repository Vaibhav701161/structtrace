# Troubleshooting

Run `structtrace doctor --strict` first. It never contacts a provider. Strict mode validates bounded
sources, pointer isolation, semantic duplicates, exact golden-value echoes, Python callable imports,
and Unix storage permissions in addition to the ordinary environment checks.

**Configuration refused:** unknown fields, missing baseline/candidate variants, duplicate evaluator IDs, undefined outcome references, and unsupported versions fail closed. Validate against `schemas/structtrace.schema.json` for editor feedback.

**All command cases failed:** confirm the executable is available from the project root, protocol responses go to stdout, logs go to stderr, and every response repeats the exact case ID and protocol version.

**Python import failed:** run from the project root and verify the configured `module:callable` can be imported by the selected interpreter.

**Provider rows show missing secret:** export the environment variable named in `api_key_env`. Do
not put the secret value in YAML. For an unauthenticated local endpoint, omit `api_key_env`
entirely.

**Gate exits 10:** the run completed correctly and one or more declared thresholds failed. Open the report; this is not a runtime crash.

**Replay reports hash mismatch:** preserve the run directory as evidence and start a new run. Do not edit a completed artifact in place.

**`latest` skips a run:** `latest` intentionally selects the newest completed run. Use
`latest-any` when diagnosing the newest run regardless of lifecycle state.

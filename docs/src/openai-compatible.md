# OpenAI-compatible integration

The focused provider adapter calls `/chat/completions` on an explicitly configured HTTP(S) API
root. Full endpoint paths, embedded credentials, query strings, fragments, and non-HTTP schemes
are rejected during configuration validation. This is a deliberately narrow compatibility
surface, not a claim of universal OpenAI API compatibility.

```yaml
kind: openai_compatible
base_url: http://127.0.0.1:8000/v1
# Optional for unauthenticated local endpoints:
api_key_env: LOCAL_LLM_API_KEY
model: candidate-model
request:
  system: Return only the required object.
  user_template: "{{ input.text }}"
  temperature: 0
  max_output_tokens: 300
structured_output:
  mode: json_schema
  schema: schemas/output.schema.json
retries: 0
```

Omit `api_key_env` for an unauthenticated local endpoint. When configured, the environment
variable name and presence are retained, never its value. Provider errors and malformed partial
responses remain failures. Retries are disabled by default; when enabled, every attempt is
retained and uses bounded exponential backoff, honoring numeric `Retry-After` seconds when
provided. Full provider response retention defaults to false. Pricing is never inferred, because
provider price tables change independently from a run.

For `json_schema`, the exact model-facing schema bytes are size-checked, compiled, captured under
`inputs/variants/<variant>/model-facing-schema.json`, and bound into resume, manifest, and replay
provenance before either variant executes. A schema or configured implementation change during the
run refuses finalization.

| Server profile | Automated evidence | Status |
|---|---|---|
| Local Axum OpenAI-shaped mock | content, malformed/error response, usage, cost, deadline, retry | Tested |
| Unauthenticated local `/v1` root | request construction and response parsing | Tested |
| Hosted OpenAI service | no live compatibility run retained | Experimental / unverified |
| Other OpenAI-compatible providers | no universal compatibility claim | Experimental / unverified |

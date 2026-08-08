# OpenAI-compatible integration

The focused provider adapter calls `/chat/completions` on an explicitly configured base URL. It supports system and rendered user messages, deterministic request settings, `json_object` or `json_schema` response formats, bounded concurrency, token accounting, and user-supplied pricing.

```yaml
kind: openai_compatible
base_url: http://127.0.0.1:8000/v1
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

The environment variable name and presence are retained, never its value. Provider errors and malformed partial responses remain failures. Retries are disabled by default; when enabled, every attempt is retained. Pricing is never inferred, because provider price tables change independently from a run.

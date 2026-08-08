# OpenAI-compatible comparison

This documented example targets a user-controlled compatible endpoint. It is not run in offline CI.

1. Edit `base_url` and both model identifiers in `structtrace.yaml`.
2. Set `LOCAL_LLM_API_KEY` in the environment. For a local server that ignores authentication, use a non-secret placeholder.
3. Run `structtrace run`.

Retries remain disabled. The endpoint receives a deterministic prompt and the external JSON Schema response format. Provider usage is retained; costs appear only if you add explicit current pricing.

# Variant adapters

All execution sources normalize into one output envelope and share the same evaluator pipeline.

| Adapter | Strength | Required runtime |
|---|---|---|
| Recorded | simplest and fully offline | none |
| Command | any implementation language | configured executable |
| Python | minimal wrapper around a callable | Python only for that run |
| OpenAI-compatible | models or request-setting comparison | configured HTTP endpoint |

Command execution never uses a shell. Python is optional and is not required by demos or recorded comparisons. Provider retries default to zero and occur only when explicitly configured. In every mode, timeouts, process crashes, malformed responses, and missing results remain scored failures.

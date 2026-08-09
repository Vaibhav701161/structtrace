# Variant adapters

All execution sources normalize into one output envelope and share the same evaluator pipeline.

| Adapter | Status | Strength | Required runtime |
|---|---|---|---|
| Recorded | Stable | simplest and fully offline | none |
| Command | Beta | any implementation language | configured executable |
| Python | Beta | minimal wrapper around a callable | Python only for that run |
| OpenAI-compatible | Experimental | models or request-setting comparison | configured HTTP endpoint |

These labels describe current validation depth, not a promise of API permanence. Command execution
never uses a shell. Command and Python process trees, shutdown, output readers, and case deadlines
are bounded. Python is optional and is not required by demos or recorded comparisons. Provider
retries default to zero, occur only when configured, and remain inside one total case deadline. In
every mode, timeouts, crashes, malformed responses, and missing results remain denominator failures.

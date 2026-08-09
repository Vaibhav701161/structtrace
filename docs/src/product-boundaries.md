# Product boundaries

StructTrace measures and gates structured-output migrations. It does not automatically rewrite schemas, optimize prompts, choose a decoding backend, select a model, repair failed JSON for primary scoring, or guarantee that a candidate is better.

The central workflow is deterministic and does not require an LLM judge. Remote provider execution occurs only when the user explicitly configures and runs an OpenAI-compatible variant.

Multi-turn agent orchestration, parallel tool calls, web search, memory management, hosted dashboards, telemetry, and automatic artifact upload are outside the current product boundary. Keeping those concerns separate prevents unrelated failures from contaminating paired structured-output evidence.

The self-contained report embeds every case for offline inspection. Very large evaluations do not
yet use chunked or lazy-loaded case payloads; use the machine-readable artifacts for bulk analysis.

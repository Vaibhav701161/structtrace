# Product boundaries

StructTrace measures and gates changes to structured extraction systems while the caller-facing
schema remains fixed. It does not migrate or rewrite schemas, optimize prompts, choose a decoding
backend, select a model, execute tool calls, repair failed JSON for primary scoring, or guarantee
that a candidate is better.

The central workflow is deterministic and does not require an LLM judge. Remote provider execution occurs only when the user explicitly configures and runs an OpenAI-compatible variant.

Multi-turn agent orchestration, parallel tool calls, web search, memory management, hosted dashboards, telemetry, and automatic artifact upload are outside the current product boundary. Keeping those concerns separate prevents unrelated failures from contaminating paired structured-output evidence.

Reports are bounded offline bundles. The summary page loads a redacted search index and fetches
50-case JSON chunks only when a filtered 25-case page needs them. A self-contained HTML derivative
is retained only when it stays below `limits.max_single_file_report_bytes`; otherwise export
refuses and the report directory is the supported artifact.

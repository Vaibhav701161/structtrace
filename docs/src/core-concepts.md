# Core concepts

**Case**: one immutable ID with an input, optional expected result, and optional metadata.

**Variant**: the baseline or candidate implementation. It may be a recorded JSONL file, a command, a Python callable, or an OpenAI-compatible endpoint.

**Evaluator**: one deterministic fact about a parsed output, such as an exact JSON Pointer match or numeric tolerance check.

**Outcome**: an explicit `all_of` or `any_of` composition of evaluator facts. One outcome is selected as primary.

**Complete denominator**: every known dataset case contributes to both variant totals. Errors, timeouts, invalid JSON, schema failures, and missing output rows count as failures rather than disappearing.

**Paired transition**: for each case, both pass, baseline only passes, candidate only passes, or both fail.

**Release gate**: independent user-declared limits on semantic regression, valid-but-wrong growth, schema validity, errors, timeouts, latency, and cost.

**Replay**: hash verification plus full recomputation from retained artifacts.

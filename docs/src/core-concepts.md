# Core concepts

**Case**: one immutable ID with an input, optional expected result, optional evaluation-only
metadata, and optional explicitly model-visible metadata. Expected results and evaluation-only
metadata stay inside the evaluation engine.

**Variant**: the baseline or candidate implementation. It may be a recorded JSONL file, a command, a Python callable, or an OpenAI-compatible endpoint.

**Evaluator**: one deterministic fact about a parsed output, such as an exact JSON Pointer match or numeric tolerance check.

**Outcome**: an explicit `all_of` or `any_of` composition of evaluator facts. One outcome is selected as primary.

**Complete denominator**: every known dataset case contributes to both variant totals. Errors, timeouts, invalid JSON, schema failures, and missing output rows count as failures rather than disappearing.

**Paired transition**: for each case, both pass, baseline only passes, candidate only passes, or both fail.

**Release gate**: independent user-declared limits on semantic regression, valid-but-wrong growth, schema validity, errors, timeouts, latency, and cost.

**Replay**: hash verification, cross-artifact reconstruction from independently retained dataset
and variant inputs, full built-in recomputation, and hash-bound verification of external
evaluator receipts without re-executing side-effecting programs.

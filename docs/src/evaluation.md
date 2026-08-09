# Evaluation

Evaluation is deterministic and complete-denominator by default. For each case and variant, StructTrace records adapter status, strict parse status, all schema errors, each evaluator result, every composed outcome, the primary binary result, and valid-but-wrong classification.

Evaluator results use four states: `passed`, `failed`, `error`, and `not_applicable`. Errors are never silently treated as passes. An outcome containing a required evaluator error becomes an error and contributes a failure to the primary binary metric.

Schema validity cannot be selected as an inferred semantic outcome. The user must explicitly encode correctness with evaluators appropriate to the application.

JSON Schema `format` assertions are enabled. A schema using `{"format":"date"}` rejects
impossible or non-ISO dates such as `2026-02-30` and `09/08/2026`; use the `canonical_date`
evaluator when the application deliberately accepts and canonicalizes additional source formats.

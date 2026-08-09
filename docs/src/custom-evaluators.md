# Custom evaluators (experimental)

Custom command and Python evaluators support deterministic executable checks that cannot be expressed as pointer comparisons. This execution path is experimental and is not yet the recommended large-dataset path. Each evaluator receives a versioned request containing the case, expected value, complete model-output envelope, evaluator ID, and variant metadata.

The response uses `passed`, `failed`, `error`, or `not_applicable`, with an optional zero-to-one score, message, and structured details. Commands execute without a shell. Persistent workers receive one request and emit one identity-matched response at a time under the configured per-case timeout. Python callables may return a boolean or response dictionary through the bundled evaluator bridge. Malformed responses, crashes, timeouts, incompatible identities, extra output, and lifecycle failures become evaluator errors and remain failures in the denominator.

Persistent workers are the default and reuse one process per evaluator and variant. Set
`process_mode: per_case` only for legacy evaluators that intentionally exit after one request. Every
external evaluator must declare an immutable `implementation_version`; that value is included in
the hash-bound replay definition. Treat a source change as a version change. Persistent responses
must echo the request's `evaluator_id` and `case_id`; mismatched identities fail closed.

Evaluator stderr is capped and retained separately. Replay recomputes built-in scoring and outcome composition from the retained external-evaluator receipt; it does not re-execute potentially side-effecting evaluator code.

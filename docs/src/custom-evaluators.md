# Custom evaluators

Custom command and Python evaluators support deterministic executable checks that cannot be expressed as pointer comparisons. Each evaluator receives a versioned request containing the case, expected value, complete model-output envelope, evaluator ID, and variant metadata.

The response uses `passed`, `failed`, `error`, or `not_applicable`, with an optional zero-to-one score, message, and structured details. Commands execute without a shell, one invocation per case and variant, under the configured timeout. Python callables may return a boolean or response dictionary through the bundled evaluator bridge. Malformed responses, crashes, timeouts, and incompatible protocol identity become evaluator errors and remain failures in the denominator.

Evaluator stderr is capped and retained separately. Replay recomputes built-in scoring and outcome composition from the retained external-evaluator receipt; it does not re-execute potentially side-effecting evaluator code.

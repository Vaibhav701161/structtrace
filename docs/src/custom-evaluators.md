# Custom evaluators (experimental)

Custom command and Python evaluators support deterministic executable checks that cannot be expressed as pointer comparisons. This execution path is experimental and is not yet the recommended large-dataset path. Each evaluator receives a versioned request containing the case, expected value, complete model-output envelope, evaluator ID, and variant metadata.

The response uses `passed`, `failed`, `error`, or `not_applicable`, with an optional zero-to-one
diagnostic score, message, structured details, and validated `fields` array. The four-state
`status` is authoritative for outcomes and gates; an undeclared score never changes pass/fail.
`error` and `not_applicable` responses must use a null or omitted score. Each field fact carries a JSON
Pointer, four-state status, expected/actual values, and message; it is included in the hash-bound
receipt and field-hotspot report. Commands execute without a shell. Persistent workers receive one
request and emit one identity-matched response at a time under the configured per-case timeout.
Python evaluators support synchronous and async callables. Malformed responses, crashes, timeouts,
incompatible identities, extra output, and lifecycle failures become evaluator errors.

Persistent workers are the only evaluator process mode accepted by the stable runtime and reuse one
process per evaluator and variant. `process_mode: per_case` is refused until its process-tree and
reader-shutdown guarantees match the persistent runtime on every supported OS. Every
external evaluator must declare an immutable `implementation_version`; that value is included in
the hash-bound replay definition. Declare every source file that can alter evaluator behavior so the
run fingerprint and replay receipt bind the implementation itself:

```yaml
implementation_version: rules-v3
implementation:
  sources: [evaluators/rules.py, evaluators/policy.json]
  digest: optional-owner-supplied-immutable-digest
```

Treat a source change as a version change. Declared sources are regular non-symlink files, are read
under explicit size limits, and are re-fingerprinted before finalization. Persistent responses
must echo the request's `evaluator_id` and `case_id`; mismatched identities fail closed.

Command and Python execution remain beta on Windows until descendant termination is backed by Job
Objects and exercised by real Windows process-tree tests.

Evaluator stderr is capped and retained separately. Replay recomputes built-in scoring and outcome composition from the retained external-evaluator receipt; it does not re-execute potentially side-effecting evaluator code.

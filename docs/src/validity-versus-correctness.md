# Validity versus correctness

StructTrace evaluates three separate boundaries:

1. Strict parsing asks whether the complete output is exactly one JSON value.
2. JSON Schema asks whether that value satisfies the caller-facing structural contract.
3. Evaluators and outcomes ask whether the value is correct for the case.

An output such as `{"priority":"low"}` can be valid JSON and schema-valid while being wrong for an urgent outage ticket. Conversely, malformed output cannot pass semantic scoring even if a recoverable JSON fragment appears inside prose. StructTrace does not use heuristic extraction to convert a strict parse failure into a pass.

This separation also applies to tool calls. Correct tool selection, exact argument semantics, reconstructed external validity, actual execution success, and correct post-execution state are distinct facts. A schema-valid call is not assumed executable-correct.

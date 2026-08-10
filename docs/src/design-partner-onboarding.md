# Design-partner onboarding

Status: **PENDING first partner**.

Use a non-production copy of an existing extraction evaluation. Begin with retained recorded outputs,
not provider credentials. Confirm the caller-facing schema, matched case IDs, expected values, and
deterministic evaluator semantics with the workload owner before running a gate.

The partner should complete `init`, `doctor --strict`, `run`, report review, replay verification, and
gate interpretation. Maintainers may observe but should not take over the keyboard during an
“unaided” attempt. Export only the aggregate share derivative unless case-level sharing is separately
approved.

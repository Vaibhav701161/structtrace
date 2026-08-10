# Configuration validation corpus

`config-valid` and `config-invalid` must produce the same accept/reject decision in the shipped
editor JSON Schema and Rust runtime. `config-runtime-invalid` contains a deliberately tiny set of
cross-field constraints that standard JSON Schema cannot express reliably; the editor may accept
their shape, but the runtime must reject them before any adapter executes.

The current runtime-only case is parent/child overlap between dataset routing pointers. Evaluator
and outcome reference integrity is also enforced at runtime because it depends on values across
separate arrays and maps.

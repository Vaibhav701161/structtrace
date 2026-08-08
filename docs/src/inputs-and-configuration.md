# Inputs and configuration

A StructTrace project binds four kinds of input: the versioned configuration, exact dataset bytes, external JSON Schema bytes, and baseline/candidate definitions or output artifacts. Relative paths resolve from `--project-root`.

Configuration may be YAML or JSON. Unknown fields fail validation. The repository ships [a JSON Schema](../../schemas/structtrace.schema.json) for editor completion, while the Rust configuration model remains the executable source of truth.

Provider credentials are referenced only by environment-variable name. StructTrace never interpolates secret values into retained configuration or the run manifest.

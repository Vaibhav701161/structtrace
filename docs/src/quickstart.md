# Quickstart

Install from the repository with stable Rust 1.87 or newer:

```bash
cargo install --path crates/structtrace-cli --locked
structtrace --help
structtrace doctor
```

Create and run an offline recorded-output project:

```bash
structtrace init my-check --template recorded
cd my-check
structtrace run
structtrace report latest --open
structtrace gate latest
structtrace replay latest
```

The generated candidate deliberately regresses one of two cases, so the run completes successfully but the gate exits with code `10`. That is different from malformed input or an execution failure.

Inspect `data/golden.jsonl`, `schemas/output.schema.json`, both files in `outputs/`, and `structtrace.yaml`. Replace the fixture with your matched cases and configure deterministic evaluators that represent correctness for your application.

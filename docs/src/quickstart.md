# Quickstart

After a binary release is published, install on macOS or Linux without Rust:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/Vaibhav701161/structtrace/main/install.sh | sh
```

On Windows PowerShell:

```powershell
irm https://raw.githubusercontent.com/Vaibhav701161/structtrace/main/install.ps1 | iex
```

Until the first binary release, contributors can install from source with stable Rust 1.87 or
newer:

```bash
cargo install --path crates/structtrace-cli --locked
structtrace --help
structtrace doctor --strict
structtrace doctor --strict --dry-run 3
```

The bounded dry run executes only configured local command, Python, and custom-evaluator
handshakes. It never contacts an OpenAI-compatible endpoint.

Create and run an offline recorded-output project:

```bash
structtrace init my-check --template recorded
cd my-check
structtrace run
structtrace report latest --open
structtrace gate latest
structtrace replay latest
```

For a production-shaped extraction starting point, use:

```bash
structtrace init my-check --preset extraction
```

The generated candidate deliberately regresses one of two cases, so the run completes successfully but the gate exits with code `10`. That is different from malformed input or an execution failure.

Inspect `data/golden.jsonl`, `schemas/output.schema.json`, both files in `outputs/`, and `structtrace.yaml`. Replace the fixture with your matched cases and configure deterministic evaluators that represent correctness for your application.

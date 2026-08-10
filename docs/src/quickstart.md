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
git clone https://github.com/Vaibhav701161/structtrace.git
cd structtrace
cargo install --path crates/structtrace-cli --locked
structtrace --help
structtrace doctor
```

After creating a project, `structtrace doctor --strict` performs bounded static validation only.
Use `--handshake` to resolve Python callables without business cases. Use `--execute-cases N` only
when you deliberately want configured local code to run; it may have network or other side effects.
Doctor never contacts an OpenAI-compatible endpoint.

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

The extraction preset contains 12 matched invoices. Both variants pass 9/12 with six discordant
cases; baseline and candidate schema validity are 10/12 and 12/12. The gate is
`INSUFFICIENT EVIDENCE` because the fixture does not meet its 100-case evidence floor.

The generic recorded template is separate: it contains two cases and is only a wiring check.

Inspect `data/golden.jsonl`, `schemas/output.schema.json`, both files in `outputs/`, and `structtrace.yaml`. Replace the fixture with your matched cases and configure deterministic evaluators that represent correctness for your application.

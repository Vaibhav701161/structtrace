# Setup and first run

Target length: 6 minutes.

1. Show `rustc --version` and install with `cargo install --path crates/structtrace-cli --locked`.
2. Run `structtrace --help` and `structtrace doctor`.
3. Create `structtrace init first-check --template recorded`.
4. Walk through the generated dataset, schema, recorded outputs, evaluator, outcome, and gate.
5. Run `structtrace run`.
6. Explain why the command succeeds even though the quality gate fails.
7. Run `structtrace gate latest`; show exit code 10.
8. Run `structtrace replay latest` and show zero mismatches.

Keep the demo offline. No credentials or external services should appear on screen.

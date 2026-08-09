# Recorded-output comparison

No Python, model, provider, or network is required.

```bash
cd examples/recorded-output-comparison
cargo run --manifest-path ../../Cargo.toml -p structtrace-cli -- --project-root . run
```

The candidate is schema-valid on both cases but misclassifies one. The report exposes one candidate
valid-but-wrong row; the two-case fixture is `INSUFFICIENT EVIDENCE`, not a deployment decision.

# Recorded-output comparison

No Python, model, provider, or network is required.

```bash
cd examples/recorded-output-comparison
cargo run --manifest-path ../../Cargo.toml -p structtrace-cli -- --project-root . run
```

The candidate is schema-valid on both cases but misclassifies one. The report therefore exposes one candidate valid-but-wrong row and the gate fails.

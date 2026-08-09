# Recorded-output comparison

No Python, model, provider, or network is required.

```bash
cd examples/recorded-output-comparison
cargo run --manifest-path ../../Cargo.toml -p structtrace-cli -- --project-root . run
```

The candidate is schema-valid on both cases but misclassifies one. The report exposes one candidate
valid-but-wrong row. Its headline is `FAILED` because a configured quality rule is violated; the
individual evidence rule still says that two cases cannot authorize deployment.

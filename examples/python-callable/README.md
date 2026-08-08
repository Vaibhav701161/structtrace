# Python-callable comparison

This example wraps two ordinary Python functions. The baseline classifies both cases correctly; the candidate introduces one regression.

```bash
cd examples/python-callable
cargo run --manifest-path ../../Cargo.toml -p structtrace-cli -- --project-root . run
```

Expected behavior: 2/2 baseline versus 1/2 candidate semantic correctness, a failed release gate, and a complete offline report under `.structtrace/runs/`.

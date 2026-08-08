# Contributing

Before opening a change, run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Changes to artifact formats, protocols, metrics, or gate semantics require a
changelog entry, migration or compatibility handling, and replay tests.

# Language-agnostic command comparison

The example adapter is Python for readability, but the stdin/stdout protocol is language-neutral and no shell is used.

```bash
cd examples/command
cargo run --manifest-path ../../Cargo.toml -p structtrace-cli -- --project-root . run
```

Expected behavior: the persistent process returns one matching response per case; the candidate regression fails the configured gate.

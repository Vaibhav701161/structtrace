# Tool-call evaluation

This offline example scores tool selection, exact arguments, and a deterministic execution receipt. The receipt represents the post-execution state produced by a side-effect-free local wrapper.

```bash
cd examples/tool-calling
cargo run --manifest-path ../../Cargo.toml -p structtrace-cli -- --project-root . run
```

The candidate remains schema-valid but applies the wrong signed inventory delta for one case. Tool name, argument semantics, execution success, and post-state are scored as separate evaluator facts.

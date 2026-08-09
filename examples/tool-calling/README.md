# Tool selection and argument regression

This offline example scores the selected tool and exact argument semantics. It does not execute a
tool and does not treat a model-generated receipt as execution evidence.

```bash
cd examples/tool-calling
cargo run --manifest-path ../../Cargo.toml -p structtrace-cli -- --project-root . run
```

The candidate remains schema-valid but emits the wrong signed inventory delta for one case. Teams
that need execution verification should add a deterministic custom evaluator which dispatches into
their own side-effect-free test double and returns an independently generated result.

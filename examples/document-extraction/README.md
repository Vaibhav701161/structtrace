# Document extraction

This offline fixture compares invoice extraction outputs. It demonstrates required fields, exact-decimal numeric tolerance, schema validity, and a candidate value that is structurally valid but financially wrong.

```bash
cd examples/document-extraction
cargo run --manifest-path ../../Cargo.toml -p structtrace-cli -- --project-root . run
```

Expected behavior: the candidate fixes one missing required field but changes one total beyond tolerance. Inspect both transitions rather than relying on marginal schema validity.

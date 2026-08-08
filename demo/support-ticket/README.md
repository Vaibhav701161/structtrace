# Support-ticket routing demo

This deterministic offline demo shows why structural validity and semantic
correctness must be measured separately.

The candidate produces schema-valid JSON on every case. It is also faster in the
recorded metadata. However, it introduces more wrong routing decisions than it
repairs, so the release gate fails.

From the repository root:

```bash
cargo run -p structtrace-cli -- \
  --project-root demo/support-ticket \
  run
```

Then inspect the generated `report/index.html` or run the gate:

```bash
cargo run -p structtrace-cli -- \
  --project-root demo/support-ticket \
  gate latest
```

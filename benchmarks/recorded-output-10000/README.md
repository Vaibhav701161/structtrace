# Recorded-output 10,000-case benchmark

This benchmark measures the conservative v1 default envelope with deterministic recorded JSONL,
strict parsing, schema validation, one built-in evaluator, paired analysis, chunked report creation,
and complete replay.

```bash
cargo build --workspace --release --locked
python3 scripts/measure-recorded-scale.py \
  --cases 10000 \
  --output benchmarks/recorded-output-10000/result.json
```

`result.json` is accepted only when it records the release-binary digest, source sizes, artifact
size, source commit and clean-worktree state, lockfile digest, commands, exit codes, wall time,
platform, and `passed: true`. It does not establish the 100,000-case hard ceiling; that ceiling
remains an explicit opt-in boundary pending measurement.

## Recorded result

Commit `3cb2121fb7b78fad4cca6ca8de011952d3eafe7b` was measured from a clean worktree on
Linux x86-64 under WSL2 with Rust 1.87.0:

| Operation | Wall time | Peak RSS |
|---|---:|---:|
| Complete 10,000-case run | 148.96 s | 326,208 KiB |
| Complete replay | 2.27 s | 287,744 KiB |

The three source artifacts total 2,478,890 bytes and the completed run directory contains
72,632,417 bytes. These are descriptive measurements on one machine, not cross-platform latency
guarantees. See `result.json` for exact commands and digests.

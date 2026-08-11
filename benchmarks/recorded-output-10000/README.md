# Recorded-output 10,000-case benchmark

This benchmark measures the conservative v1 default envelope with deterministic recorded JSONL,
strict parsing, schema validation, one built-in evaluator, paired analysis, chunked report creation,
complete replay, cheap history listing, and cold/warm indexed case search through the local API.

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

Release tags also run `.github/workflows/scale-validation.yml` and upload the receipt generated
from that exact source revision. The checked-in receipt is historical evidence, while the CI
artifact is the authority for a tagged release.

## Recorded result

Commit `0669ae9aea789804e0e18a7ffdc1905fb3cb5546` was measured from a clean worktree on
Linux x86-64 under WSL2 with Rust 1.87.0:

| Operation | Wall time | Peak RSS |
|---|---:|---:|
| Complete 10,000-case run | 135.54 s | 326,584 KiB |
| Complete replay | 2.12 s | 288,544 KiB |

The three source artifacts total 2,478,890 bytes and the completed run directory contains
72,632,417 bytes. These are descriptive measurements on one machine, not cross-platform latency
guarantees. See `result.json` for exact commands and digests.

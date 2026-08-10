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

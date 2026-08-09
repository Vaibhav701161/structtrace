# Persistent external evaluator scale check

This checked-in regression test exercises 1,000 matched cases, two variants, and one Python
evaluator. It therefore produces 2,000 evaluator requests and receipts while starting only one
persistent Python worker per variant.

Run it with:

```bash
cargo test -p structtrace-engine persistent_python_evaluator_handles_1000_cases_per_variant -- --nocapture
```

The acceptance conditions are exact: 1,000 baseline passes, 1,000 candidate passes, 1,000 jointly
scored pairs, and 2,000 retained receipts. The test is part of the normal suite so worker lifecycle,
protocol, storage, report generation, and receipt creation remain covered together.

## Recorded run

On 2026-08-09 the test completed the full run in 20.348 seconds on four logical CPUs of an Intel
Core i5-12450H under WSL2, using Rust 1.87.0 and Python 3.12.3. This number is descriptive, not a
cross-machine performance claim. The correctness assertions and bounded worker count are the
release evidence.

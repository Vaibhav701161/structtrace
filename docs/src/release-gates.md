# Release gates

Release rules are independent and explicit. Supported limits cover primary-outcome regression, valid-but-wrong growth, minimum candidate schema validity, maximum error and timeout rates, p95 latency increase, and average cost increase.

```bash
structtrace gate latest
structtrace gate latest --format json
structtrace gate latest --format github
structtrace gate latest --verify replay
```

Exit code `0` means the completed run passed all configured rules. Exit code `10` means the run completed but a quality threshold failed. Input, runtime, artifact, and protocol failures use distinct exit codes.

By default, the gate verifies the manifest-bound `summary.json` hash before applying the stored
decision. `--verify replay` requires complete artifact reconstruction and score replay first.
Local hashes establish integrity and consistency, not cryptographic authorship.

Latency and cost comparisons use only matched cases where both variants have a measurement. Each
operational rule also has `min_coverage`, which defaults to `1.0`. Insufficient coverage fails
closed. Raw observation counts, matched counts, and matched aggregates remain visible. An
unconfigured rule is clearly labelled not evaluated. StructTrace never uses a latency regression
to hide a correctness improvement or a schema improvement to hide a semantic regression.

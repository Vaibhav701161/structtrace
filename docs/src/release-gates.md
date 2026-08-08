# Release gates

Release rules are independent and explicit. Supported limits cover primary-outcome regression, valid-but-wrong growth, minimum candidate schema validity, maximum error and timeout rates, p95 latency increase, and average cost increase.

```bash
structtrace gate latest
structtrace gate latest --format json
structtrace gate latest --format github
```

Exit code `0` means the completed run passed all configured rules. Exit code `10` means the run completed but a quality threshold failed. Input, runtime, artifact, and protocol failures use distinct exit codes.

An operational rule configured without sufficient measurements fails closed. An unconfigured rule is clearly labelled not evaluated. StructTrace never uses a latency regression to hide a correctness improvement or a schema improvement to hide a semantic regression.

# Running the offline demo

The demos are compiled into the StructTrace binary and make no network requests.

```bash
structtrace demo invoice --open
structtrace demo support-ticket --open
structtrace demo research --open
```

The default invoice demo is a deterministic 120-case release scenario. It compares matched invoice extraction
outputs, surfaces exact field-level improvements and regressions, and produces a `FAILED` headline
when a configured quality rule is violated. Its 120 matched cases satisfy the configured minimum
evidence threshold.

The support-ticket demo contains twelve matched routing cases. The candidate improves strict JSON
and schema validity from 11/12 to 12/12 while semantic correctness falls from 10/12 to 8/12. Its
valid-but-wrong count grows from one to four. The gate reports `FAILED` because the quality
regression takes precedence; an individual rule also reports that 12 cases are insufficient.

The research demo reproduces three accepted paired matrices from the Contract Sensitivity Lab record. The corrected Qwen estimate is positive, while the canonical Llama and practical tool-call estimates are negative. This is evidence that contract-sensitive effects must be measured on the actual workload, not a claim that one representation always wins.

The generated report remains under `.structtrace/runs/<run-id>/report/index.html` after the loopback server is closed.

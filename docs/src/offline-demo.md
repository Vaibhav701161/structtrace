# Running the offline demo

The demos are compiled into the StructTrace binary and make no network requests.

```bash
structtrace demo invoice --open
structtrace demo support-ticket --open
structtrace demo research --open
```

The invoice demo uses twelve genuinely different fixture invoices. Both variants pass 9/12, with
three baseline-only and three candidate-only transitions. It demonstrates nested field diagnosis,
valid-but-wrong cases, and gate behavior, but its headline is `INSUFFICIENT EVIDENCE`. It is a
workflow demonstration, not a release scenario. StructTrace never repeats these rows to satisfy a
minimum evidence threshold.

The support-ticket demo contains twelve matched routing cases. The candidate improves strict JSON
and schema validity from 11/12 to 12/12 while semantic correctness falls from 10/12 to 8/12. Its
valid-but-wrong count grows from one to four. The gate reports `INSUFFICIENT EVIDENCE`; observed
quality failures remain visible as rules but cannot be promoted into a release verdict from twelve
cases.

The research command reproduces three accepted paired matrices as three separate runs and writes a
non-inferential index. The corrected Qwen estimate is positive, while the canonical Llama and
practical tool-call estimates are negative. No pooled effect or release gate is calculated.

Demo and research manifests are explicitly typed and never replace the default latest production
run. Use `latest-demo` or `latest-research` to select them. The generated report remains under
`.structtrace/runs/<run-id>/report/index.html` after the loopback server is closed.

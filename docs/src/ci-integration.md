# CI integration

Run the candidate, then gate the most recent completed result:

```yaml
- name: Run paired structured-output regression
  run: structtrace run --format github

- name: Enforce release thresholds
  run: structtrace gate latest --format github
```

The GitHub format emits annotations for failed rules and a compact Markdown-compatible table. Preserve `.structtrace/runs/` as a CI artifact when reviewers need the offline report and replay bundle.

Do not convert exit code `10` to success unless the workflow intentionally treats release regressions as advisory. Keep provider credentials in the CI secret store and reference only their environment-variable names in `structtrace.yaml`.

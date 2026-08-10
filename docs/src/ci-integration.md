# CI integration

The repository CI runs locked formatting, warnings-denied Clippy, the complete all-feature test
suite, release builds across Linux/macOS/Windows, mdBook, dependency advisories, dependency and
license policy, CodeQL, and short strict-JSON/protocol fuzz smoke jobs. A defined remote job is not
local evidence; its status must be read from the corresponding commit checks.

Run the candidate, then gate the most recent completed result:

```yaml
- name: Run paired structured-output regression
  run: structtrace run --format github

- name: Enforce release thresholds
  run: structtrace --format github release-check latest
```

`release-check` always performs complete replay and returns zero only when the stored gate is in
Release mode, passed, and explicitly authorizes deployment. Ordinary `structtrace gate` remains an
analysis command and must not guard a deployment step. The GitHub format emits annotations for
failed rules and a compact Markdown-compatible table. Preserve `.structtrace/runs/` as a CI
artifact when reviewers need the offline report and replay bundle.

Do not convert exit code `10` to success unless the workflow intentionally treats release regressions as advisory. Keep provider credentials in the CI secret store and reference only their environment-variable names in `structtrace.yaml`.

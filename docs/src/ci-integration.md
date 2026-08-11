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

The Local UI exports CI only from a verified committed project revision. The snapshot includes the
revision receipt and, when present, the accepted-baseline receipt; exported baseline bytes must
match the accepted candidate digest before any workflow is written. Source checkout is pinned only
when the running binary embeds a usable 40-character Git commit. Archive or development builds
without that provenance refuse export with an actionable error instead of emitting a placeholder
checkout ref.

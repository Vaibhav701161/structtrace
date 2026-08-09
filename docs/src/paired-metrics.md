# Paired metrics

Because baseline and candidate run on the same IDs, the important unit is the case-level transition:

| Baseline | Candidate | Category |
|---|---|---|
| pass | pass | both pass |
| pass | fail | baseline-only pass, a regression |
| fail | pass | candidate-only pass, an improvement |
| fail | fail | both fail |

The deployment-success effect is candidate pass rate minus baseline pass rate in percentage points.
Before inference, StructTrace groups rows using `dataset.evidence_unit`. The default fingerprint
contains input, expected output, and model-visible metadata, but excludes arbitrary evaluation
metadata such as timestamps and trace IDs. Users can instead declare one grouping pointer or an
explicit pointer include-list.

Repeated rows remain visible in descriptive execution totals. When repeated observations in one
evidence unit disagree in status or scored evidence, the group becomes conflicting repeated
evidence, is not arbitrarily collapsed, and forces an `INSUFFICIENT EVIDENCE` gate. Non-conflicting
groups contribute once. The primary cards, transition matrix, bootstrap, evaluator table, hotspots,
and release gate therefore share the same named evidence-unit population.

The report also computes a jointly scored semantic effect. It includes only independent pairs where
both primary outcomes explicitly resolve to true or false and lists operational/error exclusions by
reason. The complete-denominator deployment result remains the release-gate metric; an adapter crash
is unsuccessful operationally but is not described as proof that the answer was semantically wrong.

Do not pool effects across unrelated models, tasks, or experimental protocols. Compare each paired study on its own stated workload.

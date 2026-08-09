# Paired metrics

Because baseline and candidate run on the same IDs, the important unit is the case-level transition:

| Baseline | Candidate | Category |
|---|---|---|
| pass | pass | both pass |
| pass | fail | baseline-only pass, a regression |
| fail | pass | candidate-only pass, an improvement |
| fail | fail | both fail |

The deployment-success effect is candidate pass rate minus baseline pass rate in percentage points.
Before inference, StructTrace fingerprints each case from input, expected output, model-visible
metadata, and evaluation-only metadata. Exact duplicate rows remain descriptive, but one
representative per fingerprint forms the independent matrix, McNemar test, bootstrap, and gate
denominator. The four matrix cells therefore sum to the unique semantic-case denominator.

The report also computes a jointly scored semantic effect. It includes only independent pairs where
both primary outcomes explicitly resolve to true or false and lists operational/error exclusions by
reason. The complete-denominator deployment result remains the release-gate metric; an adapter crash
is unsuccessful operationally but is not described as proof that the answer was semantically wrong.

Do not pool effects across unrelated models, tasks, or experimental protocols. Compare each paired study on its own stated workload.

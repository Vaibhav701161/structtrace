# Paired metrics

Because baseline and candidate run on the same IDs, the important unit is the case-level transition:

| Baseline | Candidate | Category |
|---|---|---|
| pass | pass | both pass |
| pass | fail | baseline-only pass, a regression |
| fail | pass | candidate-only pass, an improvement |
| fail | fail | both fail |

The headline effect is candidate pass rate minus baseline pass rate in percentage points. Candidate-only and baseline-only counts show whether equal marginal percentages conceal different case behavior. The four cells always sum to the complete dataset denominator.

Do not pool effects across unrelated models, tasks, or experimental protocols. Compare each paired study on its own stated workload.

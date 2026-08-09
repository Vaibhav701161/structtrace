# Why StructTrace exists

Structured extraction changes are often evaluated with a single metric: schema-valid output rate.
That metric is necessary, but it is not sufficient. A candidate can emit cleaner JSON while
choosing the wrong enum, changing an amount, omitting a business-critical relationship, or calling
the correct tool with the wrong argument.

StructTrace treats a stable-contract system change as a matched experiment. Baseline and candidate
receive the same immutable cases. Deterministic evaluators score the business meaning. The report
separates:

- adapter success;
- strict JSON parsing;
- external-schema validity;
- semantic or executable correctness;
- valid-but-wrong outputs;
- latency, retries, token usage, and user-priced cost.

This prevents an improvement in one layer from concealing a regression in another. It also preserves uncertainty: the report shows case-level transition counts, an exact paired test, and a seeded paired bootstrap interval rather than presenting a marginal percentage as universal proof.

# Understanding McNemar and bootstrap intervals

The exact McNemar test uses only discordant independent evidence units: candidate-only and baseline-only passes. StructTrace computes the two-sided exact binomial probability, which remains valid with small discordant counts.

The paired bootstrap resamples complete, non-conflicting evidence-unit pairs with replacement. Its seed, sample count, confidence level, and evidence-unit definition are retained, so replay produces the same interval. Repeated rows remain in descriptive totals, but they cannot multiply the inferential denominator. If observations within one evidence unit disagree, StructTrace excludes that unit from inference and refuses to issue a release verdict.

When no independent deployment pairs exist, the paired effect and interval are absent. The report
and Local UI render **Not estimable** and **Not available**, never `0.0 pp` or `[0.0, 0.0]`. The
same rule applies to semantic-only inference when no pair has fully evaluated binary primary
outcomes. Zero is an estimated value; missing evidence is not.

Neither quantity turns a benchmark into a universal law. The interval describes uncertainty from the observed paired case set under the configured resampling procedure. It does not capture changes in prompt, provider implementation, sampling policy, model revision, or workload distribution. A positive point estimate whose interval crosses zero should be described as uncertain, not proven.

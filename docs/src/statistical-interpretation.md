# Understanding McNemar and bootstrap intervals

The exact McNemar test uses only discordant independent evidence units: candidate-only and baseline-only passes. StructTrace computes the two-sided exact binomial probability, which remains valid with small discordant counts.

The paired bootstrap resamples complete, non-conflicting evidence-unit pairs with replacement. Its seed, sample count, confidence level, and evidence-unit definition are retained, so replay produces the same interval. Repeated rows remain in descriptive totals, but they cannot multiply the inferential denominator. If observations within one evidence unit disagree, StructTrace excludes that unit from inference and refuses to issue a release verdict.

Neither quantity turns a benchmark into a universal law. The interval describes uncertainty from the observed paired case set under the configured resampling procedure. It does not capture changes in prompt, provider implementation, sampling policy, model revision, or workload distribution. A positive point estimate whose interval crosses zero should be described as uncertain, not proven.

# Understanding McNemar and bootstrap intervals

The exact McNemar test uses only discordant pairs: candidate-only and baseline-only passes. StructTrace computes the two-sided exact binomial probability, which remains valid with small discordant counts.

The paired bootstrap resamples complete case pairs with replacement. Its seed, sample count, and confidence level are retained in the manifest, so replay produces the same interval.

Neither quantity turns a benchmark into a universal law. The interval describes uncertainty from the observed paired case set under the configured resampling procedure. It does not capture changes in prompt, provider implementation, sampling policy, model revision, or workload distribution. A positive point estimate whose interval crosses zero should be described as uncertain, not proven.

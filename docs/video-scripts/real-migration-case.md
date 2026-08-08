# Real migration case

Target length: 8 minutes. Prepare matched recorded outputs from a real structured-output migration, with sensitive fields removed or configured for report redaction.

1. State the exact baseline and candidate change without predicting a winner.
2. Show that case IDs and dataset bytes are frozen.
3. Explain the application-specific primary outcome.
4. Run the comparison and inspect schema-validity and correctness separately.
5. Review every baseline-only regression before discussing aggregate effect.
6. Show latency and cost only as descriptive unless thresholds were preregistered.
7. Export the offline report and replay it.

Do not tune the dataset, evaluator, or gate after viewing candidate failures without starting and disclosing a new run.

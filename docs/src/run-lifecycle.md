# Run lifecycle

A run moves through explicit states: `created`, `validating`, `running`, `interrupted`, `analyzing`, `complete`, `failed`, or `corrupt`. An interrupted run is never labelled complete.

If an ordinary validation, adapter-preparation, analysis, report, or finalization error occurs after a run is allocated, a lifecycle guard records `run_failed` and marks the SQLite run `failed`. A process kill cannot execute cleanup code, so a killed run retains its last durable non-complete state and remains eligible for hash-locked resume.

Validation occurs before adapter invocation: configuration cross-references, dataset IDs, exact schema bytes, and schema compilation are checked first. Execution outputs are checkpointed atomically. Analysis writes paired case records and summaries, generates the report, checkpoints SQLite WAL state, hashes final artifacts, and only then marks the run complete.

Version 1 executes the complete baseline variant before the complete candidate variant and records
that fixed order. It does not yet offer interleaved or seeded within-pair provider scheduling. For
long hosted runs where temporal provider drift is plausible, keep the run window short or use
recorded outputs collected under a separately controlled schedule.

Completed runs are treated as immutable evidence. Start a new run for changed experimental inputs.

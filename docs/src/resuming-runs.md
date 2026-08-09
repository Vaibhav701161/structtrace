# Resuming runs

Resume an interrupted run with its ULID:

```bash
structtrace run --resume 01K...
```

StructTrace compares the current configuration file hash, normalized configuration hash, dataset
hash, schema hash, evaluator/outcome/gate definition hash, variant definitions, artifact-format
version, local entry-source hashes, dependency lockfile hashes, Git commit, and dirty-tree
fingerprint with the execution checkpoint. Any difference refuses resume and requires a new run.

Completed baseline or candidate outputs are hash-checked and reused rather than invoked again. If interruption occurred during final analysis, derived database rows are transactionally reset and recomputed from retained completed outputs. Completed and corrupt runs cannot be resumed.

The current checkpoint boundary is a complete variant. An interrupted in-progress provider,
per-case command, or Python variant is rerun as a unit; case-level paid-call resume is not yet
implemented. Persistent command mode intentionally retains this variant-level behavior.

# CI integration

Target length: 5 minutes.

1. Show a workflow that installs the locked StructTrace binary.
2. Run `structtrace run --format github` against a deterministic recorded fixture.
3. Run `structtrace gate latest --format github`.
4. Show the failed annotation and distinguish gate exit 10 from runtime failures.
5. Upload `.structtrace/runs/` as a workflow artifact.
6. Download and open the self-contained report locally.
7. Run replay to verify the downloaded evidence bundle.

Never print provider secrets or authorization headers in the recording.

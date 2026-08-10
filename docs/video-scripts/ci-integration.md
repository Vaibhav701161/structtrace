# CI integration

Target length: 5 minutes.

1. Show a workflow that installs the locked StructTrace binary.
2. Run `structtrace run --format github` against a deterministic recorded fixture.
3. Run `structtrace --format github release-check latest` and explain that only an authorized
   Release-mode decision returns zero.
4. Show the gate annotation and distinguish quality failure 10 and insufficient evidence 12 from runtime failures.
5. Upload `.structtrace/runs/` as a workflow artifact.
6. Download the complete offline report directory and open it through the loopback report command.
7. Run replay to verify the downloaded evidence bundle.

Never print provider secrets or authorization headers in the recording.

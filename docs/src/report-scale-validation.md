# Report scale validation

Report format 4 separates the aggregate shell from case data:

```text
report/index.html
report/case-index.json
report/cases/00000.json
report/cases/00001.json
...
```

The shell contains no case bodies. The redacted index supports case-ID and metadata search plus
outcome/evidence filters. A page renders at most 25 cases and loads data from 50-case chunks. All
assets remain local and are bound into the completed run manifest.

## Automated 1,000-case acceptance

`thousand_case_report_is_chunked_searchable_and_bounded` builds 1,000 paired invoice records with
nested line items, financial fields, document text, and metadata. It asserts:

- exactly 1,000 redacted index entries;
- exactly 20 case chunks;
- no case body or final case ID in `index.html`;
- an aggregate shell below 512 KiB;
- a complete report bundle below 16 MiB for this fixture;
- no single-file derivative when the configured one-file ceiling is exceeded;
- generation completes inside a conservative 30-second test ceiling.

On the local Linux validation host, the isolated test completed in 1.76 seconds. `/usr/bin/time -v`
reported 90,428 KiB maximum resident memory for the complete Rust test process, including the test
harness. These measurements are descriptive and not cross-platform performance promises.

## Supported envelope

One thousand paired cases with similarly sized nested outputs is the currently measured report
envelope. Per-case raw display values, the optional single-file derivative, and the whole report
directory each have independently configurable limits with compiled hard ceilings. The generator
streams one bounded case at a time into 50-case chunks and writes a temporary report directory
that is atomically renamed only after size and hash checks pass. Larger or much more verbose
workloads still require validation in their intended environment; machine-readable run artifacts
remain available for bulk analysis.

Clean-browser open time and interactive filter latency have not yet been recorded across the
release operating-system matrix. That evidence belongs to release-candidate VM and external-user
validation and must not be inferred from the generator benchmark.

## Complete 10,000-case recorded workflow

`scripts/measure-recorded-scale.py` deterministically generates 10,000 matched cases and measures
the release binary through strict ingestion, schema validation, paired scoring, SQLite/artifact and
report creation, followed by complete replay. The checked-in result records source sizes, generated
artifact bytes, binary digest, command exit codes, wall time, and peak RSS when `/usr/bin/time` is
available.

```bash
cargo build --workspace --release --locked
python3 scripts/measure-recorded-scale.py \
  --cases 10000 \
  --output benchmarks/recorded-output-10000/result.json
```

The default `limits.max_cases` is therefore 10,000. The compiled 100,000-case ceiling is an
explicit opt-in boundary, not a measured promise.

The clean-tree Linux x86-64 measurement for commit `0cd51fe` completed the full 10,000-case run in
153.08 seconds with 325,640 KiB peak RSS. Complete replay took 2.16 seconds with 288,124 KiB peak
RSS. The three input sources totalled 2,478,890 bytes and the finalized run artifacts totalled
72,182,337 bytes. These figures describe one WSL2 host; they are not cross-platform promises.

## Input-memory boundary

The v1 dataset and recorded-output readers enforce byte, line, and case ceilings before parsing and
avoid redundant source rereads during guided import. They still retain each complete bounded source
artifact in memory to bind the exact executed bytes into the run. Memory therefore scales with the
configured dataset and output byte ceilings. StructTrace does not claim streaming ingestion or
million-row capacity; larger sources require measurement in the intended environment or lower
source-byte limits.

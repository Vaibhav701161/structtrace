# Report scale validation

Report format 2 separates the aggregate shell from case data:

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

One thousand paired cases with similarly sized nested outputs is the currently tested report
envelope. Per-case raw display values, the optional single-file derivative, and the whole report
directory each have independently configurable limits with compiled hard ceilings. Larger or much
more verbose workloads must be validated against their intended environment before relying on the
interactive report; machine-readable run artifacts remain available for bulk analysis.

Clean-browser open time and interactive filter latency have not yet been recorded across the
release operating-system matrix. That evidence belongs to release-candidate VM and external-user
validation and must not be inferred from the generator benchmark.

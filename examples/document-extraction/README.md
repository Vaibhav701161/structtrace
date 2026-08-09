# Invoice extraction migration

This is StructTrace's primary stable-contract extraction workflow. It compares two retained
implementations on the same 12 invoices and the same caller-facing schema. Deterministic evaluators
score invoice identity, vendor, date, currency, exact-decimal financial fields, line items, and
required-field completeness.

```bash
cd examples/document-extraction
cargo run --manifest-path ../../Cargo.toml -p structtrace-cli -- --project-root . run
```

The candidate fixes missing currencies and one vendor name, but introduces a tax error, a total
error, and a missing line item. All candidate rows remain strict JSON; the financially wrong rows
are visible as valid-but-wrong. The gate reports `FAILED` because the quality regression is real;
an individual evidence rule also reports that 12 cases cannot authorize a release.

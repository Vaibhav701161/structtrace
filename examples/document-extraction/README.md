# Invoice extraction migration

This is StructTrace's primary stable-contract extraction workflow. It compares two retained
implementations on the same 12 invoices and the same caller-facing schema. Deterministic evaluators
score invoice identity, vendor, date, currency, exact-decimal financial fields, line items, and
required-field completeness.

```bash
structtrace doctor --strict
structtrace run
structtrace report latest --open
structtrace gate latest
structtrace replay latest
```

The candidate fixes missing currencies and one vendor name, but introduces a tax error, a total
error, and a missing line item. All candidate rows remain strict JSON; the financially wrong rows
are visible as valid-but-wrong. Both variants score 9/12 on the primary outcome, with six
discordant cases. Baseline schema validity is 10/12 and candidate schema validity is 12/12. The
gate reports `INSUFFICIENT EVIDENCE` because 12 cases cannot satisfy the configured 100-case floor.

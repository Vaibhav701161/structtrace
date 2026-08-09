# Run storage

Local state defaults to `.structtrace`. Each ULID-named run contains a versioned SQLite database and portable files. SQLite tables store runs, cases, variants, outputs, evaluator results, outcomes, paired results, events, and artifact hashes.

Portable JSON and JSONL files make reports and replay inspectable without querying SQLite. Exact
configuration source bytes are retained alongside the normalized configuration. Dataset and
configuration bytes are captured once before execution and carried through finalization, so a
source-file edit during a long run cannot change its evidence bundle. Baseline and candidate input
JSONL are retained independently of derived `cases.jsonl`. Custom evaluators add hash-bound
`external-evaluator-receipts.jsonl`.

Final writes are atomic. BLAKE3 hashes bind exact dataset, schema, imported/generated outputs, and
finalized artifacts. WAL is used during active execution and checkpointed on completion.

The storage root is configurable. Run IDs and manifest artifact paths are validated against path traversal before they are opened.

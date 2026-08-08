# Run storage

Local state defaults to `.structtrace`. Each ULID-named run contains a versioned SQLite database and portable files. SQLite tables store runs, cases, variants, outputs, evaluator results, outcomes, paired results, events, and artifact hashes.

Portable JSON and JSONL files make reports and replay inspectable without querying SQLite. Final writes are atomic. BLAKE3 hashes bind exact dataset, schema, imported/generated outputs, and finalized artifacts. WAL is used during active execution and checkpointed on completion.

The storage root is configurable. Run IDs and manifest artifact paths are validated against path traversal before they are opened.

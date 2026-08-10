# Run storage

Use the explicit management surface instead of editing `.structtrace/runs` by hand:

```bash
structtrace runs list
structtrace runs show <run-id>
structtrace runs latest --kind production
structtrace runs archive <run-id> <destination>
structtrace runs delete <run-id> --yes
```

Deletion refuses active lifecycle states, validates that the target is an immediate child of the
resolved storage root, and refuses any symlink in the run tree. Without `--yes`, confirmation is
required. Archive first verifies every manifest-bound artifact, copies only the manifest allowlist
plus `manifest.json`, rejects symlinks, excludes unbound regular files, and writes
`archive-verification.json` with a BLAKE3 hash for every copied file. On Unix, archive directories
and files are created with owner-only permissions. A verified archive is complete evidence, not a
share-safe derivative, and may contain retained inputs or raw outputs. Use `report --export-share`
when only aggregate public material is required.

Local state defaults to `.structtrace`. Each ULID-named run contains a versioned SQLite database and portable files. SQLite tables store runs, cases, variants, outputs, evaluator results, outcomes, paired results, events, and artifact hashes.

Portable JSON and JSONL files make reports and replay inspectable without querying SQLite. Exact
configuration source bytes are retained alongside the normalized configuration. Dataset and
configuration bytes are captured once before execution and carried through finalization, so a
source-file edit during a long run cannot change its evidence bundle. Baseline and candidate input
JSONL are retained independently of derived `cases.jsonl`. Custom evaluators add hash-bound
`external-evaluator-receipts.jsonl`.

Manifests and SQLite record a run kind: `production`, `demo`, `research_fixture`, or `test`.
`latest` selects only a completed production run; use `latest-demo`, `latest-research`, or
`latest-any` explicitly for other histories.

Live runs snapshot model-facing schemas before either variant executes. Configured executables,
Python entry modules, evaluator implementations, declared source files/digests, relevant lockfiles,
and interpreter identity form a bounded fingerprint that is rechecked before finalization.

Final writes are atomic. BLAKE3 hashes bind exact dataset, schema, imported/generated outputs, and
finalized artifacts. WAL is used during active execution and checkpointed on completion.

The storage root is configurable. Run IDs and manifest artifact paths are validated against path traversal before they are opened.

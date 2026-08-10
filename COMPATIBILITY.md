# Compatibility policy

StructTrace uses explicit versions for configuration, portable artifacts, reports, SQLite metadata,
and subprocess protocols. A newer binary refuses formats it cannot interpret safely. It never
silently applies new scoring semantics to an old completed run.

The current compatibility surface is:

| Surface | Version | Policy |
|---|---:|---|
| Configuration | 3 | Only the current version is accepted; migration is documented |
| Portable artifact | 9 | Older semantic formats require rerunning original inputs |
| Report data | 4 | Regenerated only as part of a verified current-format run |
| SQLite metadata | 5 | Compatible metadata migrations are automatic |
| Command/evaluator protocol | 3 | Exact version match required |

Platform support is established only by packaged-binary CI and clean-machine evidence. Until the
first signed release is published, source builds on Linux are the locally verified path; macOS and
Windows remain CI-verified or pending as recorded in `ACCEPTANCE.md`.

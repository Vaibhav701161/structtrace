# Release hardening audit

This page maps the trust-invariant review to the current implementation. It distinguishes software
fixes from platform and external-user evidence that cannot be inferred from a local test suite.

## Closed software blockers

| Review blocker | Implemented invariant | Verification |
|---|---|---|
| Retention changed scoring | Immutable analysis view precedes retention; raw-disabled rows retain strict parse receipts | Eight retention combinations produce identical summary and gate; duplicate-key output remains invariant |
| Paired coverage used marginal minimums | Coverage is the per-case intersection of fully evaluated primary outcomes | Disjoint one-row failures in 100 cases produce 98%, not 99% |
| Schema parsing differed by command | External and model-facing schemas use the recursive duplicate-key-rejecting parser | Direct compare, configured run, doctor, and replay share strict schema semantics |
| Semantic pass was called deployment success | Structural, semantic, and deployment success are separate versioned facts | Schema-invalid, parse-invalid, adapter-error, and incomplete rows cannot be deployment successes |
| Release thresholds could be vacuous | Release mode requires the safe absolute and relative profile | One-case, zero-floor, 100%-allowance profiles are rejected during validation and strict doctor |
| Non-authorizing pass looked deployable | Gate mode and authorization are explicit; CI has an authorization-only flag | Advisory/regression never authorize; `--require-release-authorization` fails closed |
| Candidate-omitted fields disappeared in setup | Pointer union uses schema, expected, baseline, and candidate evidence | A field absent from every candidate row remains selectable |
| Verified archives copied unbound files | Verified archives copy only manifest-bound artifacts plus the manifest | Unbound debug/secret files are absent and symlinks are rejected |

The same change also binds effective compare overrides, uses run-scoped per-row adapter tokens, pins
all GitHub Actions to immutable commits, publishes migration/support/compatibility/deprecation
policies, and uploads commit-bound acceptance evidence from CI.

## Version boundary

| Format | Current version | Reason |
|---|---:|---|
| Configuration | 3 | Safe release profile and deployment rules |
| Portable artifact | 10 | Explicit success types, schema provenance, and typed deployment/semantic transitions |
| Report data | 4 | Deployment and semantic denominators are separate |
| SQLite metadata | 5 | Binds stored evaluation JSON to versioned artifact semantics |
| Command/evaluator protocol | 3 | Unchanged in this hardening pass |

Artifact versions before 10 are historical evidence. They must be rerun from original inputs and
are never silently upgraded to the new decision semantics.

## Still requires non-local evidence

The repository does not claim public-launch completion for clean packaged installs on every target,
real Chromium/Firefox/WebKit/Edge and assistive-technology validation, external private-alpha
workflows, or a real deployment decision. These are recorded as release gates in `ROADMAP.md` and
`ACCEPTANCE.md`; documentation cannot substitute for observing them.

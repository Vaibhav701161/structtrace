# StructTrace Local browser workflow

StructTrace Local is the primary human interface for recorded-output comparisons. It is served by
the same Rust binary that runs the CLI evidence engine:

```bash
structtrace open
```

The command binds a random IPv4 loopback port, creates a random 256-bit capability URL, opens the
default browser, and prints the URL as a fallback. No account, cloud service, provider credential,
telemetry, CDN, or Node.js runtime is involved. The server stops on Ctrl-C or after 30 minutes of
inactivity when no comparison is active.

## Complete recorded-output path

The browser guides a comparison through six explicit stages:

1. Add golden data, baseline outputs, candidate outputs, and an optional caller-facing schema.
2. Confirm case ID, input, expected, and output mappings with real sample records.
3. Select deterministic correctness rules from the union of expected, baseline, and candidate
   fields. Candidate omissions stay visible.
4. Choose Advisory, Regression, or Release authority and inspect the evidence requirement.
5. Review names, matched rows, selected semantics, and retained artifacts.
6. Run the existing Rust initializer, evaluator, paired analysis, gate, report, and artifact
   verification path.

JSON, JSONL, and ordinary CSV sources are accepted. The Rust server is the authoritative parser
for readiness, row counts, strict duplicate-key rejection, CSV semantics, and preview values; the
browser never declares a source ready from a more permissive client parser. Browser uploads are content-only: the API does
not accept arbitrary local paths. Each source is staged exactly once under an opaque ULID and
BLAKE3 digest. Draft autosaves contain only that reference plus mappings and policy; source bytes
are reloaded from the staged store only after their digest is verified. The source screen exposes
save failures and a destructive, explicit clear action. Dataset and recorded-output sources are
capped at the same 32 MiB default used by the CLI; schemas are capped at the same 16 MiB default.
Each source is staged in its own request under a 64 MiB transport ceiling. Larger custom CLI limits
are an advanced opt-in and are not claimed as browser-product capacity.
Duplicate IDs, label leakage, malformed lines, missing outputs, insufficient evidence, and evaluator
failures remain visible and fail closed.

If no schema is supplied, StructTrace derives a closed structural shape from the first expected
value and labels the resulting metric **Schema valid (inferred shape)**. This is diagnostic only:
Release mode is unavailable and runtime validation rejects any release configuration whose schema
provenance is not explicitly `caller_supplied`.

## Decision language

The local product deliberately uses distinct result states:

- **RELEASE AUTHORIZED** only when a Release gate sets `deployment_authorized`.
- **DO NOT DEPLOY** when a configured quality rule fails.
- **REGRESSION CHECK PASSED** with an explicit statement that it is not release authorization.
- **NOT ENOUGH EVIDENCE** when evidence safeguards are not met.
- **ANALYSIS COMPLETE** when no deployment authority was requested.
- **RUN ERROR** when a required result cannot be evaluated safely.

The result screen presents complete-denominator structural, semantic, deployment, and
valid-but-wrong metrics; paired transitions; the paired interval; primary field hotspots; and the
independent-evidence audit. Every aggregate leads to immutable case evidence.

Case evidence is queried from the Rust server in 200-row pages and rendered through a virtualized
table. A request can return no more than 500 cases, filters and search execute on the server, and
the complete case artifact remains subject to the existing 64 MiB evidence bound. This avoids
placing an entire large run in either an API response or the browser DOM.

The case drawer uses JSON Pointer-addressed structural rows, distinguishes added, removed, changed,
and type-changed values, aligns common ID/SKU/product-code arrays before comparison, offers unified
and side-by-side modes, collapses unchanged paths, and copies exact pointers. Error,
not-applicable, and unscored evaluator states remain distinct from a semantic “wrong” result.

Recorded evaluation executes through a durable local job. The browser polls real engine
checkpoints, shows the current phase and completed work units, preserves an event history, recovers
the active job after reload, and requests cancellation through a shared atomic control checked at
safe source and case boundaries. Cancelled, failed, or server-interrupted jobs can be resumed from
their retained source references as a new auditable job; no decision is produced for an incomplete
job.

## Security boundary

Every page, asset, and `/api/v1` endpoint requires the per-process capability path. The server
rejects foreign Host, Origin, and Referer values. Responses are `no-store` and carry a restrictive
Content Security Policy, frame denial, referrer denial, and MIME-sniffing protection. Static assets
are compiled into the Rust binary and have no runtime network dependency.

Draft references, staged sources, and browser-created projects are retained beneath
`.structtrace/ui/`. Re-running a retained draft updates that stable project definition atomically
while preserving every prior immutable run. CLI projects opened with
`structtrace --project-root <folder> open` are discovered alongside UI-created projects. Completed
evidence continues to use the same hash-bound portable artifacts and replay model as the CLI.
The Projects screen reopens the saved wizard policy, persists name changes, duplicates a project
under a new identity, and moves archived projects to `.structtrace/ui/archived-projects/` rather
than deleting them. A new comparison always receives a new project identity.

Saved cases are local bookmarks and are deliberately not described as a regression suite. The CI
screen exports a complete runnable snapshot of the saved project: full configuration and evaluator
definitions, golden/baseline/candidate sources, caller schema, commit-pinned StructTrace install,
authority-safe command, required-input checks, evidence upload, and integration instructions. The
application-specific candidate generation step remains caller-owned and is named explicitly rather
than guessed. Candidate-baseline promotion appears
only for an authorizing Release decision. It copies the immutable candidate input into a separately
hash-bound staged source, records the accepted run ID and candidate artifact hash, and prepares the
same persistent project for its next candidate. Advisory, regression, insufficient-evidence, and
failed decisions cannot promote a baseline; there is no unrecorded override path.

## Frontend development

Node.js is build-time only:

```bash
cd ui
npm ci
npm audit --audit-level=high
npm run check
npm run build
cd ..
./scripts/test-local-ui-e2e.sh --project=chromium
```

Release CI verifies the checked-in embedded build matches the TypeScript source, exercises direct
result refresh and asset MIME types, and runs light/dark and responsive WCAG A/AA browser tests
across Chromium, Firefox, and WebKit. The local UI does not change the status of live
command, Python, or provider adapters; recorded outputs remain the stable private-alpha path.

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

JSON, JSONL, and ordinary CSV sources are accepted. Browser uploads are content-only: the API does
not accept arbitrary local paths. Each file is capped at 32 MiB and the complete request at 64 MiB.
Duplicate IDs, label leakage, malformed lines, missing outputs, insufficient evidence, and evaluator
failures remain visible and fail closed.

If no schema is supplied, StructTrace derives a closed structural shape from the first expected
value and labels the resulting metric **Schema valid (inferred shape)**. This is not presented as
validation against a caller-owned contract.

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

## Security boundary

Every page, asset, and `/api/v1` endpoint requires the per-process capability path. The server
rejects foreign Host, Origin, and Referer values. Responses are `no-store` and carry a restrictive
Content Security Policy, frame denial, referrer denial, and MIME-sniffing protection. Static assets
are compiled into the Rust binary and have no runtime network dependency.

Drafts and completed projects are retained beneath `.structtrace/ui/`. The browser keeps only
ephemeral wizard state and presentation preferences. Completed evidence continues to use the same
hash-bound portable artifacts and replay model as the CLI.

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

Release CI verifies the checked-in embedded build matches the TypeScript source and runs WCAG A/AA
browser tests across Chromium, Firefox, and WebKit. The local UI does not change the status of live
command, Python, or provider adapters; recorded outputs remain the stable private-alpha path.

# Public deployment baseline

Recorded before the public website implementation on 2026-08-11.

## Product architecture

StructTrace is a Rust workspace with one evaluation path shared by recorded outputs, command
processes, Python callables, and the experimental OpenAI-compatible adapter. The primary human
interface is a React application compiled into `ui/dist` and embedded into the
`structtrace` executable. `structtrace open` starts an Axum server on a random loopback port,
creates a fresh capability URL, and opens the system browser unless `--no-browser` is supplied.

The engine is intentionally local. The public website must not implement a second evaluator or
receive user evaluation data.

| Layer | Current implementation |
|---|---|
| CLI and local server | `crates/structtrace-cli` |
| Configuration, strict JSON, scoring, statistics, gates | `crates/structtrace-core` |
| Execution, storage, replay, retained artifacts | `crates/structtrace-engine` |
| Command, Python, OpenAI-compatible adapters | `crates/structtrace-adapters` |
| Offline report generation | `crates/structtrace-report` |
| Local browser product | React 19 and TypeScript under `ui/` |
| Documentation | mdBook sources under `docs/src/` |

## Current functionality

- strict whole-output JSON parsing with duplicate-key rejection;
- external JSON Schema validation with remote retrieval disabled;
- deterministic extraction, classification, numeric, date, keyed-array, and tool evaluators;
- complete-denominator outcomes, valid-but-wrong accounting, paired transitions, exact McNemar,
  seeded paired bootstrap intervals, and evidence-unit conflict handling;
- advisory, regression, and release gates with distinct insufficient-evidence and error states;
- immutable completed runs, SQLite history, BLAKE3-bound portable artifacts, replay, archives,
  aggregate-only sharing, saved regression cases, and baseline promotion;
- capability-protected loopback UI with no account, telemetry, CDN, or cloud storage;
- Stable-candidate recorded-output workflow, Beta command and Python workflows, and an
  Experimental OpenAI-compatible workflow.

## Baseline evidence

The source baseline is commit `b730609d6f73e8d92e9b23fc6f92e1d834a1ae77` on `main`.
The exact-source GitHub CI run completed the Rust, UI, browser, docs, dependency, fuzz-smoke,
research-provenance, cross-platform, and acceptance jobs. CodeQL and the 10,000-case scale workflow
also completed for that source.

The local acceptance command is:

```bash
./scripts/record-local-acceptance.sh target/deployment-baseline-acceptance.json
```

It records command status, logs, hashes, toolchain, platform, source commit, and dirty-source state
outside version control. A previously completed clean exact-source scale receipt records 10,000
cases, 20,000 variant outputs, zero replay mismatches, a 173.333 second run, and 333,568 KiB peak
RSS on Linux/WSL2. This is a workload measurement, not a universal performance claim.

The baseline receipt completed successfully. It recorded exit code 0 for Rustfmt, warnings-denied
Clippy, all-feature workspace tests, the npm vulnerability policy, frontend type and unit checks,
the deterministic embedded frontend build, the release workspace build, final-binary UI smoke,
Chromium and Firefox end-to-end accessibility flows, mdBook, and the release CLI surface. The
receipt correctly records a dirty worktree because the private knowledge-transfer ignore entry was
already present; its source-tree digest binds the exact audited state rather than representing it
as the clean baseline commit.

## Release and platform state

The repository already has a tag-triggered release workflow for:

- Linux x86_64 musl;
- macOS x86_64;
- macOS Apple Silicon;
- Windows x86_64 MSVC.

Each target builds from locked source, runs workspace and frontend checks, packages the final
binary, writes SHA-256 checksums, generates an SPDX SBOM, validates the extracted archive against
the exact source and embedded UI, and requests a GitHub build-provenance attestation. Shell and
PowerShell installers verify the selected release asset checksum.

No public versioned release existed at baseline. Clean-machine installation and independent-user
evidence therefore remained public-release gates rather than completed claims.

## Deployment gap

There was no public product website, zero-install guided demo, domain deployment, release download
surface, or website-specific CI. The `structtrace.tech` domain was still using the registrar's
default Orderbox nameservers. The implementation that follows adds the discovery and distribution
layer without changing the local evaluation engine.

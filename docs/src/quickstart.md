# Quickstart

After a binary release is published, download the installer from the same immutable release tag,
inspect it, and install that specific version. Replace `vX.Y.Z` with the release you selected:

```bash
curl -fsSLO https://github.com/Vaibhav701161/structtrace/releases/download/vX.Y.Z/install.sh
sh install.sh --version vX.Y.Z
```

On Windows PowerShell:

```powershell
Invoke-WebRequest https://github.com/Vaibhav701161/structtrace/releases/download/vX.Y.Z/install.ps1 -OutFile install.ps1
.\install.ps1 -Version vX.Y.Z
```

Both installers verify the archive checksum. When GitHub CLI is available, they also verify the
GitHub build-provenance attestation. Set `STRUCTTRACE_REQUIRE_ATTESTATION=1` to fail rather than
fall back to checksum-only verification. Use `--uninstall` on Unix or `-Uninstall` on Windows;
rerun with a newer explicit tag to update.

Until the first binary release, contributors can install from source with stable Rust 1.87 or
newer:

```bash
git clone https://github.com/Vaibhav701161/structtrace.git
cd structtrace
cargo install --path crates/structtrace-cli --locked
structtrace --help
structtrace doctor
```

After creating a project, `structtrace doctor --strict` performs bounded static validation only.
Use `--handshake` to resolve Python callables without passing business cases. The handshake imports
configured Python modules, so import-time code executes. Use `--execute-cases N` only when you
deliberately want configured local code to run; it may have network or other side effects.
Doctor never contacts an OpenAI-compatible endpoint.

Create and run an offline recorded-output project:

```bash
structtrace init my-check --template recorded
cd my-check
structtrace run
structtrace report latest --open
structtrace gate latest
structtrace replay latest
```

For a production-shaped extraction starting point, use:

```bash
structtrace init my-check --preset extraction
```

The extraction preset contains 12 matched invoices. Both variants pass 9/12 with six discordant
cases; baseline and candidate schema validity are 10/12 and 12/12. The gate is
`INSUFFICIENT EVIDENCE` because the fixture does not meet its 100-case evidence floor.

The generic recorded template is separate: it contains two cases and is only a wiring check.

Inspect `data/golden.jsonl`, `schemas/output.schema.json`, both files in `outputs/`, and `structtrace.yaml`. Replace the fixture with your matched cases and configure deterministic evaluators that represent correctness for your application.

## Import an existing extraction comparison

`init --from-outputs` accepts both StructTrace envelopes and ordinary JSONL such as
`{"id":"invoice-1","output":{...}}`. In a terminal, omitted paths and correctness semantics are
prompted interactively. In automation, provide them explicitly:

```bash
structtrace init invoice-check --from-outputs \
  --dataset invoices.jsonl \
  --baseline baseline.jsonl \
  --candidate candidate.jsonl \
  --schema invoice.schema.json \
  --dataset-id-pointer /document_id \
  --dataset-input-pointer /payload \
  --dataset-expected-pointer /ground_truth \
  --output-id-pointer /record_id \
  --output-value-pointer /result \
  --field-evaluator /vendor_name=normalized_string \
  --field-evaluator /invoice_date=canonical_date:iso,dmy_slash \
  --field-evaluator /total=decimal_exact \
  --keyed-array '/line_items=/sku;/description:normalized_string,/quantity:exact_integer,/amount:decimal_tolerance:0.01' \
  --financial-invariants
```

The generated `ONBOARDING.md` reports field presence across expected, baseline, and candidate data.
A field missing from every candidate output remains visible. Suggested evaluator types are never
silently enabled; semantic choices must be confirmed interactively or supplied as flags.

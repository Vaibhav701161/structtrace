# Research foundation and product lineage

StructTrace grew directly from the artifact-backed
[Constrained Sensitivity Lab](https://github.com/Vaibhav701161/constrained-senstivity-lab).
That repository treats the model-facing contract as an experimental variable and keeps the original
caller-facing contract fixed. It records frozen protocols, raw generations, validators, paired
statistics, mechanism audits, and decision reports. StructTrace turns the conclusions into a
general local evaluation product; it does not rewrite the research result into a product claim.

## Evidence chain

The accepted evidence found a positive but uncertain Qwen estimate for an internal integer
representation, followed by a negative canonical replication on Llama and a negative point
estimate in a small executable tool-call pilot.

The bundled research command creates one normalized run per study and a fourth non-inferential
index. It preserves these exact paired counts:

| Study | Baseline correct | Candidate correct | Candidate-only | Baseline-only |
|---|---:|---:|---:|---:|
| Corrected Qwen | 18/49 | 24/49 | 9 | 3 |
| Canonical Llama | 92/150 | 82/150 | 6 | 16 |
| Tool-call pilot | 26/30 | 24/30 | 1 | 3 |

These are separate studies, not a pooled estimate or model leaderboard. No top-level effect,
bootstrap, or release gate is calculated across them. The engineering conclusion is narrower and
more useful: representation and contract changes can alter semantic behavior, and their direction
may not generalize. Measure them on the actual target workload.

## How the evidence became product requirements

```text
Frozen model experiments
        |
        v
Paired repairs, regressions, validity, and execution outcomes
        |
        v
Negative cross-family and practical replication
        |
        v
Requirement: measure contract sensitivity, never assume optimization
        |
        v
StructTrace matched runner, deterministic evaluators, replay, and release gates
```

| Research boundary | StructTrace implementation |
|---|---|
| Structural validity is not task correctness | Separate parse, schema, semantic, executable, and deployment states |
| Paired cases reveal changes hidden by marginal accuracy | Case-ID joins and a paired transition matrix |
| One model-family gain did not generalize | No automatic schema rewrite or universal optimization claim |
| Reference and execution defects must remain visible | Complete denominators, explicit evaluator errors, and multi-state gates |
| Evidence changed during corrections | Immutable inputs, manifests, artifact hashes, and deterministic replay |
| Practical adoption needs recurring checks | Accepted baselines, pinned regression cases, and project-bound CI export |

The machine-readable bridge is
[`provenance/research-foundation.json`](https://github.com/Vaibhav701161/structtrace/blob/main/provenance/research-foundation.json). It pins the lab
source revision and the SHA-256 digest of each accepted paired summary used by the bundled research
demonstration.

## What is synchronized and what is not

The repositories are synchronized at the evidence and design boundary:

- StructTrace links the exact accepted research artifacts and pins their hashes.
- The research repository documents why the compiler thesis was narrowed into an evaluation tool.
- `structtrace demo research` reproduces the accepted transition matrices as normalized offline
  product fixtures.

The demo is not a byte-for-byte replay of remote model generation. Raw prompts, generations,
environment manifests, and study-specific audits remain authoritative in Constrained Sensitivity
Lab. Product runs created by StructTrace are new workload-specific evidence and never retroactively
modify the research record.

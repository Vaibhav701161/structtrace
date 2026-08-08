# Research foundation

StructTrace grew from artifact-backed work on contract sensitivity in constrained structured generation. The accepted evidence chain found a positive but uncertain Qwen estimate for an internal integer representation, followed by a negative canonical replication on Llama and a negative point estimate in a small executable tool-call pilot.

The bundled normalized research fixture preserves exact paired counts:

| Study | Baseline correct | Candidate correct | Candidate-only | Baseline-only |
|---|---:|---:|---:|---:|
| Corrected Qwen | 18/49 | 24/49 | 9 | 3 |
| Canonical Llama | 92/150 | 82/150 | 6 | 16 |
| Tool-call pilot | 26/30 | 24/30 | 1 | 3 |

These are separate studies, not a pooled estimate or model leaderboard. The engineering conclusion is narrower and more useful: representation and contract changes can alter semantic behavior, and their direction may not generalize. Measure them on the actual target workload.

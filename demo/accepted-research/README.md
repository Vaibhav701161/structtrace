# Accepted research evidence demo

This fixture is a compact normalized replay of the accepted paired counts from
Contract Sensitivity Lab. It contains outcome states and source labels, not the
original model text or hidden reasoning.

The three frozen paired matrices are:

| Study | Baseline correct | Candidate correct | Candidate-only | Baseline-only |
|---|---:|---:|---:|---:|
| Corrected Qwen | 18/49 | 24/49 | 9 | 3 |
| Canonical Llama | 92/150 | 82/150 | 6 | 16 |
| Tool-call pilot | 26/30 | 24/30 | 1 | 3 |

The point is not that one model is universally better. The same class of
contract-preserving change had different effects across evaluated systems.

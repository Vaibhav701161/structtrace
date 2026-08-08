# Product demo: the valid-but-wrong regression

Target length: 4 minutes. Use a clean terminal at repository root.

1. Open with the question: “Your schema passed. Did the answer?”
2. Run `structtrace demo support-ticket`.
3. Read the exact result: strict JSON and schema validity improve from 11/12 to 12/12; semantic correctness falls from 10/12 to 8/12; valid-but-wrong rises from 1/12 to 4/12.
4. Open the report with `structtrace report latest --open`.
5. Show the transition matrix, then filter baseline-only cases.
6. Open one case and point to the structured field diff and evaluator evidence.
7. Show the independent gate explanations.
8. Close with: “StructTrace does not tell you which change should win. It shows what changed on your workload before deployment.”

Do not call the candidate universally worse. It is worse on this frozen fixture under this configured primary outcome.

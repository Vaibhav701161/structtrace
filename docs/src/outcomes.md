# Outcomes

An outcome gives application meaning to evaluator facts. Define exactly one composition mode:

```yaml
outcomes:
  executable_correct:
    all_of:
      - tool_name
      - arguments
      - state_receipt
```

`all_of` requires every listed evaluator to pass. `any_of` passes when at least one listed evaluator passes, unless the remaining state is only errors or non-applicable results. Empty compositions and unknown evaluator IDs fail configuration validation.

Choose the primary result explicitly:

```yaml
analysis:
  primary_outcome: executable_correct
```

Additional outcomes remain stored even when they are not primary.

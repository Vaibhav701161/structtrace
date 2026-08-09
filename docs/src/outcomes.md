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

`all_of` uses `False > Error > NotApplicable > True` precedence: a known failure dominates
uncertainty, an error dominates non-applicability, and every evaluator must pass for `True`.
`any_of` uses `True > Error > False > NotApplicable`: a known pass dominates an error; without a
pass, an error remains an error; all non-applicable results produce `NotApplicable`; otherwise the
outcome is `False`. Empty compositions and unknown evaluator IDs fail validation.

Choose the primary result explicitly:

```yaml
analysis:
  primary_outcome: executable_correct
```

Additional outcomes remain stored even when they are not primary.

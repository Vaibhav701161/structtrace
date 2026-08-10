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

Each stored outcome has two independent dimensions: logical `truth` and evaluation health. The
health record includes `fully_evaluated` plus required, passed, failed, error, not-applicable, and
unscored component counts.

`all_of` uses `False > Error > NotApplicable > True` truth precedence: a known failure dominates
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

Truth precedence never erases health. If one required evaluator fails and another errors, the
outcome truth is `False`, `fully_evaluated` is false, and both the failed and error component counts
are retained. Gates use component health rather than composed truth alone.

`known_valid_but_wrong` means schema-valid output with primary truth `False`.
`fully_evaluated_valid_but_wrong` additionally requires every primary component to have completed.
Reports expose both so incomplete evaluation cannot be mistaken for a clean semantic diagnosis.

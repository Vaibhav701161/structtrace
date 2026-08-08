# Valid-but-wrong outputs

An output is valid-but-wrong when strict JSON parsing succeeds, external-schema validation succeeds, and the primary semantic or executable outcome fails.

This metric is valuable because it captures failures that are easy to miss in production dashboards. A migration may reduce parse errors while shifting failures into plausible, contract-compliant objects. StructTrace reports the baseline and candidate counts separately, provides a dedicated case filter, and supports a release threshold:

```yaml
gate:
  max_valid_but_wrong_increase_pp: 0.5
```

The threshold is expressed in percentage points. It is evaluated independently from the primary correctness gate and schema-validity gate.

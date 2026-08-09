# Dataset format

The golden dataset is UTF-8 JSONL with one case per nonblank line:

```json
{"id":"ticket-001","input":{"text":"payment failed"},"expected":{"team":"billing"},"metadata":{"split":"golden"},"model_visible_metadata":{"locale":"en-IN"}}
```

`id` must be a non-empty unique string. Dataset order is preserved. Malformed rows and duplicate IDs fail with line numbers before any model, provider, or subprocess is invoked. The exact source bytes are bound with BLAKE3.

Pointers for nonstandard envelopes are configurable:

```yaml
dataset:
  path: data/golden.jsonl
  format: jsonl
  fields:
    id: /example_id
    input: /request
    expected: /gold
    metadata: /tags
    model_visible_metadata: /request_context
```

Expected values and both metadata classes are optional. `metadata` is evaluation-only and is
available to scorers and retained evidence, never to variants. `model_visible_metadata` is the
only metadata exposed to command, Python, or provider adapters. Configured evaluators may require
expected values or evaluation metadata.

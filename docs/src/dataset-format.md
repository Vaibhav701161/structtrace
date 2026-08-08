# Dataset format

The golden dataset is UTF-8 JSONL with one case per nonblank line:

```json
{"id":"ticket-001","input":{"text":"payment failed"},"expected":{"team":"billing"},"metadata":{"split":"golden"}}
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
```

Expected values and metadata are optional, but configured evaluators may require them.

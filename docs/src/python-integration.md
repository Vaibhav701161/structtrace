# Python integration

Create a project with:

```bash
structtrace init my-check --template python
cd my-check
structtrace run
```

A callable receives a label-free case envelope containing `input` and optional
`model_visible_metadata`. It never receives the dataset ID, golden expected result, or
evaluation-only metadata. It may return a dictionary or a JSON string:

```python
def baseline(case: dict) -> dict:
    text = case["input"]["text"]
    return {"label": "rejected" if "negative" in text else "accepted"}
```

Configure it as `module:callable`:

```yaml
variants:
  baseline:
    kind: python
    interpreter: python
    callable: variants.app:baseline
    timeout_ms: 60000
```

StructTrace invokes the callable through its versioned JSONL bridge. Synchronous and `async def`
callables are supported. Exceptions retain only the exception class by default, never a traceback
or exception message. Malformed requests and non-JSON-serializable return values become per-case
error envelopes; a bad case does not terminate the persistent worker.

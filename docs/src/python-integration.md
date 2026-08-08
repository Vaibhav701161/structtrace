# Python integration

Create a project with:

```bash
structtrace init my-check --template python
cd my-check
structtrace run
```

A callable receives the complete case envelope as a dictionary. It may return a dictionary or a JSON string:

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

StructTrace invokes the callable through its versioned JSONL bridge. Exceptions are returned as redaction-safe error envelopes and tracebacks go to the variant stderr log. Async Python callables are refused rather than executed ambiguously.

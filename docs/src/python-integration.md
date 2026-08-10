# Python integration

Create a project with:

```bash
structtrace init my-check --template python
cd my-check
structtrace run
```

A callable receives a label-free case envelope containing `input` and optional
`model_visible_metadata`. It never receives the dataset ID, golden expected result, or
evaluation-only metadata. It may return JSON values, dataclasses, enums, paths, attrs classes,
or Pydantic v1/v2 models:

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
callables are supported on one event loop for the complete worker lifetime. Ordinary `print()`
calls during import and execution are redirected to bounded stderr; only the bridge owns protocol
stdout. Exceptions retain a safe class message and stable class fingerprint by default, never a
traceback or original exception text. Malformed requests and non-JSON-serializable return values
become per-case error envelopes; a bad case does not terminate the persistent worker.

An advanced callable may explicitly import `StructTraceEnvelope` from `structtrace_bridge`.
Ordinary dictionaries containing a `protocol` key remain ordinary output data. The explicit
wrapper cannot override `protocol`, `protocol_version`, or `case_id`.

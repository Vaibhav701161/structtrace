# Command integration

The command adapter wraps any language that can read and write JSONL. StructTrace starts the configured executable directly, without a shell.

Each stdin request contains the protocol name and version, case ID, input, and explicitly
model-visible metadata. Golden expected values and evaluation-only metadata never cross the
variant boundary. The process must emit exactly one matching response line:

```json
{"protocol":"structtrace.variant","protocol_version":1,"case_id":"case-001","status":"ok","output":{"label":"accepted"}}
```

Application logs belong on stderr. StructTrace drains and caps stderr separately so logging cannot deadlock stdout. A wrong case ID, duplicate response, unsolicited stdout, incompatible protocol version, oversize response, timeout, process crash, or unexpected nonzero exit fails closed.

Persistent mode amortizes startup across cases. Per-case mode creates an isolated process for each case:

```yaml
process_mode: persistent # or per_case
```

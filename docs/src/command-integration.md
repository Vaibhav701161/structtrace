# Command integration

The command adapter wraps any language that can read and write JSONL. StructTrace starts the configured executable directly, without a shell.

Each stdin request contains the protocol name and version, an opaque execution token, input, and
explicitly model-visible metadata. The dataset case ID, golden expected values, and evaluation-only
metadata never cross the variant boundary. The process must echo exactly the opaque token:

```json
{"protocol":"structtrace.variant","protocol_version": 3,"case_id":"stx-opaque-token","status":"ok","output":{"label":"accepted"}}
```

Application logs belong on stderr. StructTrace drains and caps stderr separately so logging cannot deadlock stdout. A wrong case ID, duplicate response, unsolicited stdout, incompatible protocol version, oversize response, timeout, process crash, or unexpected nonzero exit fails closed.

Responses reject unknown fields and contradictory states. An `ok` response cannot contain an
error; an `error` response cannot contain output; and `output` plus `raw_output` must represent the
same JSON value.

Persistent mode amortizes startup across cases. Per-case mode creates an isolated process for each case:

```yaml
process_mode: persistent # or per_case
```

Bind scripts and non-code assets into resume identity when the executable alone is insufficient:

```yaml
implementation:
  digest: sha256:release-owned-digest
  sources: [worker.sh, rules.json]
```

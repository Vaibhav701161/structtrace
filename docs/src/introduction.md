# Introduction

StructTrace is a local-first paired regression harness for structured extraction outputs. It
answers one deployment question: while the caller-facing schema stays fixed, did a change to the
model, prompt, decoder, provider setting, or implementation make the candidate more or less
correct on the same cases?

The central distinction is deliberate. JSON parsing and JSON Schema validation tell you whether an output satisfies a structural contract. They do not tell you whether a classification, extracted amount, tool argument, or workflow result is correct. StructTrace records both classes of evidence and makes outputs that are structurally valid but semantically wrong directly inspectable.

Every run uses matched case IDs, keeps errors and missing rows in the denominator, computes paired transitions, applies user-declared release thresholds, and writes a replayable local evidence bundle. The normal workflow is:

```bash
structtrace init
structtrace demo --open
structtrace run
structtrace report latest --open
structtrace gate latest
```

No telemetry is sent. The recorded-output workflow and bundled demos need no model, Python runtime, provider credential, GPU, or network service.

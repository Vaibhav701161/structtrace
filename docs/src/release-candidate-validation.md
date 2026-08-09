# Release-candidate validation

This protocol is for real external users. A completed automated test suite is necessary, but it cannot establish whether a new user understands the workflow, can integrate an actual application, or trusts the resulting evidence. Do not mark this validation complete from an internal dry run.

## Participant and data safety

- Use consenting participants who were not involved in implementation.
- Ask participants to use synthetic or already-approved non-sensitive data.
- Do not request provider credentials, production outputs, customer records, or private report artifacts.
- Record operating system and command outcomes, not secrets or raw business data.
- Tell participants that command, Python, and custom-evaluator definitions execute user-authorized local code without sandboxing.

## Validation tracks

Each participant completes the offline track. At least one participant should also complete an integration track that matches their normal stack.

### Offline comprehension track

```bash
structtrace --help
structtrace doctor
structtrace demo support-ticket
structtrace report latest --export-share structtrace-share
structtrace replay latest
structtrace gate latest
```

The participant should be able to explain, in their own words:

- why the candidate is more schema-valid;
- why it is nevertheless less correct;
- what a baseline-only transition means;
- why a failed gate is not an execution crash;
- what replay verifies and what it does not verify.

### Integration track

The participant initializes one of the supported templates:

```bash
structtrace init my-structured-change --template recorded
# or: python, command, openai-compatible
cd my-structured-change
structtrace doctor
structtrace run
structtrace report latest --export-share structtrace-share
structtrace gate latest
```

They then replace the generated golden cases, schema, baseline, candidate, and evaluator with a small approved workload from their own context. Provider integration is optional; recorded outputs are sufficient for this track.

## Evidence to retain

Create one record per participant without including workload content:

```text
participant_id:
operating_system:
installation_method:
integration_track:
commands_completed:
first_blocker:
blocker_category:
workaround_required:
documentation_page_used:
gate_result_understood:
replay_result_understood:
would_use_for_a_real_migration:
highest-severity defect:
requested_improvement:
```

Keep command exit codes and sanitized error messages when a task fails. Do not reinterpret a failed task as a pass because the participant eventually found an undocumented workaround.

## Acceptance gate

The external validation gate passes only when all of the following are supported by retained participant records:

- installation and the offline demo complete without maintainer intervention;
- participants distinguish structural validity from the configured primary outcome;
- at least one independent integration reaches a finalized report, gate result, and verified replay;
- no participant encounters silent denominator shrinkage, secret exposure, public report binding, or a false successful exit code;
- every release-blocking defect found during validation has a regression test before closure;
- material confusion is corrected in product copy or documentation and rechecked by someone other than the author.

Feedback is evidence, not a vote. A participant liking the interface cannot override an integrity or safety failure, and one difficult setup does not justify deleting a supported workflow without diagnosing the cause.

## Current status

No external participant result is recorded in the repository. The automated and local acceptance evidence is maintained separately in `ACCEPTANCE.md`.

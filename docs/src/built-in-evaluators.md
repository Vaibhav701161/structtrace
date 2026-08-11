# Built-in evaluators

| Kind | Behavior |
|---|---|
| `exact_json` | complete parsed JSON equality |
| `json_pointer_exact` | one output pointer against one expected pointer |
| `json_pointers_exact` | all configured pointer pairs |
| `enum_accuracy` | exact classification value |
| `normalized_string` | NFKC Unicode normalization, whitespace collapse, and optional Unicode lowercasing. This is deliberately not described as full Unicode case folding. |
| `canonical_date` | accepted date formats parsed, impossible dates rejected, and values compared as ISO dates |
| `numeric_tolerance` | exact integer or exact-decimal absolute/relative tolerance |
| `required_fields` | configured output pointers exist and are non-null |
| `tool_selection` | tool name matches expected |
| `tool_arguments` | selected argument pointers match expected |
| `keyed_array` | order-independent identity matching with identity-only normalization policies, plus independently configured per-item comparisons |
| `financial_invariants` | mapped line amount, subtotal, and total arithmetic using exact decimals; dependent totals become `error`, never false failures, when inputs are incomplete |

Numeric evaluation does not round through binary floating point. Arbitrary-length integer text is
normalized exactly. Decimal comparison uses a bounded 96-bit coefficient with up to 28 fractional
digits. Plain decimal and scientific notation are accepted only when conversion is exact. An
expected numeric reference outside that range produces evaluator `error` and remains not fully
evaluated. An output value outside the declared numeric contract is a candidate failure. Missing
expected values or malformed references also produce explicit evaluator errors.

For invoice line items, `fields` makes item diagnosis practical instead of comparing the whole
object byte-for-byte. Every failed field is attributed to its concrete array index and pointer;
missing, extra, changed-identity, and changed-value cases remain distinguishable. Identity keys and
compared fields are separate policies: a description can use NFKC, whitespace collapse, and
Unicode lowercasing for matching without forcing that field to be a correctness target.

Before baseline or candidate scoring, StructTrace validates every golden value consumed by a
deterministic reference-based evaluator. A missing or malformed reference stops the run as a
dataset error. A missing or malformed model output remains a candidate failure. This boundary
prevents benchmark defects from being reported as model regressions or valid-but-wrong cases.

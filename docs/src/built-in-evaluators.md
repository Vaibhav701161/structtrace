# Built-in evaluators

| Kind | Behavior |
|---|---|
| `exact_json` | complete parsed JSON equality |
| `json_pointer_exact` | one output pointer against one expected pointer |
| `json_pointers_exact` | all configured pointer pairs |
| `enum_accuracy` | exact classification value |
| `normalized_string` | NFKC Unicode normalization, whitespace collapse, and optional case folding |
| `canonical_date` | accepted date formats parsed, impossible dates rejected, and values compared as ISO dates |
| `numeric_tolerance` | exact integer or exact-decimal absolute/relative tolerance |
| `required_fields` | configured output pointers exist and are non-null |
| `tool_selection` | tool name matches expected |
| `tool_arguments` | selected argument pointers match expected |
| `keyed_array` | order-independent identity matching plus optional per-item exact, normalized-string, exact-integer, exact-decimal, decimal-tolerance, and canonical-date comparisons |
| `financial_invariants` | line amount, subtotal, and total arithmetic using exact decimals |

Numeric evaluation does not round through binary floating point. Arbitrary-length integer text is
normalized exactly. Decimal comparison uses a bounded 96-bit coefficient with up to 28 fractional
digits. Plain decimal and scientific notation are accepted only when conversion is exact. A valid
numeric lexeme outside that range produces evaluator `error` and remains not fully evaluated; it
is never labelled as a wrong model answer. Missing expected values or malformed references also
produce explicit evaluator errors.

For invoice line items, `fields` makes item diagnosis practical instead of comparing the whole
object byte-for-byte. Every failed field is attributed to its concrete array index and pointer;
missing, extra, changed-identity, and changed-value cases remain distinguishable.

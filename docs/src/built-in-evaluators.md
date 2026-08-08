# Built-in evaluators

| Kind | Behavior |
|---|---|
| `exact_json` | complete parsed JSON equality |
| `json_pointer_exact` | one output pointer against one expected pointer |
| `json_pointers_exact` | all configured pointer pairs |
| `enum_accuracy` | exact classification value |
| `numeric_tolerance` | exact integer or exact-decimal absolute/relative tolerance |
| `required_fields` | configured output pointers exist and are non-null |
| `tool_selection` | tool name matches expected |
| `tool_arguments` | selected argument pointers match expected |

Numeric evaluation does not round through binary floating point. Arbitrary-length integer text is normalized exactly, and decimal tolerances use exact decimal arithmetic. Missing expected values or malformed numeric references produce explicit evaluator errors.

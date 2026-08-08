//! Strict parsing, schema validation, deterministic evaluators, and outcomes.

use std::{collections::BTreeMap, str::FromStr};

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    config::{EvaluatorConfig, EvaluatorKind, OutcomeConfig, PointerPair},
    dataset::Case,
    output::{OutputStatus, VariantOutput},
};

/// JSON Schema validator used by a complete run.
pub type SchemaValidator = jsonschema::Validator;

/// Compile once with network retrieval unavailable.
pub fn compile_schema(schema: &Value) -> crate::Result<SchemaValidator> {
    jsonschema::validator_for(schema).map_err(|error| crate::CoreError::Schema(error.to_string()))
}

/// Individual deterministic evaluator status.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationStatus {
    /// Evaluator ran and its predicate passed.
    Passed,
    /// Evaluator ran and its predicate failed.
    Failed,
    /// Evaluator could not produce a trustworthy result.
    Error,
    /// Evaluator does not apply to this case.
    NotApplicable,
}

/// One auditable evaluator fact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvaluatorResult {
    /// Configured evaluator ID.
    pub evaluator_id: String,
    /// Four-state evaluator outcome.
    pub status: EvaluationStatus,
    /// Binary convenience value. Errors and not-applicable results are false.
    pub passed: bool,
    /// Optional score in the inclusive zero-to-one range.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    /// Human-readable reason.
    pub message: String,
    /// Structured evidence for reports and replay.
    #[serde(default)]
    pub details: Value,
}

impl EvaluatorResult {
    fn passed(id: &str, message: impl Into<String>, details: Value) -> Self {
        Self {
            evaluator_id: id.to_owned(),
            status: EvaluationStatus::Passed,
            passed: true,
            score: Some(1.0),
            message: message.into(),
            details,
        }
    }

    fn failed(id: &str, message: impl Into<String>, details: Value) -> Self {
        Self {
            evaluator_id: id.to_owned(),
            status: EvaluationStatus::Failed,
            passed: false,
            score: Some(0.0),
            message: message.into(),
            details,
        }
    }

    fn error(id: &str, message: impl Into<String>) -> Self {
        Self {
            evaluator_id: id.to_owned(),
            status: EvaluationStatus::Error,
            passed: false,
            score: None,
            message: message.into(),
            details: Value::Object(Default::default()),
        }
    }
}

/// Four-state named outcome.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeStatus {
    /// Composition passed.
    True,
    /// Composition failed.
    False,
    /// At least one required evaluator errored.
    Error,
    /// Composition was not applicable.
    NotApplicable,
}

impl OutcomeStatus {
    /// Whether this outcome contributes a pass to a primary binary metric.
    pub fn is_pass(self) -> bool {
        self == Self::True
    }
}

/// Full scored output for one variant and case.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CaseEvaluation {
    /// Case ID.
    pub case_id: String,
    /// Adapter-level status.
    pub adapter_status: OutputStatus,
    /// Strict whole-output parse result.
    pub parse_valid: bool,
    /// Parse failure text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_error: Option<String>,
    /// Parsed JSON when strict parsing passed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parsed_output: Option<Value>,
    /// External-schema validity.
    pub schema_valid: bool,
    /// Complete instance and schema path diagnostics.
    #[serde(default)]
    pub schema_errors: Vec<SchemaError>,
    /// Configured evaluator facts.
    pub evaluators: BTreeMap<String, EvaluatorResult>,
    /// Named composed outcomes.
    pub outcomes: BTreeMap<String, OutcomeStatus>,
    /// Whether the primary semantic outcome passed.
    pub primary_pass: bool,
    /// Strict parse plus schema validity plus primary failure.
    pub valid_but_wrong: bool,
}

/// JSON Schema failure location and explanation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchemaError {
    /// JSON Pointer-like instance path.
    pub instance_path: String,
    /// Schema path.
    pub schema_path: String,
    /// Validator explanation.
    pub message: String,
}

/// Strictly parse one complete JSON value.
pub fn parse_strict(raw: &str) -> std::result::Result<Value, String> {
    serde_json::from_str::<Value>(raw.trim()).map_err(|error| error.to_string())
}

/// Evaluate one output without removing failures from the denominator.
pub fn evaluate_case(
    case: &Case,
    output: &VariantOutput,
    schema: &SchemaValidator,
    evaluators: &[EvaluatorConfig],
    outcomes: &BTreeMap<String, OutcomeConfig>,
    primary_outcome: &str,
) -> CaseEvaluation {
    evaluate_case_with_external(
        case,
        output,
        schema,
        evaluators,
        outcomes,
        primary_outcome,
        &BTreeMap::new(),
    )
}

/// Evaluate one output while incorporating explicitly executed external evaluators.
pub fn evaluate_case_with_external(
    case: &Case,
    output: &VariantOutput,
    schema: &SchemaValidator,
    evaluators: &[EvaluatorConfig],
    outcomes: &BTreeMap<String, OutcomeConfig>,
    primary_outcome: &str,
    external_results: &BTreeMap<String, EvaluatorResult>,
) -> CaseEvaluation {
    let parsed_result = output
        .parse_source()
        .filter(|_| output.status == OutputStatus::Ok)
        .ok_or_else(|| {
            output.error.as_ref().map_or_else(
                || "adapter did not return an output".to_owned(),
                |error| error.message.clone(),
            )
        })
        .and_then(|raw| parse_strict(&raw));
    let (parsed_output, parse_error) = match parsed_result {
        Ok(value) => (Some(value), None),
        Err(error) => (None, Some(error)),
    };
    let mut schema_errors = Vec::new();
    if let Some(value) = &parsed_output {
        for error in schema.iter_errors(value) {
            schema_errors.push(SchemaError {
                instance_path: error.instance_path().to_string(),
                schema_path: error.schema_path().to_string(),
                message: error.to_string(),
            });
        }
    }
    let schema_valid = parsed_output.is_some() && schema_errors.is_empty();
    let evaluator_results = evaluators
        .iter()
        .map(|config| {
            let result = match &config.kind {
                EvaluatorKind::Command { .. } | EvaluatorKind::Python { .. } => external_results
                    .get(&config.id)
                    .cloned()
                    .unwrap_or_else(|| {
                        EvaluatorResult::error(
                            &config.id,
                            "external evaluator did not produce a result",
                        )
                    }),
                _ => evaluate_builtin(config, case.expected.as_ref(), parsed_output.as_ref()),
            };
            (config.id.clone(), result)
        })
        .collect::<BTreeMap<_, _>>();
    let outcome_results = outcomes
        .iter()
        .map(|(name, config)| (name.clone(), compose_outcome(config, &evaluator_results)))
        .collect::<BTreeMap<_, _>>();
    let primary_pass = outcome_results
        .get(primary_outcome)
        .copied()
        .is_some_and(OutcomeStatus::is_pass);
    CaseEvaluation {
        case_id: case.id.clone(),
        adapter_status: output.status,
        parse_valid: parsed_output.is_some(),
        parse_error,
        parsed_output,
        schema_valid,
        schema_errors,
        evaluators: evaluator_results,
        outcomes: outcome_results,
        primary_pass,
        valid_but_wrong: schema_valid && !primary_pass,
    }
}

fn evaluate_builtin(
    config: &EvaluatorConfig,
    expected: Option<&Value>,
    output: Option<&Value>,
) -> EvaluatorResult {
    let id = &config.id;
    let Some(output) = output else {
        return EvaluatorResult::error(id, "strict JSON parsing did not produce a value");
    };
    match &config.kind {
        EvaluatorKind::ExactJson => match expected {
            Some(expected) if expected == output => EvaluatorResult::passed(
                id,
                "Output exactly matched the expected JSON.",
                Value::Null,
            ),
            Some(expected) => EvaluatorResult::failed(
                id,
                "Output did not exactly match the expected JSON.",
                serde_json::json!({"expected": expected, "actual": output}),
            ),
            None => EvaluatorResult::error(id, "case has no expected value"),
        },
        EvaluatorKind::JsonPointerExact {
            pointer,
            expected_pointer,
        }
        | EvaluatorKind::EnumAccuracy {
            pointer,
            expected_pointer,
        } => compare_pointer(id, output, expected, pointer, expected_pointer),
        EvaluatorKind::JsonPointersExact { pointers }
        | EvaluatorKind::ToolArguments { pointers } => {
            compare_pointer_list(id, output, expected, pointers)
        }
        EvaluatorKind::NumericTolerance {
            pointer,
            expected_pointer,
            absolute,
            relative,
            exact_integer,
        } => evaluate_numeric(
            id,
            output,
            expected,
            pointer,
            expected_pointer,
            absolute.as_deref(),
            relative.as_deref(),
            *exact_integer,
        ),
        EvaluatorKind::RequiredFields { pointers } => {
            let missing = pointers
                .iter()
                .filter(|pointer| output.pointer(pointer).is_none_or(Value::is_null))
                .cloned()
                .collect::<Vec<_>>();
            if missing.is_empty() {
                EvaluatorResult::passed(
                    id,
                    "All required fields were present and non-null.",
                    serde_json::json!({"pointers": pointers}),
                )
            } else {
                EvaluatorResult::failed(
                    id,
                    "One or more required fields were missing or null.",
                    serde_json::json!({"missing": missing}),
                )
            }
        }
        EvaluatorKind::ToolSelection {
            pointer,
            expected_pointer,
        } => compare_pointer(id, output, expected, pointer, expected_pointer),
        EvaluatorKind::Command { .. } | EvaluatorKind::Python { .. } => EvaluatorResult::error(
            id,
            "custom evaluator must be executed by structtrace-adapters",
        ),
    }
}

fn compare_pointer(
    id: &str,
    output: &Value,
    expected: Option<&Value>,
    pointer: &str,
    expected_pointer: &str,
) -> EvaluatorResult {
    let Some(expected) = expected else {
        return EvaluatorResult::error(id, "case has no expected value");
    };
    let actual_value = output.pointer(pointer);
    let expected_value = expected.pointer(expected_pointer);
    match (actual_value, expected_value) {
        (Some(actual), Some(reference)) if actual == reference => EvaluatorResult::passed(
            id,
            format!("Value at {pointer} matched the expected value."),
            serde_json::json!({"pointer": pointer, "value": actual}),
        ),
        (Some(actual), Some(reference)) => EvaluatorResult::failed(
            id,
            format!("Value at {pointer} did not match."),
            serde_json::json!({"pointer": pointer, "expected": reference, "actual": actual}),
        ),
        (None, _) => EvaluatorResult::failed(
            id,
            format!("Output pointer {pointer} did not resolve."),
            serde_json::json!({"pointer": pointer, "failure": "missing_output_field"}),
        ),
        (_, None) => EvaluatorResult::error(
            id,
            format!("expected pointer {expected_pointer} did not resolve"),
        ),
    }
}

fn compare_pointer_list(
    id: &str,
    output: &Value,
    expected: Option<&Value>,
    pointers: &[PointerPair],
) -> EvaluatorResult {
    let Some(expected) = expected else {
        return EvaluatorResult::error(id, "case has no expected value");
    };
    let mut failures = Vec::new();
    for pair in pointers {
        let actual = output.pointer(&pair.pointer);
        let reference = expected.pointer(&pair.expected_pointer);
        if actual != reference {
            failures.push(serde_json::json!({
                "pointer": pair.pointer,
                "expected_pointer": pair.expected_pointer,
                "expected": reference,
                "actual": actual,
            }));
        }
    }
    if failures.is_empty() {
        EvaluatorResult::passed(id, "All selected fields matched.", Value::Null)
    } else {
        EvaluatorResult::failed(
            id,
            format!("{} selected field(s) did not match.", failures.len()),
            serde_json::json!({"failures": failures}),
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn evaluate_numeric(
    id: &str,
    output: &Value,
    expected: Option<&Value>,
    pointer: &str,
    expected_pointer: &str,
    absolute: Option<&str>,
    relative: Option<&str>,
    exact_integer: bool,
) -> EvaluatorResult {
    let Some(expected) = expected else {
        return EvaluatorResult::error(id, "case has no expected value");
    };
    let Some(actual_value) = output.pointer(pointer) else {
        return EvaluatorResult::failed(
            id,
            format!("Output pointer {pointer} did not resolve."),
            Value::Null,
        );
    };
    let Some(expected_value) = expected.pointer(expected_pointer) else {
        return EvaluatorResult::error(
            id,
            format!("expected pointer {expected_pointer} did not resolve"),
        );
    };
    let actual_text = number_text(actual_value);
    let expected_text = number_text(expected_value);
    let (Some(actual_text), Some(expected_text)) = (actual_text, expected_text) else {
        return EvaluatorResult::failed(
            id,
            "Numeric evaluator received a non-numeric value.",
            serde_json::json!({"actual": actual_value, "expected": expected_value}),
        );
    };
    if exact_integer {
        let actual = canonical_integer(actual_text);
        let reference = canonical_integer(expected_text);
        return match (actual, reference) {
            (Some(actual), Some(reference)) if actual == reference => EvaluatorResult::passed(
                id,
                "Integer values matched exactly.",
                serde_json::json!({"value": actual}),
            ),
            (Some(actual), Some(reference)) => EvaluatorResult::failed(
                id,
                "Integer values did not match exactly.",
                serde_json::json!({"actual": actual, "expected": reference}),
            ),
            _ => EvaluatorResult::failed(
                id,
                "Exact-integer comparison requires integer values.",
                Value::Null,
            ),
        };
    }
    let actual = Decimal::from_str(actual_text);
    let reference = Decimal::from_str(expected_text);
    let (Ok(actual), Ok(reference)) = (actual, reference) else {
        return EvaluatorResult::failed(
            id,
            "Values could not be represented as exact decimals.",
            Value::Null,
        );
    };
    let absolute_tolerance = absolute.and_then(|value| Decimal::from_str(value).ok());
    let relative_tolerance = relative.and_then(|value| Decimal::from_str(value).ok());
    if absolute.is_some() && absolute_tolerance.is_none()
        || relative.is_some() && relative_tolerance.is_none()
    {
        return EvaluatorResult::error(id, "configured tolerance is not a valid decimal");
    }
    let difference = (actual - reference).abs();
    let absolute_pass = absolute_tolerance.is_some_and(|limit| difference <= limit);
    let relative_pass = relative_tolerance.is_some_and(|limit| {
        if reference.is_zero() {
            difference.is_zero()
        } else {
            difference / reference.abs() <= limit
        }
    });
    if difference.is_zero() || absolute_pass || relative_pass {
        EvaluatorResult::passed(
            id,
            "Numeric value was within the configured tolerance.",
            serde_json::json!({"difference": difference.to_string()}),
        )
    } else {
        EvaluatorResult::failed(
            id,
            "Numeric value exceeded the configured tolerance.",
            serde_json::json!({"difference": difference.to_string()}),
        )
    }
}

fn number_text(value: &Value) -> Option<&str> {
    match value {
        Value::Number(number) => Some(number.as_str()),
        Value::String(text) => Some(text),
        _ => None,
    }
}

fn canonical_integer(text: &str) -> Option<String> {
    let text = text.trim();
    let (negative, digits) = text
        .strip_prefix('-')
        .map_or((false, text), |rest| (true, rest));
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let trimmed = digits.trim_start_matches('0');
    let normalized = if trimmed.is_empty() { "0" } else { trimmed };
    if negative && normalized != "0" {
        Some(format!("-{normalized}"))
    } else {
        Some(normalized.to_owned())
    }
}

fn compose_outcome(
    config: &OutcomeConfig,
    results: &BTreeMap<String, EvaluatorResult>,
) -> OutcomeStatus {
    let ids = if config.all_of.is_empty() {
        &config.any_of
    } else {
        &config.all_of
    };
    let selected = ids
        .iter()
        .filter_map(|id| results.get(id))
        .collect::<Vec<_>>();
    if selected.len() != ids.len()
        || selected
            .iter()
            .any(|result| result.status == EvaluationStatus::Error)
    {
        return OutcomeStatus::Error;
    }
    if selected
        .iter()
        .all(|result| result.status == EvaluationStatus::NotApplicable)
    {
        return OutcomeStatus::NotApplicable;
    }
    if !config.all_of.is_empty() {
        if selected.iter().all(|result| result.passed) {
            OutcomeStatus::True
        } else {
            OutcomeStatus::False
        }
    } else if selected.iter().any(|result| result.passed) {
        OutcomeStatus::True
    } else {
        OutcomeStatus::False
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use proptest::prelude::*;
    use serde_json::json;

    use crate::{
        config::{EvaluatorConfig, EvaluatorKind, OutcomeConfig},
        dataset::Case,
        output::VariantOutput,
    };

    use super::*;

    fn output(raw: &str) -> VariantOutput {
        serde_json::from_value(json!({
            "case_id": "a",
            "status": "ok",
            "raw_output": raw
        }))
        .unwrap()
    }

    #[test]
    fn surrounding_prose_fails_scored_parse() {
        assert!(parse_strict("Here: {\"a\":1}").is_err());
    }

    proptest! {
        #[test]
        fn exact_integer_normalization_preserves_arbitrary_length_identity(
            negative in any::<bool>(),
            leading_zeroes in 0usize..32,
            digits in "[1-9][0-9]{0,200}",
        ) {
            let sign = if negative { "-" } else { "" };
            let encoded = format!("{sign}{}{digits}", "0".repeat(leading_zeroes));
            let expected = format!("{sign}{digits}");
            prop_assert_eq!(canonical_integer(&encoded), Some(expected));
        }

        #[test]
        fn signed_zero_has_one_canonical_form(
            negative in any::<bool>(),
            zeroes in 1usize..128,
        ) {
            let sign = if negative { "-" } else { "" };
            let encoded = format!("{sign}{}", "0".repeat(zeroes));
            prop_assert_eq!(canonical_integer(&encoded), Some("0".to_owned()));
        }
    }

    #[test]
    fn valid_but_wrong_is_first_class() {
        let schema = compile_schema(&json!({
            "type": "object",
            "required": ["priority"],
            "properties": {"priority": {"type": "string"}},
            "additionalProperties": false
        }))
        .unwrap();
        let case = Case {
            id: "a".to_owned(),
            input: Value::Null,
            expected: Some(json!({"priority": "high"})),
            metadata: None,
            source_line: 1,
        };
        let evaluators = vec![EvaluatorConfig {
            id: "priority".to_owned(),
            kind: EvaluatorKind::JsonPointerExact {
                pointer: "/priority".to_owned(),
                expected_pointer: "/priority".to_owned(),
            },
        }];
        let outcomes = BTreeMap::from([(
            "semantic".to_owned(),
            OutcomeConfig {
                all_of: vec!["priority".to_owned()],
                any_of: vec![],
            },
        )]);
        let result = evaluate_case(
            &case,
            &output(r#"{"priority":"low"}"#),
            &schema,
            &evaluators,
            &outcomes,
            "semantic",
        );
        assert!(result.parse_valid);
        assert!(result.schema_valid);
        assert!(!result.primary_pass);
        assert!(result.valid_but_wrong);
    }

    #[test]
    fn exact_integer_supports_values_beyond_i128() {
        assert_eq!(
            canonical_integer("000123456789012345678901234567890"),
            Some("123456789012345678901234567890".to_owned())
        );
        assert_eq!(canonical_integer("-000"), Some("0".to_owned()));
    }
}

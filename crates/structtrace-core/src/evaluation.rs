//! Strict parsing, schema validation, deterministic evaluators, and outcomes.

use std::{collections::BTreeMap, str::FromStr};

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use unicode_normalization::UnicodeNormalization;

use crate::{
    config::{EvaluatorConfig, EvaluatorKind, KeyedArrayField, OutcomeConfig, PointerPair},
    dataset::Case,
    output::{OutputStatus, VariantOutput},
};

/// JSON Schema validator used by a complete run.
pub type SchemaValidator = jsonschema::Validator;

/// Compile once with network retrieval unavailable.
pub fn compile_schema(schema: &Value) -> crate::Result<SchemaValidator> {
    jsonschema::options()
        .should_validate_formats(true)
        .build(schema)
        .map_err(|error| crate::CoreError::Schema(error.to_string()))
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
    /// Pointer-level facts produced by field-aware built-in evaluators.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<FieldEvaluationFact>,
}

/// One resolved field comparison used for truthful hotspot attribution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FieldEvaluationFact {
    /// Output JSON Pointer.
    pub pointer: String,
    /// Expected-value JSON Pointer when the evaluator uses a reference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_pointer: Option<String>,
    /// Four-state result for this field only.
    pub status: EvaluationStatus,
    /// Concrete expected value when it resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<Value>,
    /// Concrete output value when it resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<Value>,
    /// Auditable field-specific explanation.
    pub message: String,
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
            fields: Vec::new(),
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
            fields: Vec::new(),
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
            fields: Vec::new(),
        }
    }

    fn with_fields(mut self, fields: Vec<FieldEvaluationFact>) -> Self {
        self.fields = fields;
        self
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

/// Versioned composed truth together with independent evaluation-health facts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutcomeResult {
    /// Logical result under the configured `all_of` or `any_of` composition.
    pub truth: OutcomeStatus,
    /// True only when every required evaluator produced a resolved pass or fail.
    pub fully_evaluated: bool,
    /// Number of evaluator components required by this outcome.
    pub required_components: usize,
    /// Required components that passed.
    pub passed_components: usize,
    /// Required components that failed.
    pub failed_components: usize,
    /// Required components that errored.
    pub error_components: usize,
    /// Required components that were not applicable.
    pub not_applicable_components: usize,
    /// Required components absent from the evaluator result set.
    pub unscored_components: usize,
}

impl OutcomeResult {
    /// Construct a synthetic single-component result for tests and migrations.
    pub fn from_truth(truth: OutcomeStatus) -> Self {
        let (passed, failed, error, not_applicable) = match truth {
            OutcomeStatus::True => (1, 0, 0, 0),
            OutcomeStatus::False => (0, 1, 0, 0),
            OutcomeStatus::Error => (0, 0, 1, 0),
            OutcomeStatus::NotApplicable => (0, 0, 0, 1),
        };
        Self {
            truth,
            fully_evaluated: matches!(truth, OutcomeStatus::True | OutcomeStatus::False),
            required_components: 1,
            passed_components: passed,
            failed_components: failed,
            error_components: error,
            not_applicable_components: not_applicable,
            unscored_components: 0,
        }
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
    pub outcomes: BTreeMap<String, OutcomeResult>,
    /// Whether the primary semantic outcome passed.
    pub primary_pass: bool,
    /// Strict parse plus schema validity plus primary failure.
    pub valid_but_wrong: bool,
    /// Structurally valid, semantically false, and evaluated by every required component.
    pub fully_evaluated_valid_but_wrong: bool,
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
    crate::strict_json::value_from_str(raw.trim()).map_err(|error| error.to_string())
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
    let parsed_result = if output.status == OutputStatus::Ok {
        output.parse_source().map_or_else(
            || {
                Err(output
                    .metadata
                    .pointer("/_structtrace_retained_parse_error")
                    .and_then(Value::as_str)
                    .unwrap_or("adapter did not return an output")
                    .to_owned())
            },
            |raw| parse_strict(&raw),
        )
    } else {
        Err(output.error.as_ref().map_or_else(
            || "adapter did not return an output".to_owned(),
            |error| error.message.clone(),
        ))
    };
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
    let primary_result = outcome_results.get(primary_outcome);
    let primary_status = primary_result.map(|result| result.truth);
    let primary_pass = primary_status.is_some_and(OutcomeStatus::is_pass);
    let primary_fully_evaluated = primary_result.is_some_and(|result| result.fully_evaluated);
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
        valid_but_wrong: schema_valid && primary_status == Some(OutcomeStatus::False),
        fully_evaluated_valid_but_wrong: schema_valid
            && primary_status == Some(OutcomeStatus::False)
            && primary_fully_evaluated,
    }
}

fn evaluate_builtin(
    config: &EvaluatorConfig,
    expected: Option<&Value>,
    output: Option<&Value>,
) -> EvaluatorResult {
    let id = &config.id;
    let Some(output) = output else {
        return EvaluatorResult::error(id, "strict JSON parsing did not produce a value")
            .with_fields(unparsed_field_facts(&config.kind));
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
        EvaluatorKind::NormalizedString {
            pointer,
            expected_pointer,
            case_insensitive,
        } => evaluate_normalized_string(
            id,
            output,
            expected,
            pointer,
            expected_pointer,
            *case_insensitive,
        ),
        EvaluatorKind::CanonicalDate {
            pointer,
            expected_pointer,
            formats,
        } => evaluate_canonical_date(id, output, expected, pointer, expected_pointer, formats),
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
            let fields = pointers
                .iter()
                .map(|pointer| {
                    let actual = output.pointer(pointer).cloned();
                    let passed = actual.as_ref().is_some_and(|value| !value.is_null());
                    FieldEvaluationFact {
                        pointer: pointer.clone(),
                        expected_pointer: None,
                        status: if passed {
                            EvaluationStatus::Passed
                        } else {
                            EvaluationStatus::Failed
                        },
                        expected: None,
                        actual,
                        message: if passed {
                            "Required field was present and non-null.".to_owned()
                        } else {
                            "Required field was missing or null.".to_owned()
                        },
                    }
                })
                .collect::<Vec<_>>();
            let missing = fields
                .iter()
                .filter(|field| field.status == EvaluationStatus::Failed)
                .map(|field| field.pointer.clone())
                .collect::<Vec<_>>();
            if missing.is_empty() {
                EvaluatorResult::passed(
                    id,
                    "All required fields were present and non-null.",
                    serde_json::json!({"pointers": pointers}),
                )
                .with_fields(fields)
            } else {
                EvaluatorResult::failed(
                    id,
                    "One or more required fields were missing or null.",
                    serde_json::json!({"missing": missing}),
                )
                .with_fields(fields)
            }
        }
        EvaluatorKind::ToolSelection {
            pointer,
            expected_pointer,
        } => compare_pointer(id, output, expected, pointer, expected_pointer),
        EvaluatorKind::KeyedArray {
            pointer,
            expected_pointer,
            keys,
            fields,
        } => evaluate_keyed_array(
            id,
            output,
            expected,
            pointer,
            expected_pointer,
            keys,
            fields,
        ),
        EvaluatorKind::FinancialInvariants {
            line_items_pointer,
            subtotal_pointer,
            tax_pointer,
            total_pointer,
            absolute,
        } => evaluate_financial_invariants(
            id,
            output,
            line_items_pointer,
            subtotal_pointer,
            tax_pointer,
            total_pointer,
            absolute,
        ),
        EvaluatorKind::Command { .. } | EvaluatorKind::Python { .. } => EvaluatorResult::error(
            id,
            "custom evaluator must be executed by structtrace-adapters",
        ),
    }
}

fn unparsed_field_facts(kind: &EvaluatorKind) -> Vec<FieldEvaluationFact> {
    let pointers = match kind {
        EvaluatorKind::JsonPointerExact {
            pointer,
            expected_pointer,
        }
        | EvaluatorKind::EnumAccuracy {
            pointer,
            expected_pointer,
        }
        | EvaluatorKind::NormalizedString {
            pointer,
            expected_pointer,
            ..
        }
        | EvaluatorKind::CanonicalDate {
            pointer,
            expected_pointer,
            ..
        }
        | EvaluatorKind::ToolSelection {
            pointer,
            expected_pointer,
        }
        | EvaluatorKind::NumericTolerance {
            pointer,
            expected_pointer,
            ..
        } => vec![(pointer.clone(), Some(expected_pointer.clone()))],
        EvaluatorKind::JsonPointersExact { pointers }
        | EvaluatorKind::ToolArguments { pointers } => pointers
            .iter()
            .map(|pair| (pair.pointer.clone(), Some(pair.expected_pointer.clone())))
            .collect(),
        EvaluatorKind::KeyedArray {
            pointer,
            expected_pointer,
            ..
        } => vec![(pointer.clone(), Some(expected_pointer.clone()))],
        EvaluatorKind::FinancialInvariants {
            line_items_pointer,
            subtotal_pointer,
            tax_pointer,
            total_pointer,
            ..
        } => vec![
            (line_items_pointer.clone(), None),
            (subtotal_pointer.clone(), None),
            (tax_pointer.clone(), None),
            (total_pointer.clone(), None),
        ],
        EvaluatorKind::RequiredFields { pointers } => pointers
            .iter()
            .map(|pointer| (pointer.clone(), None))
            .collect(),
        EvaluatorKind::ExactJson | EvaluatorKind::Command { .. } | EvaluatorKind::Python { .. } => {
            Vec::new()
        }
    };
    pointers
        .into_iter()
        .map(|(pointer, expected_pointer)| FieldEvaluationFact {
            pointer,
            expected_pointer,
            status: EvaluationStatus::Error,
            expected: None,
            actual: None,
            message: "Strict JSON parsing did not produce a value.".to_owned(),
        })
        .collect()
}

fn compare_pointer(
    id: &str,
    output: &Value,
    expected: Option<&Value>,
    pointer: &str,
    expected_pointer: &str,
) -> EvaluatorResult {
    let Some(expected) = expected else {
        return EvaluatorResult::error(id, "case has no expected value").with_fields(vec![
            FieldEvaluationFact {
                pointer: pointer.to_owned(),
                expected_pointer: Some(expected_pointer.to_owned()),
                status: EvaluationStatus::Error,
                expected: None,
                actual: output.pointer(pointer).cloned(),
                message: "Case has no expected value.".to_owned(),
            },
        ]);
    };
    let actual_value = output.pointer(pointer);
    let expected_value = expected.pointer(expected_pointer);
    let field = |status, actual: Option<&Value>, reference: Option<&Value>, message: &str| {
        FieldEvaluationFact {
            pointer: pointer.to_owned(),
            expected_pointer: Some(expected_pointer.to_owned()),
            status,
            expected: reference.cloned(),
            actual: actual.cloned(),
            message: message.to_owned(),
        }
    };
    match (actual_value, expected_value) {
        (Some(actual), Some(reference)) if actual == reference => EvaluatorResult::passed(
            id,
            format!("Value at {pointer} matched the expected value."),
            serde_json::json!({"pointer": pointer, "value": actual}),
        )
        .with_fields(vec![field(
            EvaluationStatus::Passed,
            Some(actual),
            Some(reference),
            "Values matched.",
        )]),
        (Some(actual), Some(reference)) => EvaluatorResult::failed(
            id,
            format!("Value at {pointer} did not match."),
            serde_json::json!({"pointer": pointer, "expected": reference, "actual": actual}),
        )
        .with_fields(vec![field(
            EvaluationStatus::Failed,
            Some(actual),
            Some(reference),
            "Values did not match.",
        )]),
        (_, None) => EvaluatorResult::error(
            id,
            format!("expected pointer {expected_pointer} did not resolve"),
        )
        .with_fields(vec![field(
            EvaluationStatus::Error,
            actual_value,
            None,
            "Expected pointer did not resolve.",
        )]),
        (None, Some(reference)) => EvaluatorResult::failed(
            id,
            format!("Output pointer {pointer} did not resolve."),
            serde_json::json!({"pointer": pointer, "failure": "missing_output_field"}),
        )
        .with_fields(vec![field(
            EvaluationStatus::Failed,
            None,
            Some(reference),
            "Output pointer did not resolve.",
        )]),
    }
}

fn compare_pointer_list(
    id: &str,
    output: &Value,
    expected: Option<&Value>,
    pointers: &[PointerPair],
) -> EvaluatorResult {
    let Some(expected) = expected else {
        return EvaluatorResult::error(id, "case has no expected value").with_fields(
            pointers
                .iter()
                .map(|pair| FieldEvaluationFact {
                    pointer: pair.pointer.clone(),
                    expected_pointer: Some(pair.expected_pointer.clone()),
                    status: EvaluationStatus::Error,
                    expected: None,
                    actual: output.pointer(&pair.pointer).cloned(),
                    message: "Case has no expected value.".to_owned(),
                })
                .collect(),
        );
    };
    let mut fields = Vec::with_capacity(pointers.len());
    let mut missing_expected = false;
    for pair in pointers {
        let actual = output.pointer(&pair.pointer).cloned();
        let reference = expected.pointer(&pair.expected_pointer).cloned();
        let (status, message) = match (&actual, &reference) {
            (_, None) => {
                missing_expected = true;
                (EvaluationStatus::Error, "Expected pointer did not resolve.")
            }
            (None, Some(_)) => (EvaluationStatus::Failed, "Output pointer did not resolve."),
            (Some(actual), Some(reference)) if actual == reference => {
                (EvaluationStatus::Passed, "Values matched.")
            }
            (Some(_), Some(_)) => (EvaluationStatus::Failed, "Values did not match."),
        };
        fields.push(FieldEvaluationFact {
            pointer: pair.pointer.clone(),
            expected_pointer: Some(pair.expected_pointer.clone()),
            status,
            expected: reference,
            actual,
            message: message.to_owned(),
        });
    }
    let failures = fields
        .iter()
        .filter(|field| field.status == EvaluationStatus::Failed)
        .cloned()
        .collect::<Vec<_>>();
    if missing_expected {
        EvaluatorResult::error(id, "one or more expected pointers did not resolve")
            .with_fields(fields)
    } else if failures.is_empty() {
        EvaluatorResult::passed(id, "All selected fields matched.", Value::Null).with_fields(fields)
    } else {
        EvaluatorResult::failed(
            id,
            format!("{} selected field(s) did not match.", failures.len()),
            serde_json::json!({"failures": failures}),
        )
        .with_fields(fields)
    }
}

fn evaluate_normalized_string(
    id: &str,
    output: &Value,
    expected: Option<&Value>,
    pointer: &str,
    expected_pointer: &str,
    case_insensitive: bool,
) -> EvaluatorResult {
    let Some(expected) = expected else {
        return field_error(
            id,
            output,
            pointer,
            expected_pointer,
            "case has no expected value",
        );
    };
    let actual = output.pointer(pointer);
    let reference = expected.pointer(expected_pointer);
    let (Some(actual_text), Some(reference_text)) = (
        actual.and_then(Value::as_str),
        reference.and_then(Value::as_str),
    ) else {
        return field_result(
            id,
            EvaluationStatus::Error,
            pointer,
            expected_pointer,
            actual,
            reference,
            "Normalized-string inputs must both be strings.",
            Value::Null,
        );
    };
    let actual_normalized = normalize_text(actual_text, case_insensitive);
    let expected_normalized = normalize_text(reference_text, case_insensitive);
    let status = if actual_normalized == expected_normalized {
        EvaluationStatus::Passed
    } else {
        EvaluationStatus::Failed
    };
    field_result(
        id,
        status,
        pointer,
        expected_pointer,
        actual,
        reference,
        if status == EvaluationStatus::Passed {
            "Normalized strings matched."
        } else {
            "Normalized strings differed."
        },
        serde_json::json!({
            "actual_normalized": actual_normalized,
            "expected_normalized": expected_normalized,
        }),
    )
}

fn normalize_text(value: &str, case_insensitive: bool) -> String {
    let normalized = value.nfkc().collect::<String>();
    let collapsed = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
    if case_insensitive {
        collapsed.to_lowercase()
    } else {
        collapsed
    }
}

fn evaluate_canonical_date(
    id: &str,
    output: &Value,
    expected: Option<&Value>,
    pointer: &str,
    expected_pointer: &str,
    formats: &[String],
) -> EvaluatorResult {
    let Some(expected) = expected else {
        return field_error(
            id,
            output,
            pointer,
            expected_pointer,
            "case has no expected value",
        );
    };
    let actual = output.pointer(pointer);
    let reference = expected.pointer(expected_pointer);
    let (Some(actual_text), Some(reference_text)) = (
        actual.and_then(Value::as_str),
        reference.and_then(Value::as_str),
    ) else {
        return field_result(
            id,
            EvaluationStatus::Error,
            pointer,
            expected_pointer,
            actual,
            reference,
            "Canonical-date inputs must both be strings.",
            Value::Null,
        );
    };
    let actual_date = canonical_date(actual_text, formats);
    let expected_date = canonical_date(reference_text, formats);
    let (Some(actual_date), Some(expected_date)) = (actual_date, expected_date) else {
        return field_result(
            id,
            EvaluationStatus::Error,
            pointer,
            expected_pointer,
            actual,
            reference,
            "A date was invalid or did not match an accepted format.",
            serde_json::json!({"accepted_formats": formats}),
        );
    };
    let status = if actual_date == expected_date {
        EvaluationStatus::Passed
    } else {
        EvaluationStatus::Failed
    };
    field_result(
        id,
        status,
        pointer,
        expected_pointer,
        actual,
        reference,
        if status == EvaluationStatus::Passed {
            "Canonical dates matched."
        } else {
            "Canonical dates differed."
        },
        serde_json::json!({"actual_iso": actual_date, "expected_iso": expected_date}),
    )
}

fn canonical_date(value: &str, formats: &[String]) -> Option<String> {
    for format in formats {
        let parts = match format.as_str() {
            "iso" => parse_date_parts(value, '-'),
            "dmy_slash" => parse_date_parts(value, '/').map(|(d, m, y)| (y, m, d)),
            "mdy_slash" => parse_date_parts(value, '/').map(|(m, d, y)| (y, m, d)),
            _ => None,
        };
        if let Some((year, month, day)) = parts.filter(|parts| valid_date(*parts)) {
            return Some(format!("{year:04}-{month:02}-{day:02}"));
        }
    }
    None
}

fn parse_date_parts(value: &str, separator: char) -> Option<(u32, u32, u32)> {
    let mut parts = value.split(separator);
    let first = parts.next()?.parse().ok()?;
    let second = parts.next()?.parse().ok()?;
    let third = parts.next()?.parse().ok()?;
    (parts.next().is_none()).then_some((first, second, third))
}

fn valid_date((year, month, day): (u32, u32, u32)) -> bool {
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let maximum = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    year > 0 && (1..=maximum).contains(&day)
}

fn evaluate_keyed_array(
    id: &str,
    output: &Value,
    expected: Option<&Value>,
    pointer: &str,
    expected_pointer: &str,
    keys: &[String],
    field_configs: &[KeyedArrayField],
) -> EvaluatorResult {
    let Some(expected) = expected else {
        return field_error(
            id,
            output,
            pointer,
            expected_pointer,
            "case has no expected value",
        );
    };
    let actual = output.pointer(pointer);
    let reference = expected.pointer(expected_pointer);
    let (Some(actual_items), Some(expected_items)) = (
        actual.and_then(Value::as_array),
        reference.and_then(Value::as_array),
    ) else {
        return field_result(
            id,
            EvaluationStatus::Error,
            pointer,
            expected_pointer,
            actual,
            reference,
            "Keyed-array inputs must both be arrays.",
            Value::Null,
        );
    };
    let actual_map = keyed_items(actual_items, keys, field_configs);
    let expected_map = keyed_items(expected_items, keys, field_configs);
    let (Ok(actual_map), Ok(expected_map)) = (actual_map, expected_map) else {
        return field_result(
            id,
            EvaluationStatus::Error,
            pointer,
            expected_pointer,
            actual,
            reference,
            "Array items had missing or duplicate keys.",
            serde_json::json!({"keys": keys}),
        );
    };
    let missing = expected_map
        .keys()
        .filter(|key| !actual_map.contains_key(*key))
        .cloned()
        .collect::<Vec<_>>();
    let extra = actual_map
        .keys()
        .filter(|key| !expected_map.contains_key(*key))
        .cloned()
        .collect::<Vec<_>>();
    let mut changed = Vec::new();
    let mut field_facts = Vec::new();
    if !missing.is_empty() || !extra.is_empty() {
        field_facts.push(FieldEvaluationFact {
            pointer: pointer.to_owned(),
            expected_pointer: Some(expected_pointer.to_owned()),
            status: EvaluationStatus::Failed,
            expected: reference.cloned(),
            actual: actual.cloned(),
            message: format!(
                "Keyed array has {} missing and {} extra identity-matched item(s).",
                missing.len(),
                extra.len()
            ),
        });
    }
    let mut had_error = false;
    for (key, (expected_index, expected_item)) in &expected_map {
        let Some((actual_index, actual_item)) = actual_map.get(key) else {
            continue;
        };
        if field_configs.is_empty() {
            if actual_item != expected_item {
                changed.push(key.clone());
            }
            continue;
        }
        let mut item_changed = false;
        for field in field_configs {
            let actual_value = actual_item.pointer(&field.pointer);
            let expected_value = expected_item.pointer(&field.pointer);
            let (field_status, message) = compare_keyed_field(field, actual_value, expected_value);
            item_changed |= field_status != EvaluationStatus::Passed;
            had_error |= field_status == EvaluationStatus::Error;
            field_facts.push(FieldEvaluationFact {
                pointer: format!("{pointer}/{actual_index}{}", field.pointer),
                expected_pointer: Some(format!(
                    "{expected_pointer}/{expected_index}{}",
                    field.pointer
                )),
                status: field_status,
                expected: expected_value.cloned(),
                actual: actual_value.cloned(),
                message: message.to_owned(),
            });
        }
        if item_changed {
            changed.push(key.clone());
        }
    }
    let status = if missing.is_empty() && extra.is_empty() && changed.is_empty() {
        if had_error {
            EvaluationStatus::Error
        } else {
            EvaluationStatus::Passed
        }
    } else if had_error {
        EvaluationStatus::Error
    } else {
        EvaluationStatus::Failed
    };
    let message = if status == EvaluationStatus::Passed {
        "Keyed array items matched independent of order."
    } else if status == EvaluationStatus::Error {
        "Keyed-array field comparison could not be evaluated reliably."
    } else {
        "Keyed array had missing, extra, or changed items."
    };
    let details = serde_json::json!({"missing": missing, "extra": extra, "changed": changed});
    let result = match status {
        EvaluationStatus::Passed => EvaluatorResult::passed(id, message, details),
        EvaluationStatus::Failed => EvaluatorResult::failed(id, message, details),
        EvaluationStatus::Error => EvaluatorResult::error(id, message),
        EvaluationStatus::NotApplicable => unreachable!(),
    };
    result.with_fields(field_facts)
}

fn compare_keyed_field(
    field: &KeyedArrayField,
    actual: Option<&Value>,
    expected: Option<&Value>,
) -> (EvaluationStatus, &'static str) {
    let Some(expected) = expected else {
        return (
            EvaluationStatus::Error,
            "Expected item field did not resolve.",
        );
    };
    let Some(actual) = actual else {
        return (
            EvaluationStatus::Failed,
            "Output item field did not resolve.",
        );
    };
    let passed = match field.evaluator.as_str() {
        "exact" => actual == expected,
        "normalized_string" => {
            actual
                .as_str()
                .zip(expected.as_str())
                .is_some_and(|(actual, expected)| {
                    normalize_text(actual, field.case_insensitive)
                        == normalize_text(expected, field.case_insensitive)
                })
        }
        "exact_integer" => {
            number_text(actual)
                .zip(number_text(expected))
                .is_some_and(|(actual, expected)| {
                    canonical_integer(actual) == canonical_integer(expected)
                        && canonical_integer(actual).is_some()
                })
        }
        "decimal_tolerance" => decimal_value(Some(actual))
            .zip(decimal_value(Some(expected)))
            .zip(
                field
                    .absolute
                    .as_deref()
                    .and_then(|value| Decimal::from_str(value).ok()),
            )
            .is_some_and(|((actual, expected), tolerance)| (actual - expected).abs() <= tolerance),
        "canonical_date" => {
            actual
                .as_str()
                .zip(expected.as_str())
                .is_some_and(|(actual, expected)| {
                    canonical_date(actual, &field.formats)
                        == canonical_date(expected, &field.formats)
                        && canonical_date(actual, &field.formats).is_some()
                })
        }
        _ => {
            return (
                EvaluationStatus::Error,
                "Keyed-array field evaluator is unsupported.",
            );
        }
    };
    if passed {
        (
            EvaluationStatus::Passed,
            "Matched using the configured item-field comparator.",
        )
    } else {
        (
            EvaluationStatus::Failed,
            "Did not match using the configured item-field comparator.",
        )
    }
}

fn keyed_items<'a>(
    items: &'a [Value],
    keys: &[String],
    fields: &[KeyedArrayField],
) -> Result<BTreeMap<String, (usize, &'a Value)>, ()> {
    let mut indexed = BTreeMap::new();
    for (index, item) in items.iter().enumerate() {
        let identity = keys
            .iter()
            .map(|key| {
                let value = item.pointer(key).ok_or(())?;
                Ok(normalized_key_value(
                    value,
                    fields.iter().find(|field| field.pointer == *key),
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let identity = serde_json::to_string(&identity).map_err(|_| ())?;
        if indexed.insert(identity, (index, item)).is_some() {
            return Err(());
        }
    }
    Ok(indexed)
}

fn normalized_key_value(value: &Value, field: Option<&KeyedArrayField>) -> Value {
    let Some(field) = field else {
        return value.clone();
    };
    match field.evaluator.as_str() {
        "normalized_string" => value.as_str().map_or_else(
            || value.clone(),
            |text| Value::String(normalize_text(text, field.case_insensitive)),
        ),
        "exact_integer" => number_text(value)
            .and_then(canonical_integer)
            .map_or_else(|| value.clone(), Value::String),
        "canonical_date" => value
            .as_str()
            .and_then(|text| canonical_date(text, &field.formats))
            .map_or_else(|| value.clone(), Value::String),
        _ => value.clone(),
    }
}

fn evaluate_financial_invariants(
    id: &str,
    output: &Value,
    line_items_pointer: &str,
    subtotal_pointer: &str,
    tax_pointer: &str,
    total_pointer: &str,
    absolute: &str,
) -> EvaluatorResult {
    let tolerance = match Decimal::from_str(absolute) {
        Ok(value) => value,
        Err(_) => return EvaluatorResult::error(id, "financial tolerance is invalid"),
    };
    let Some(items) = output.pointer(line_items_pointer).and_then(Value::as_array) else {
        return EvaluatorResult::error(id, "line items were missing or not an array");
    };
    let mut fields = Vec::new();
    let mut item_sum = Decimal::ZERO;
    let mut had_error = false;
    for (index, item) in items.iter().enumerate() {
        let quantity = decimal_value(item.pointer("/quantity"));
        let unit_price = decimal_value(item.pointer("/unit_price"));
        let amount = decimal_value(item.pointer("/amount"));
        let pointer = format!("{line_items_pointer}/{index}/amount");
        match (quantity, unit_price, amount) {
            (Some(quantity), Some(unit_price), Some(amount)) => {
                item_sum += amount;
                let passed = (quantity * unit_price - amount).abs() <= tolerance;
                fields.push(FieldEvaluationFact {
                    pointer,
                    expected_pointer: None,
                    status: if passed {
                        EvaluationStatus::Passed
                    } else {
                        EvaluationStatus::Failed
                    },
                    expected: Some(Value::String(
                        (quantity * unit_price).normalize().to_string(),
                    )),
                    actual: item.pointer("/amount").cloned(),
                    message: "Line amount must equal quantity multiplied by unit price.".to_owned(),
                });
            }
            _ => {
                had_error = true;
                fields.push(FieldEvaluationFact {
                    pointer,
                    expected_pointer: None,
                    status: EvaluationStatus::Error,
                    expected: None,
                    actual: item.pointer("/amount").cloned(),
                    message: "Line item financial values must be decimal-compatible.".to_owned(),
                });
            }
        }
    }
    let subtotal = decimal_value(output.pointer(subtotal_pointer));
    let tax = decimal_value(output.pointer(tax_pointer));
    let total = decimal_value(output.pointer(total_pointer));
    if tax.is_none() {
        had_error = true;
        fields.push(FieldEvaluationFact {
            pointer: tax_pointer.to_owned(),
            expected_pointer: None,
            status: EvaluationStatus::Error,
            expected: None,
            actual: output.pointer(tax_pointer).cloned(),
            message: "Required financial value was missing or nonnumeric.".to_owned(),
        });
    }
    for (pointer, actual, expected_value, message) in [
        (
            subtotal_pointer,
            subtotal,
            Some(item_sum),
            "Subtotal must equal the sum of line amounts.",
        ),
        (
            total_pointer,
            total,
            subtotal.zip(tax).map(|(subtotal, tax)| subtotal + tax),
            "Total must equal subtotal plus tax.",
        ),
    ] {
        match (actual, expected_value) {
            (Some(actual), Some(expected)) => fields.push(FieldEvaluationFact {
                pointer: pointer.to_owned(),
                expected_pointer: None,
                status: if (actual - expected).abs() <= tolerance {
                    EvaluationStatus::Passed
                } else {
                    EvaluationStatus::Failed
                },
                expected: Some(Value::String(expected.normalize().to_string())),
                actual: Some(Value::String(actual.normalize().to_string())),
                message: message.to_owned(),
            }),
            _ => {
                had_error = true;
                fields.push(FieldEvaluationFact {
                    pointer: pointer.to_owned(),
                    expected_pointer: None,
                    status: EvaluationStatus::Error,
                    expected: None,
                    actual: output.pointer(pointer).cloned(),
                    message: "Required financial values were missing or nonnumeric.".to_owned(),
                });
            }
        }
    }
    let status = if had_error {
        EvaluationStatus::Error
    } else if fields
        .iter()
        .all(|field| field.status == EvaluationStatus::Passed)
    {
        EvaluationStatus::Passed
    } else {
        EvaluationStatus::Failed
    };
    let message = match status {
        EvaluationStatus::Passed => "All financial invariants held.",
        EvaluationStatus::Failed => "One or more financial invariants failed.",
        EvaluationStatus::Error => "Financial invariants could not be evaluated reliably.",
        EvaluationStatus::NotApplicable => unreachable!(),
    };
    EvaluatorResult {
        evaluator_id: id.to_owned(),
        status,
        passed: status == EvaluationStatus::Passed,
        score: Some(if status == EvaluationStatus::Passed {
            1.0
        } else {
            0.0
        }),
        message: message.to_owned(),
        details: serde_json::json!({"tolerance": absolute}),
        fields,
    }
}

fn decimal_value(value: Option<&Value>) -> Option<Decimal> {
    value.and_then(|value| match value {
        Value::String(value) => Decimal::from_str(value).ok(),
        Value::Number(value) => Decimal::from_str(&value.to_string()).ok(),
        _ => None,
    })
}

#[allow(clippy::too_many_arguments)]
fn field_result(
    id: &str,
    status: EvaluationStatus,
    pointer: &str,
    expected_pointer: &str,
    actual: Option<&Value>,
    expected: Option<&Value>,
    message: &str,
    details: Value,
) -> EvaluatorResult {
    EvaluatorResult {
        evaluator_id: id.to_owned(),
        status,
        passed: status == EvaluationStatus::Passed,
        score: Some(if status == EvaluationStatus::Passed {
            1.0
        } else {
            0.0
        }),
        message: message.to_owned(),
        details,
        fields: vec![FieldEvaluationFact {
            pointer: pointer.to_owned(),
            expected_pointer: Some(expected_pointer.to_owned()),
            status,
            expected: expected.cloned(),
            actual: actual.cloned(),
            message: message.to_owned(),
        }],
    }
}

fn field_error(
    id: &str,
    output: &Value,
    pointer: &str,
    expected_pointer: &str,
    message: &str,
) -> EvaluatorResult {
    field_result(
        id,
        EvaluationStatus::Error,
        pointer,
        expected_pointer,
        output.pointer(pointer),
        None,
        message,
        Value::Null,
    )
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
    let field = |status: EvaluationStatus,
                 actual: Option<&Value>,
                 reference: Option<&Value>,
                 message: &str| {
        vec![FieldEvaluationFact {
            pointer: pointer.to_owned(),
            expected_pointer: Some(expected_pointer.to_owned()),
            status,
            expected: reference.cloned(),
            actual: actual.cloned(),
            message: message.to_owned(),
        }]
    };
    let Some(expected) = expected else {
        return EvaluatorResult::error(id, "case has no expected value").with_fields(field(
            EvaluationStatus::Error,
            output.pointer(pointer),
            None,
            "Case has no expected value.",
        ));
    };
    let Some(actual_value) = output.pointer(pointer) else {
        return EvaluatorResult::failed(
            id,
            format!("Output pointer {pointer} did not resolve."),
            Value::Null,
        )
        .with_fields(field(
            EvaluationStatus::Failed,
            None,
            expected.pointer(expected_pointer),
            "Output pointer did not resolve.",
        ));
    };
    let Some(expected_value) = expected.pointer(expected_pointer) else {
        return EvaluatorResult::error(
            id,
            format!("expected pointer {expected_pointer} did not resolve"),
        )
        .with_fields(field(
            EvaluationStatus::Error,
            Some(actual_value),
            None,
            "Expected pointer did not resolve.",
        ));
    };
    let actual_text = number_text(actual_value);
    let expected_text = number_text(expected_value);
    let (Some(actual_text), Some(expected_text)) = (actual_text, expected_text) else {
        return EvaluatorResult::failed(
            id,
            "Numeric evaluator received a non-numeric value.",
            serde_json::json!({"actual": actual_value, "expected": expected_value}),
        )
        .with_fields(field(
            EvaluationStatus::Failed,
            Some(actual_value),
            Some(expected_value),
            "Numeric evaluator received a non-numeric value.",
        ));
    };
    if exact_integer {
        let actual = canonical_integer(actual_text);
        let reference = canonical_integer(expected_text);
        return match (actual, reference) {
            (Some(actual), Some(reference)) if actual == reference => EvaluatorResult::passed(
                id,
                "Integer values matched exactly.",
                serde_json::json!({"value": actual}),
            )
            .with_fields(field(
                EvaluationStatus::Passed,
                Some(actual_value),
                Some(expected_value),
                "Integer values matched exactly.",
            )),
            (Some(actual), Some(reference)) => EvaluatorResult::failed(
                id,
                "Integer values did not match exactly.",
                serde_json::json!({"actual": actual, "expected": reference}),
            )
            .with_fields(field(
                EvaluationStatus::Failed,
                Some(actual_value),
                Some(expected_value),
                "Integer values did not match exactly.",
            )),
            _ => EvaluatorResult::failed(
                id,
                "Exact-integer comparison requires integer values.",
                Value::Null,
            )
            .with_fields(field(
                EvaluationStatus::Failed,
                Some(actual_value),
                Some(expected_value),
                "Exact-integer comparison requires integer values.",
            )),
        };
    }
    let actual = Decimal::from_str(actual_text);
    let reference = Decimal::from_str(expected_text);
    let (Ok(actual), Ok(reference)) = (actual, reference) else {
        return EvaluatorResult::failed(
            id,
            "Values could not be represented as exact decimals.",
            Value::Null,
        )
        .with_fields(field(
            EvaluationStatus::Failed,
            Some(actual_value),
            Some(expected_value),
            "Values could not be represented as exact decimals.",
        ));
    };
    let absolute_tolerance = absolute.and_then(|value| Decimal::from_str(value).ok());
    let relative_tolerance = relative.and_then(|value| Decimal::from_str(value).ok());
    if absolute.is_some() && absolute_tolerance.is_none()
        || relative.is_some() && relative_tolerance.is_none()
    {
        return EvaluatorResult::error(id, "configured tolerance is not a valid decimal")
            .with_fields(field(
                EvaluationStatus::Error,
                Some(actual_value),
                Some(expected_value),
                "Configured tolerance is not a valid decimal.",
            ));
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
        .with_fields(field(
            EvaluationStatus::Passed,
            Some(actual_value),
            Some(expected_value),
            "Numeric value was within the configured tolerance.",
        ))
    } else {
        EvaluatorResult::failed(
            id,
            "Numeric value exceeded the configured tolerance.",
            serde_json::json!({"difference": difference.to_string()}),
        )
        .with_fields(field(
            EvaluationStatus::Failed,
            Some(actual_value),
            Some(expected_value),
            "Numeric value exceeded the configured tolerance.",
        ))
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
) -> OutcomeResult {
    let ids = if config.all_of.is_empty() {
        &config.any_of
    } else {
        &config.all_of
    };
    let selected = ids
        .iter()
        .filter_map(|id| results.get(id))
        .collect::<Vec<_>>();
    let passed_components = selected
        .iter()
        .filter(|result| result.status == EvaluationStatus::Passed)
        .count();
    let failed_components = selected
        .iter()
        .filter(|result| result.status == EvaluationStatus::Failed)
        .count();
    let error_components = selected
        .iter()
        .filter(|result| result.status == EvaluationStatus::Error)
        .count();
    let not_applicable_components = selected
        .iter()
        .filter(|result| result.status == EvaluationStatus::NotApplicable)
        .count();
    let unscored_components = ids.len().saturating_sub(selected.len());
    let truth = if unscored_components > 0 {
        OutcomeStatus::Error
    } else if !config.all_of.is_empty() {
        if selected
            .iter()
            .any(|result| result.status == EvaluationStatus::Failed)
        {
            OutcomeStatus::False
        } else if selected
            .iter()
            .any(|result| result.status == EvaluationStatus::Error)
        {
            OutcomeStatus::Error
        } else if selected
            .iter()
            .any(|result| result.status == EvaluationStatus::NotApplicable)
        {
            OutcomeStatus::NotApplicable
        } else {
            OutcomeStatus::True
        }
    } else if selected
        .iter()
        .any(|result| result.status == EvaluationStatus::Passed)
    {
        OutcomeStatus::True
    } else if selected
        .iter()
        .any(|result| result.status == EvaluationStatus::Error)
    {
        OutcomeStatus::Error
    } else if selected
        .iter()
        .all(|result| result.status == EvaluationStatus::NotApplicable)
    {
        OutcomeStatus::NotApplicable
    } else {
        OutcomeStatus::False
    };
    OutcomeResult {
        truth,
        fully_evaluated: error_components == 0
            && not_applicable_components == 0
            && unscored_components == 0,
        required_components: ids.len(),
        passed_components,
        failed_components,
        error_components,
        not_applicable_components,
        unscored_components,
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

    #[test]
    fn any_of_true_dominates_error_and_truth_table_is_explicit() {
        let config = OutcomeConfig {
            all_of: Vec::new(),
            any_of: vec!["a".to_owned(), "b".to_owned()],
        };
        let result = |id: &str, status| EvaluatorResult {
            evaluator_id: id.to_owned(),
            status,
            passed: status == EvaluationStatus::Passed,
            score: None,
            message: String::new(),
            details: Value::Null,
            fields: Vec::new(),
        };
        for (left, right, expected) in [
            (
                EvaluationStatus::Passed,
                EvaluationStatus::Error,
                OutcomeStatus::True,
            ),
            (
                EvaluationStatus::Failed,
                EvaluationStatus::Error,
                OutcomeStatus::Error,
            ),
            (
                EvaluationStatus::Failed,
                EvaluationStatus::NotApplicable,
                OutcomeStatus::False,
            ),
            (
                EvaluationStatus::NotApplicable,
                EvaluationStatus::NotApplicable,
                OutcomeStatus::NotApplicable,
            ),
        ] {
            let results = BTreeMap::from([
                ("a".to_owned(), result("a", left)),
                ("b".to_owned(), result("b", right)),
            ]);
            assert_eq!(compose_outcome(&config, &results).truth, expected);
        }
    }

    #[test]
    fn all_of_false_plus_error_preserves_error_component() {
        let config = OutcomeConfig {
            all_of: vec!["known_failure".to_owned(), "crashed".to_owned()],
            any_of: Vec::new(),
        };
        let results = BTreeMap::from([
            (
                "known_failure".to_owned(),
                EvaluatorResult::failed("known_failure", "wrong", Value::Null),
            ),
            (
                "crashed".to_owned(),
                EvaluatorResult::error("crashed", "unavailable"),
            ),
        ]);
        let outcome = compose_outcome(&config, &results);
        assert_eq!(outcome.truth, OutcomeStatus::False);
        assert!(!outcome.fully_evaluated);
        assert_eq!(outcome.failed_components, 1);
        assert_eq!(outcome.error_components, 1);
    }

    #[test]
    fn all_of_false_plus_not_applicable_is_not_fully_evaluated() {
        let config = OutcomeConfig {
            all_of: vec!["failed".to_owned(), "na".to_owned()],
            any_of: Vec::new(),
        };
        let mut not_applicable = EvaluatorResult::error("na", "not applicable");
        not_applicable.status = EvaluationStatus::NotApplicable;
        let results = BTreeMap::from([
            (
                "failed".to_owned(),
                EvaluatorResult::failed("failed", "wrong", Value::Null),
            ),
            ("na".to_owned(), not_applicable),
        ]);
        let outcome = compose_outcome(&config, &results);
        assert_eq!(outcome.truth, OutcomeStatus::False);
        assert!(!outcome.fully_evaluated);
        assert_eq!(outcome.not_applicable_components, 1);
    }

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

    #[test]
    fn duplicate_top_level_and_nested_raw_output_keys_are_rejected() {
        assert!(parse_strict(r#"{"value":1,"value":2}"#).is_err());
        assert!(parse_strict(r#"{"outer":{"value":1,"value":2}}"#).is_err());
    }

    #[test]
    fn json_schema_date_format_is_enforced() {
        let schema = compile_schema(&json!({"type": "string", "format": "date"})).unwrap();
        assert!(schema.is_valid(&json!("2026-08-09")));
        assert!(!schema.is_valid(&json!("2026-02-30")));
        assert!(!schema.is_valid(&json!("09/08/2026")));
    }

    #[test]
    fn numeric_evaluator_emits_pointer_level_failure_evidence() {
        let result = evaluate_numeric(
            "total",
            &json!({"total": "11.00"}),
            Some(&json!({"total": "10.00"})),
            "/total",
            "/total",
            Some("0.01"),
            None,
            false,
        );
        assert_eq!(result.status, EvaluationStatus::Failed);
        assert_eq!(result.fields.len(), 1);
        assert_eq!(result.fields[0].pointer, "/total");
        assert_eq!(result.fields[0].status, EvaluationStatus::Failed);
    }

    #[test]
    fn normalized_strings_handle_unicode_case_and_whitespace() {
        let result = evaluate_normalized_string(
            "vendor",
            &json!({"vendor": "  CAFÉ\nSUPPLIES "}),
            Some(&json!({"vendor": "cafe\u{301} supplies"})),
            "/vendor",
            "/vendor",
            true,
        );
        assert_eq!(result.status, EvaluationStatus::Passed);
    }

    #[test]
    fn canonical_dates_accept_declared_formats_and_reject_impossible_dates() {
        let formats = vec!["iso".to_owned(), "dmy_slash".to_owned()];
        assert_eq!(
            canonical_date("09/08/2026", &formats).as_deref(),
            Some("2026-08-09")
        );
        assert_eq!(canonical_date("2026-02-30", &formats), None);
    }

    #[test]
    fn keyed_arrays_report_missing_items_without_order_sensitivity() {
        let keys = vec!["/sku".to_owned()];
        let reordered = evaluate_keyed_array(
            "items",
            &json!({"items": [{"sku": "B", "qty": 2}, {"sku": "A", "qty": 1}]}),
            Some(&json!({"items": [{"sku": "A", "qty": 1}, {"sku": "B", "qty": 2}]})),
            "/items",
            "/items",
            &keys,
            &[],
        );
        assert_eq!(reordered.status, EvaluationStatus::Passed);
        let missing = evaluate_keyed_array(
            "items",
            &json!({"items": [{"sku": "A", "qty": 1}]}),
            Some(&json!({"items": [{"sku": "A", "qty": 1}, {"sku": "B", "qty": 2}]})),
            "/items",
            "/items",
            &keys,
            &[],
        );
        assert_eq!(missing.status, EvaluationStatus::Failed);
        assert_eq!(missing.details["missing"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn keyed_arrays_apply_field_specific_invoice_semantics() {
        let keys = vec!["/description".to_owned()];
        let fields = vec![
            KeyedArrayField {
                pointer: "/description".to_owned(),
                evaluator: "normalized_string".to_owned(),
                absolute: None,
                case_insensitive: true,
                formats: vec!["iso".to_owned()],
            },
            KeyedArrayField {
                pointer: "/quantity".to_owned(),
                evaluator: "exact_integer".to_owned(),
                absolute: None,
                case_insensitive: true,
                formats: vec!["iso".to_owned()],
            },
            KeyedArrayField {
                pointer: "/amount".to_owned(),
                evaluator: "decimal_tolerance".to_owned(),
                absolute: Some("0.01".to_owned()),
                case_insensitive: true,
                formats: vec!["iso".to_owned()],
            },
        ];
        let result = evaluate_keyed_array(
            "items",
            &json!({"items": [{"description": " Widget ", "quantity": "01", "amount": "10.0"}]}),
            Some(&json!({"items": [{"description": "widget", "quantity": 1, "amount": "10.00"}]})),
            "/items",
            "/items",
            &keys,
            &fields,
        );
        assert_eq!(result.status, EvaluationStatus::Passed);
        assert_eq!(result.fields.len(), 3);
        assert!(
            result
                .fields
                .iter()
                .all(|field| field.status == EvaluationStatus::Passed)
        );
    }

    #[test]
    fn financial_invariants_attribute_failures_to_exact_paths() {
        let base = json!({
            "line_items": [{"quantity": "2", "unit_price": "5.00", "amount": "10.00"}],
            "subtotal": "10.00", "tax": "1.00", "total": "11.00"
        });
        let evaluate = |value: &Value| {
            evaluate_financial_invariants(
                "financial",
                value,
                "/line_items",
                "/subtotal",
                "/tax",
                "/total",
                "0.01",
            )
        };

        let mut wrong_amount = base.clone();
        wrong_amount["line_items"][0]["amount"] = json!("9.00");
        let result = evaluate(&wrong_amount);
        let fact = result
            .fields
            .iter()
            .find(|fact| fact.pointer == "/line_items/0/amount")
            .unwrap();
        assert_eq!(fact.status, EvaluationStatus::Failed);
        assert_eq!(fact.expected, Some(json!("10")));
        assert_eq!(fact.actual, Some(json!("9.00")));
        assert_eq!(
            fact.message,
            "Line amount must equal quantity multiplied by unit price."
        );

        let mut wrong_subtotal = base.clone();
        wrong_subtotal["subtotal"] = json!("12.00");
        let result = evaluate(&wrong_subtotal);
        let fact = result
            .fields
            .iter()
            .find(|fact| fact.pointer == "/subtotal")
            .unwrap();
        assert_eq!(fact.status, EvaluationStatus::Failed);
        assert_eq!(fact.expected, Some(json!("10")));
        assert_eq!(fact.actual, Some(json!("12")));
        assert_eq!(fact.message, "Subtotal must equal the sum of line amounts.");

        let mut wrong_total = base.clone();
        wrong_total["total"] = json!("12.00");
        let result = evaluate(&wrong_total);
        let fact = result
            .fields
            .iter()
            .find(|fact| fact.pointer == "/total")
            .unwrap();
        assert_eq!(fact.status, EvaluationStatus::Failed);
        assert_eq!(fact.expected, Some(json!("11")));
        assert_eq!(fact.actual, Some(json!("12")));

        let mut missing_tax = base.clone();
        missing_tax.as_object_mut().unwrap().remove("tax");
        let result = evaluate(&missing_tax);
        let fact = result
            .fields
            .iter()
            .find(|fact| fact.pointer == "/tax")
            .unwrap();
        assert_eq!(fact.status, EvaluationStatus::Error);
        assert_eq!(fact.actual, None);

        let mut nonnumeric_subtotal = base;
        nonnumeric_subtotal["subtotal"] = json!("not-a-number");
        let result = evaluate(&nonnumeric_subtotal);
        let fact = result
            .fields
            .iter()
            .find(|fact| fact.pointer == "/subtotal")
            .unwrap();
        assert_eq!(fact.status, EvaluationStatus::Error);
        assert_eq!(fact.actual, Some(json!("not-a-number")));
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
            model_visible_metadata: None,
            metadata: None,
            source_line: 1,
        };
        let evaluators = vec![EvaluatorConfig {
            id: "priority".to_owned(),
            implementation_version: None,
            implementation: Default::default(),
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

    fn pointer_pairs() -> Vec<PointerPair> {
        vec![
            PointerPair {
                pointer: "/a".to_owned(),
                expected_pointer: "/a".to_owned(),
            },
            PointerPair {
                pointer: "/b".to_owned(),
                expected_pointer: "/b".to_owned(),
            },
        ]
    }

    #[test]
    fn pointer_list_missing_both_is_not_pass() {
        let result = compare_pointer_list("fields", &json!({}), Some(&json!({})), &pointer_pairs());
        assert_eq!(result.status, EvaluationStatus::Error);
        assert!(!result.passed);
    }

    #[test]
    fn pointer_list_missing_expected_is_error() {
        let result = compare_pointer_list(
            "fields",
            &json!({"a": 1, "b": 2}),
            Some(&json!({"a": 1})),
            &pointer_pairs(),
        );
        assert_eq!(result.status, EvaluationStatus::Error);
        assert_eq!(result.fields[1].status, EvaluationStatus::Error);
    }

    #[test]
    fn pointer_list_missing_output_is_failure() {
        let result = compare_pointer_list(
            "fields",
            &json!({"a": 1}),
            Some(&json!({"a": 1, "b": 2})),
            &pointer_pairs(),
        );
        assert_eq!(result.status, EvaluationStatus::Failed);
        assert_eq!(result.fields[1].status, EvaluationStatus::Failed);
    }

    #[test]
    fn pointer_list_compares_only_resolved_values() {
        let result = compare_pointer_list(
            "fields",
            &json!({"a": 1, "b": 9}),
            Some(&json!({"a": 1, "b": 2})),
            &pointer_pairs(),
        );
        assert_eq!(result.status, EvaluationStatus::Failed);
        assert_eq!(result.fields[0].status, EvaluationStatus::Passed);
        assert_eq!(result.fields[1].status, EvaluationStatus::Failed);
        assert_eq!(result.fields[1].actual, Some(json!(9)));
        assert_eq!(result.fields[1].expected, Some(json!(2)));
    }

    fn valid_output_with_primary_status(status: EvaluationStatus) -> CaseEvaluation {
        let schema = compile_schema(&json!({"type": "object"})).unwrap();
        let case = Case {
            id: "status".to_owned(),
            input: Value::Null,
            expected: None,
            model_visible_metadata: None,
            metadata: None,
            source_line: 1,
        };
        let evaluators = vec![EvaluatorConfig {
            id: "external".to_owned(),
            implementation_version: Some("test-v1".to_owned()),
            implementation: Default::default(),
            kind: EvaluatorKind::Command {
                command: crate::config::CommandSpec {
                    program: "unused".to_owned(),
                    args: vec![],
                },
                process_mode: crate::config::ProcessMode::Persistent,
                timeout_ms: 1000,
            },
        }];
        let outcomes = BTreeMap::from([(
            "semantic".to_owned(),
            OutcomeConfig {
                all_of: vec!["external".to_owned()],
                any_of: vec![],
            },
        )]);
        let external = BTreeMap::from([(
            "external".to_owned(),
            EvaluatorResult {
                evaluator_id: "external".to_owned(),
                status,
                passed: status == EvaluationStatus::Passed,
                score: None,
                message: "fixture".to_owned(),
                details: Value::Null,
                fields: vec![],
            },
        )]);
        evaluate_case_with_external(
            &case,
            &output("{}"),
            &schema,
            &evaluators,
            &outcomes,
            "semantic",
            &external,
        )
    }

    #[test]
    fn evaluator_error_is_not_valid_but_wrong() {
        assert!(!valid_output_with_primary_status(EvaluationStatus::Error).valid_but_wrong);
    }

    #[test]
    fn not_applicable_is_not_valid_but_wrong() {
        assert!(!valid_output_with_primary_status(EvaluationStatus::NotApplicable).valid_but_wrong);
    }

    #[test]
    fn semantic_false_is_valid_but_wrong() {
        assert!(valid_output_with_primary_status(EvaluationStatus::Failed).valid_but_wrong);
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

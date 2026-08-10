//! Recorded-output envelope and exact ID matching.

use std::{collections::HashMap, path::Path, str::FromStr};

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    CoreError, Result,
    config::LimitsConfig,
    dataset::Dataset,
    hashing::{hash_bytes, read_bounded},
    strict_json,
};

/// Success or retained failure.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutputStatus {
    /// Adapter returned an output.
    Ok,
    /// Adapter failed; the case remains in the denominator.
    Error,
    /// No output row was present for a known dataset case.
    Missing,
}

/// Adapter failure details.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutputError {
    /// Stable machine-readable failure category.
    pub kind: String,
    /// Redaction-safe explanation.
    pub message: String,
    /// Stable, redaction-safe error-class fingerprint when supplied by an adapter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
}

/// Optional provider token accounting.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Usage {
    /// Prompt or input tokens.
    #[serde(default)]
    pub input_tokens: Option<u64>,
    /// Completion or output tokens.
    #[serde(default)]
    pub output_tokens: Option<u64>,
}

/// Optional user-priced cost.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Cost {
    /// Exact decimal text.
    pub amount: String,
    /// User-declared currency.
    pub currency: String,
}

/// One recorded or adapter-produced case result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct VariantOutput {
    /// Dataset case ID.
    pub case_id: String,
    /// Success, retained error, or materialized missing row.
    pub status: OutputStatus,
    /// Source of truth for strict parsing when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_output: Option<String>,
    /// Optional convenience object. It never overrides invalid raw text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parsed_output: Option<Value>,
    /// Error envelope for unsuccessful cases.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<OutputError>,
    /// End-to-end adapter latency.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    /// Provider usage when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    /// User-priced cost when configured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<Cost>,
    /// Redaction-safe adapter metadata.
    #[serde(default)]
    pub metadata: Value,
    /// Retry attempts retained for observability.
    #[serde(default)]
    pub retries: Vec<Value>,
}

impl VariantOutput {
    /// Create a complete-denominator row for a missing output.
    pub fn missing(case_id: String) -> Self {
        Self {
            case_id,
            status: OutputStatus::Missing,
            raw_output: None,
            parsed_output: None,
            error: Some(OutputError {
                kind: "missing_output".to_owned(),
                message: "No output row was supplied for this dataset case.".to_owned(),
                fingerprint: None,
            }),
            latency_ms: None,
            usage: None,
            cost: None,
            metadata: Value::Object(Default::default()),
            retries: Vec::new(),
        }
    }

    /// Return parse source text, preferring retained raw output.
    pub fn parse_source(&self) -> Option<String> {
        self.raw_output.clone().or_else(|| {
            self.parsed_output
                .as_ref()
                .and_then(|value| serde_json::to_string(value).ok())
        })
    }
}

/// Matched output rows plus the exact input artifact hash.
#[derive(Debug, Clone)]
pub struct RecordedOutputs {
    /// One row per dataset case in dataset order.
    pub rows: Vec<VariantOutput>,
    /// Hash of exact JSONL source bytes.
    pub source_hash: String,
    /// Exact bounded source bytes used for parsing and retention.
    pub source_bytes: Vec<u8>,
}

impl RecordedOutputs {
    /// Read a JSONL envelope and match it to the complete dataset denominator.
    pub fn read(path: &Path, dataset: &Dataset) -> Result<Self> {
        Self::read_bounded(path, dataset, &LimitsConfig::default())
    }

    /// Read recorded outputs under configured artifact and JSONL-line ceilings.
    pub fn read_bounded(path: &Path, dataset: &Dataset, limits: &LimitsConfig) -> Result<Self> {
        let bytes = read_bounded(path, limits.max_recorded_output_bytes, "recorded output")?;
        Self::from_bytes_bounded(&bytes, dataset, limits)
    }

    /// Parse already-captured canonical recorded-output JSONL under explicit limits.
    pub fn from_bytes_bounded(
        bytes: &[u8],
        dataset: &Dataset,
        limits: &LimitsConfig,
    ) -> Result<Self> {
        if bytes.len() > limits.max_recorded_output_bytes {
            return Err(CoreError::RecordedOutput {
                line: 1,
                message: format!(
                    "recorded output is {} bytes; limit is {}",
                    bytes.len(),
                    limits.max_recorded_output_bytes
                ),
            });
        }
        let text = std::str::from_utf8(bytes).map_err(|error| CoreError::RecordedOutput {
            line: 1,
            message: format!("output file is not valid UTF-8: {error}"),
        })?;
        let known = dataset
            .cases
            .iter()
            .map(|case| case.id.as_str())
            .collect::<std::collections::HashSet<_>>();
        let mut by_id = HashMap::new();
        for (index, line) in text.lines().enumerate() {
            let line_number = index + 1;
            if line.len() > limits.max_jsonl_line_bytes {
                return Err(CoreError::RecordedOutput {
                    line: line_number,
                    message: format!(
                        "JSONL line is {} bytes; limit is {}",
                        line.len(),
                        limits.max_jsonl_line_bytes
                    ),
                });
            }
            if line.trim().is_empty() {
                return Err(CoreError::RecordedOutput {
                    line: line_number,
                    message: "blank lines are not valid outputs".to_owned(),
                });
            }
            let row: VariantOutput =
                strict_json::from_str(line).map_err(|error| CoreError::RecordedOutput {
                    line: line_number,
                    message: error.to_string(),
                })?;
            if row.case_id.trim().is_empty() {
                return Err(CoreError::RecordedOutput {
                    line: line_number,
                    message: "case_id must not be empty".to_owned(),
                });
            }
            if !known.contains(row.case_id.as_str()) {
                return Err(CoreError::RecordedOutput {
                    line: line_number,
                    message: format!("unknown case ID `{}`", row.case_id),
                });
            }
            if row.status == OutputStatus::Ok && row.parse_source().is_none() {
                return Err(CoreError::RecordedOutput {
                    line: line_number,
                    message: "successful output requires raw_output or parsed_output".to_owned(),
                });
            }
            match row.status {
                OutputStatus::Ok => {
                    if row.error.is_some() {
                        return Err(CoreError::RecordedOutput {
                            line: line_number,
                            message: "successful output must not contain error".to_owned(),
                        });
                    }
                    if let (Some(raw), Some(parsed)) = (&row.raw_output, &row.parsed_output) {
                        let raw_value = strict_json::value_from_str(raw).map_err(|error| {
                            CoreError::RecordedOutput {
                                line: line_number,
                                message: format!(
                                    "raw_output conflicts with parsed_output: {error}"
                                ),
                            }
                        })?;
                        if &raw_value != parsed {
                            return Err(CoreError::RecordedOutput {
                                line: line_number,
                                message:
                                    "raw_output and parsed_output represent different JSON values"
                                        .to_owned(),
                            });
                        }
                    }
                }
                OutputStatus::Error | OutputStatus::Missing => {
                    if row.error.is_none()
                        || row.raw_output.is_some()
                        || row.parsed_output.is_some()
                    {
                        return Err(CoreError::RecordedOutput {
                            line: line_number,
                            message:
                                "error or missing output requires error and forbids output values"
                                    .to_owned(),
                        });
                    }
                }
            }
            if let Some(cost) = &row.cost {
                let amount =
                    Decimal::from_str(&cost.amount).map_err(|_| CoreError::RecordedOutput {
                        line: line_number,
                        message: "cost amount must be a valid decimal".to_owned(),
                    })?;
                if amount.is_sign_negative() || cost.currency.trim().is_empty() {
                    return Err(CoreError::RecordedOutput {
                        line: line_number,
                        message: "cost amount must be non-negative and currency non-empty"
                            .to_owned(),
                    });
                }
            }
            let row_id = row.case_id.clone();
            if by_id.insert(row_id.clone(), row).is_some() {
                return Err(CoreError::RecordedOutput {
                    line: line_number,
                    message: format!("duplicate output for case ID `{row_id}`"),
                });
            }
        }
        let rows = dataset
            .cases
            .iter()
            .map(|case| {
                by_id
                    .remove(&case.id)
                    .unwrap_or_else(|| VariantOutput::missing(case.id.clone()))
            })
            .collect();
        Ok(Self {
            rows,
            source_hash: hash_bytes(bytes),
            source_bytes: bytes.to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use proptest::prelude::*;
    use tempfile::NamedTempFile;

    use crate::{config::DatasetFields, dataset::Dataset};

    use super::*;

    #[test]
    fn materializes_missing_rows_without_shrinking_denominator() {
        let mut dataset_file = NamedTempFile::new().unwrap();
        writeln!(dataset_file, r#"{{"id":"a","input":1}}"#).unwrap();
        writeln!(dataset_file, r#"{{"id":"b","input":2}}"#).unwrap();
        let dataset = Dataset::read(dataset_file.path(), &DatasetFields::default()).unwrap();
        let mut output_file = NamedTempFile::new().unwrap();
        writeln!(
            output_file,
            r#"{{"case_id":"a","status":"ok","raw_output":"1"}}"#
        )
        .unwrap();
        let outputs = RecordedOutputs::read(output_file.path(), &dataset).unwrap();
        assert_eq!(outputs.rows.len(), 2);
        assert_eq!(outputs.rows[1].status, OutputStatus::Missing);
    }

    #[test]
    fn invalid_utf8_recorded_output_fails_before_matching() {
        let mut dataset_file = NamedTempFile::new().unwrap();
        writeln!(dataset_file, r#"{{"id":"a","input":1}}"#).unwrap();
        let dataset = Dataset::read(dataset_file.path(), &DatasetFields::default()).unwrap();
        let mut output_file = NamedTempFile::new().unwrap();
        output_file.write_all(&[0xff, b'\n']).unwrap();
        let error = RecordedOutputs::read(output_file.path(), &dataset).unwrap_err();
        assert!(error.to_string().contains("not valid UTF-8"));
    }

    #[test]
    fn duplicate_recorded_output_field_is_rejected() {
        let mut dataset_file = NamedTempFile::new().unwrap();
        writeln!(dataset_file, r#"{{"id":"a","input":1}}"#).unwrap();
        let dataset = Dataset::read(dataset_file.path(), &DatasetFields::default()).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("outputs.jsonl");
        std::fs::write(
            &path,
            "{\"case_id\":\"a\",\"case_id\":\"b\",\"status\":\"ok\",\"raw_output\":\"{}\"}\n",
        )
        .unwrap();
        let error = RecordedOutputs::read(&path, &dataset)
            .unwrap_err()
            .to_string();
        assert!(error.contains("duplicate object key `case_id`"));
    }

    proptest! {
        #[test]
        fn output_envelopes_round_trip_without_semantic_change(
            case_id in "[A-Za-z0-9_-]{1,64}",
            raw_output in ".{0,256}",
            latency_ms in any::<u64>(),
            input_tokens in any::<u32>(),
            output_tokens in any::<u32>(),
        ) {
            let output = VariantOutput {
                case_id,
                status: OutputStatus::Ok,
                raw_output: Some(raw_output),
                parsed_output: None,
                error: None,
                latency_ms: Some(latency_ms),
                usage: Some(Usage {
                    input_tokens: Some(u64::from(input_tokens)),
                    output_tokens: Some(u64::from(output_tokens)),
                }),
                cost: None,
                metadata: serde_json::json!({"source": "property-test"}),
                retries: vec![],
            };
            let encoded = serde_json::to_vec(&output).unwrap();
            let decoded: VariantOutput = serde_json::from_slice(&encoded).unwrap();
            prop_assert_eq!(decoded, output);
        }
    }
}

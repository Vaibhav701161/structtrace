//! Matched JSONL dataset ingestion.

use std::{collections::HashSet, path::Path};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    CoreError, Result,
    config::{DatasetFields, LimitsConfig},
    hashing::{hash_bytes, read_bounded},
};

/// One immutable matched case retained inside the evaluation boundary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Case {
    /// Non-empty unique identifier.
    pub id: String,
    /// Input supplied to both variants.
    pub input: Value,
    /// Optional reference used by deterministic evaluators.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<Value>,
    /// Optional metadata that may be shown to the implementation under test.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_visible_metadata: Option<Value>,
    /// Optional evaluator-only tags or metadata. This never crosses a variant
    /// adapter boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    /// Original one-based source line.
    pub source_line: usize,
}

/// The deliberately restricted case view supplied to an implementation.
///
/// Expected values and evaluator-only metadata are structurally absent, so an
/// adapter cannot leak them accidentally through serialization or templates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VariantCase {
    /// Opaque transport token. This is never the dataset case ID.
    pub id: String,
    /// Model or application input.
    pub input: Value,
    /// Explicitly model-visible metadata only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

impl From<&Case> for VariantCase {
    fn from(case: &Case) -> Self {
        Self {
            id: format!("stx-{}", &hash_bytes(case.id.as_bytes())[..24]),
            input: case.input.clone(),
            metadata: case.model_visible_metadata.clone(),
        }
    }
}

/// Parsed cases plus the hash of exact source bytes.
#[derive(Debug, Clone)]
pub struct Dataset {
    /// Cases in original display and execution order.
    pub cases: Vec<Case>,
    /// BLAKE3 digest of exact source bytes.
    pub source_hash: String,
    /// Exact immutable source bytes captured at ingestion.
    pub source_bytes: Vec<u8>,
}

impl Dataset {
    /// Read, validate, and preserve a JSONL dataset.
    pub fn read(path: &Path, fields: &DatasetFields) -> Result<Self> {
        Self::read_bounded(path, fields, &LimitsConfig::default())
    }

    /// Read a dataset under configured total, line, and case-count ceilings.
    pub fn read_bounded(
        path: &Path,
        fields: &DatasetFields,
        limits: &LimitsConfig,
    ) -> Result<Self> {
        let bytes = read_bounded(path, limits.max_dataset_bytes, "dataset")?;
        Self::from_bytes_bounded(
            &bytes,
            fields,
            limits.max_jsonl_line_bytes,
            limits.max_cases,
        )
    }

    /// Parse already-captured bytes so execution and finalization use one
    /// immutable dataset snapshot.
    pub fn from_bytes(bytes: &[u8], fields: &DatasetFields) -> Result<Self> {
        let limits = LimitsConfig::default();
        Self::from_bytes_bounded(bytes, fields, limits.max_jsonl_line_bytes, limits.max_cases)
    }

    /// Parse immutable dataset bytes under line and case-count ceilings.
    pub fn from_bytes_bounded(
        bytes: &[u8],
        fields: &DatasetFields,
        max_line_bytes: usize,
        max_cases: usize,
    ) -> Result<Self> {
        let text = std::str::from_utf8(bytes).map_err(|error| CoreError::Dataset {
            line: 1,
            message: format!("dataset is not valid UTF-8: {error}"),
        })?;
        let mut cases = Vec::new();
        let mut ids = HashSet::new();
        for (index, line) in text.lines().enumerate() {
            let line_number = index + 1;
            if line.len() > max_line_bytes {
                return Err(CoreError::Dataset {
                    line: line_number,
                    message: format!(
                        "JSONL line is {} bytes; limit is {max_line_bytes}",
                        line.len()
                    ),
                });
            }
            if cases.len() >= max_cases {
                return Err(CoreError::Dataset {
                    line: line_number,
                    message: format!("dataset exceeds the configured {max_cases}-case limit"),
                });
            }
            if line.trim().is_empty() {
                return Err(CoreError::Dataset {
                    line: line_number,
                    message: "blank lines are not valid cases".to_owned(),
                });
            }
            let row: Value = serde_json::from_str(line).map_err(|error| CoreError::Dataset {
                line: line_number,
                message: error.to_string(),
            })?;
            let id = row
                .pointer(&fields.id)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| CoreError::Dataset {
                    line: line_number,
                    message: format!("{} must point to a non-empty string", fields.id),
                })?
                .to_owned();
            if !ids.insert(id.clone()) {
                return Err(CoreError::Dataset {
                    line: line_number,
                    message: format!("duplicate case ID `{id}`"),
                });
            }
            let input = row
                .pointer(&fields.input)
                .cloned()
                .ok_or_else(|| CoreError::Dataset {
                    line: line_number,
                    message: format!("input pointer {} did not resolve", fields.input),
                })?;
            cases.push(Case {
                id,
                input,
                expected: row.pointer(&fields.expected).cloned(),
                model_visible_metadata: row.pointer(&fields.model_visible_metadata).cloned(),
                metadata: row.pointer(&fields.metadata).cloned(),
                source_line: line_number,
            });
        }
        if cases.is_empty() {
            return Err(CoreError::Dataset {
                line: 1,
                message: "dataset contains no cases".to_owned(),
            });
        }
        Ok(Self {
            cases,
            source_hash: hash_bytes(bytes),
            source_bytes: bytes.to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::*;

    #[test]
    fn preserves_source_order_and_hashes_exact_bytes() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"{{"id":"b","input":2}}"#).unwrap();
        writeln!(file, r#"{{"id":"a","input":1}}"#).unwrap();
        let dataset = Dataset::read(file.path(), &DatasetFields::default()).unwrap();
        assert_eq!(
            dataset
                .cases
                .iter()
                .map(|case| &case.id)
                .collect::<Vec<_>>(),
            vec!["b", "a"]
        );
        assert_eq!(
            dataset.source_hash,
            hash_bytes(&std::fs::read(file.path()).unwrap())
        );
    }

    #[test]
    fn duplicate_ids_fail_with_line_number() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"{{"id":"same","input":1}}"#).unwrap();
        writeln!(file, r#"{{"id":"same","input":2}}"#).unwrap();
        let error = Dataset::read(file.path(), &DatasetFields::default()).unwrap_err();
        assert!(error.to_string().contains("line 2"));
    }

    #[test]
    fn invalid_utf8_fails_before_case_parsing() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"{\"id\":\"a\",\"input\":\"").unwrap();
        file.write_all(&[0xff]).unwrap();
        file.write_all(b"\"}\n").unwrap();
        let error = Dataset::read(file.path(), &DatasetFields::default()).unwrap_err();
        assert!(error.to_string().contains("not valid UTF-8"));
    }

    #[test]
    fn evaluation_metadata_is_not_model_visible() {
        let bytes = b"{\"id\":\"a\",\"input\":{},\"expected\":{\"label\":\"gold\"},\"model_visible_metadata\":{\"locale\":\"en\"},\"metadata\":{\"private_label\":\"billing\"}}\n";
        let dataset = Dataset::from_bytes(bytes, &DatasetFields::default()).unwrap();
        let variant = VariantCase::from(&dataset.cases[0]);
        let encoded = serde_json::to_value(variant).unwrap();
        assert_eq!(
            encoded.pointer("/metadata/locale"),
            Some(&Value::String("en".to_owned()))
        );
        assert!(encoded.pointer("/expected").is_none());
        assert!(!encoded.to_string().contains("billing"));
    }

    #[test]
    fn bounded_ingestion_rejects_long_lines_and_excess_cases() {
        let bytes = b"{\"id\":\"a\",\"input\":{}}\n{\"id\":\"b\",\"input\":{}}\n";
        let long =
            Dataset::from_bytes_bounded(bytes, &DatasetFields::default(), 8, 10).unwrap_err();
        assert!(long.to_string().contains("JSONL line"));
        let many =
            Dataset::from_bytes_bounded(bytes, &DatasetFields::default(), 1024, 1).unwrap_err();
        assert!(many.to_string().contains("case limit"));
    }
}

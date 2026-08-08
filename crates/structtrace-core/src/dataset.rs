//! Matched JSONL dataset ingestion.

use std::{collections::HashSet, path::Path};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{CoreError, Result, config::DatasetFields, error::read_error, hashing::hash_bytes};

/// One immutable matched case.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Case {
    /// Non-empty unique identifier.
    pub id: String,
    /// Input supplied to both variants.
    pub input: Value,
    /// Optional reference used by deterministic evaluators.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<Value>,
    /// Optional tags or user metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    /// Original one-based source line.
    pub source_line: usize,
}

/// Parsed cases plus the hash of exact source bytes.
#[derive(Debug, Clone)]
pub struct Dataset {
    /// Cases in original display and execution order.
    pub cases: Vec<Case>,
    /// BLAKE3 digest of exact source bytes.
    pub source_hash: String,
}

impl Dataset {
    /// Read, validate, and preserve a JSONL dataset.
    pub fn read(path: &Path, fields: &DatasetFields) -> Result<Self> {
        let bytes = std::fs::read(path).map_err(read_error(path))?;
        let text = std::str::from_utf8(&bytes).map_err(|error| CoreError::Dataset {
            line: 1,
            message: format!("dataset is not valid UTF-8: {error}"),
        })?;
        let mut cases = Vec::new();
        let mut ids = HashSet::new();
        for (index, line) in text.lines().enumerate() {
            let line_number = index + 1;
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
            source_hash: hash_bytes(&bytes),
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
}

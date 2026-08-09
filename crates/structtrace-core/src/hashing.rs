//! BLAKE3 hashing and deterministic JSON serialization.

use std::{collections::BTreeMap, io::Read, path::Path};

use serde::Serialize;
use serde_json::Value;

use crate::{CoreError, Result, error::read_error};

/// BLAKE3 digest represented as lowercase hexadecimal.
pub fn hash_bytes(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// Hash the exact bytes of a file.
pub fn hash_file(path: &Path) -> Result<String> {
    std::fs::read(path)
        .map(|bytes| hash_bytes(&bytes))
        .map_err(read_error(path))
}

/// Read a regular file without allocating beyond a caller-declared byte ceiling.
pub fn read_bounded(path: &Path, maximum: usize, label: &str) -> Result<Vec<u8>> {
    let metadata = std::fs::symlink_metadata(path).map_err(read_error(path))?;
    if metadata.file_type().is_symlink() {
        return Err(CoreError::Artifact(format!(
            "{label} {} must not be a symbolic link",
            path.display()
        )));
    }
    if metadata.len() > maximum as u64 {
        return Err(CoreError::Artifact(format!(
            "{label} {} is {} bytes; limit is {maximum}",
            path.display(),
            metadata.len()
        )));
    }
    let file = std::fs::File::open(path).map_err(read_error(path))?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(maximum as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(read_error(path))?;
    if bytes.len() > maximum {
        return Err(CoreError::Artifact(format!(
            "{label} {} grew beyond the {maximum}-byte limit while being read",
            path.display()
        )));
    }
    Ok(bytes)
}

/// Serialize a value with recursively sorted object keys.
pub fn canonical_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let raw = serde_json::to_value(value)
        .map_err(|error| CoreError::Artifact(format!("could not serialize value: {error}")))?;
    serde_json::to_vec(&sort_json(raw))
        .map_err(|error| CoreError::Artifact(format!("could not encode canonical JSON: {error}")))
}

/// Hash the canonical JSON representation of a serializable value.
pub fn hash_canonical_json<T: Serialize>(value: &T) -> Result<String> {
    canonical_json_bytes(value).map(|bytes| hash_bytes(&bytes))
}

fn sort_json(value: Value) -> Value {
    match value {
        Value::Object(object) => {
            let sorted = object
                .into_iter()
                .map(|(key, value)| (key, sort_json(value)))
                .collect::<BTreeMap<_, _>>();
            Value::Object(sorted.into_iter().collect())
        }
        Value::Array(values) => Value::Array(values.into_iter().map(sort_json).collect()),
        scalar => scalar,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn canonical_hash_ignores_object_insertion_order() {
        let first = json!({"b": 2, "a": {"d": 4, "c": 3}});
        let second = json!({"a": {"c": 3, "d": 4}, "b": 2});
        assert_eq!(
            hash_canonical_json(&first).unwrap(),
            hash_canonical_json(&second).unwrap()
        );
    }
}

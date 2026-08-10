//! Duplicate-key-rejecting JSON parsing for every untrusted boundary.

use std::{collections::HashSet, fmt, str::FromStr};

use serde::de::{DeserializeOwned, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

const ARBITRARY_NUMBER_TOKEN: &str = "$serde_json::private::Number";

/// Parse one JSON value while rejecting duplicate object keys recursively.
pub fn value_from_str(input: &str) -> Result<Value, serde_json::Error> {
    let mut deserializer = serde_json::Deserializer::from_str(input);
    let value = StrictValueSeed::root().deserialize(&mut deserializer)?;
    deserializer.end()?;
    Ok(value)
}

/// Parse one JSON value from UTF-8 bytes while rejecting duplicate object keys recursively.
pub fn value_from_slice(input: &[u8]) -> Result<Value, serde_json::Error> {
    let mut deserializer = serde_json::Deserializer::from_slice(input);
    let value = StrictValueSeed::root().deserialize(&mut deserializer)?;
    deserializer.end()?;
    Ok(value)
}

/// Deserialize a typed value only after duplicate-key-rejecting structural parsing.
pub fn from_str<T: DeserializeOwned>(input: &str) -> Result<T, serde_json::Error> {
    serde_json::from_value(value_from_str(input)?)
}

/// Deserialize a typed value from bytes only after strict structural parsing.
pub fn from_slice<T: DeserializeOwned>(input: &[u8]) -> Result<T, serde_json::Error> {
    serde_json::from_value(value_from_slice(input)?)
}

#[derive(Clone)]
struct StrictValueSeed {
    path: String,
}

impl StrictValueSeed {
    fn root() -> Self {
        Self {
            path: String::new(),
        }
    }

    fn child_key(&self, key: &str) -> Self {
        let escaped = key.replace('~', "~0").replace('/', "~1");
        Self {
            path: format!("{}/{}", self.path, escaped),
        }
    }

    fn child_index(&self, index: usize) -> Self {
        Self {
            path: format!("{}/{index}", self.path),
        }
    }

    fn display_path(&self) -> &str {
        if self.path.is_empty() {
            "/"
        } else {
            &self.path
        }
    }
}

impl<'de> DeserializeSeed<'de> for StrictValueSeed {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(StrictValueVisitor { seed: self })
    }
}

struct StrictValueVisitor {
    seed: StrictValueSeed,
}

impl<'de> Visitor<'de> for StrictValueVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value without duplicate object keys")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Value, E> {
        Ok(Value::Number(value.into()))
    }

    fn visit_i128<E>(self, value: i128) -> Result<Value, E>
    where
        E: serde::de::Error,
    {
        Number::from_str(&value.to_string())
            .map(Value::Number)
            .map_err(E::custom)
    }

    fn visit_u64<E>(self, value: u64) -> Result<Value, E> {
        Ok(Value::Number(value.into()))
    }

    fn visit_u128<E>(self, value: u128) -> Result<Value, E>
    where
        E: serde::de::Error,
    {
        Number::from_str(&value.to_string())
            .map(Value::Number)
            .map_err(E::custom)
    }

    fn visit_f64<E>(self, value: f64) -> Result<Value, E>
    where
        E: serde::de::Error,
    {
        Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| E::custom("non-finite JSON number"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Value, E> {
        Ok(Value::String(value.to_owned()))
    }

    fn visit_string<E>(self, value: String) -> Result<Value, E> {
        Ok(Value::String(value))
    }

    fn visit_none<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        self.seed.deserialize(deserializer)
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::with_capacity(sequence.size_hint().unwrap_or(0));
        while let Some(value) = sequence.next_element_seed(self.seed.child_index(values.len()))? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }

    fn visit_map<A>(self, mut object: A) -> Result<Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        // serde_json exposes arbitrary-precision numbers to visitors through a synthetic
        // one-entry map. Restrict its reserved token to that access type so an ordinary user
        // object containing the same string remains an object.
        let arbitrary_number_map = std::any::type_name::<A>().contains("NumberDeserializer");
        let mut values = Map::new();
        let mut seen = HashSet::new();
        while let Some(key) = object.next_key::<String>()? {
            if arbitrary_number_map
                && key == ARBITRARY_NUMBER_TOKEN
                && values.is_empty()
                && seen.is_empty()
            {
                let raw = object.next_value::<String>()?;
                return Number::from_str(&raw)
                    .map(Value::Number)
                    .map_err(serde::de::Error::custom);
            }
            if !seen.insert(key.clone()) {
                return Err(serde::de::Error::custom(format!(
                    "duplicate object key `{key}` at {}",
                    self.seed.display_path()
                )));
            }
            let value = object.next_value_seed(self.seed.child_key(&key))?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn duplicate_keys_are_rejected_recursively() {
        assert!(value_from_str(r#"{"id":"a","id":"b"}"#).is_err());
        let error = value_from_str(r#"{"outer":{"x":1,"x":2}}"#)
            .unwrap_err()
            .to_string();
        assert!(error.contains("duplicate object key `x` at /outer"));
    }

    #[test]
    fn ordinary_unique_json_and_large_numbers_are_unchanged() {
        let parsed =
            value_from_str(r#"{"nested":[true,null,{"value":123456789012345678901234567890}]}"#)
                .unwrap();
        assert_eq!(
            parsed,
            json!({"nested": [true, null, {"value": 123456789012345678901234567890_u128}]})
        );
        assert_eq!(
            value_from_str(r#"{"$serde_json::private::Number":"123"}"#).unwrap(),
            json!({"$serde_json::private::Number": "123"})
        );
    }

    #[test]
    fn typed_values_are_only_deserialized_after_strict_parsing() {
        #[derive(Debug, serde::Deserialize, PartialEq)]
        struct Item {
            value: u8,
        }
        assert_eq!(
            from_str::<Item>(r#"{"value":7}"#).unwrap(),
            Item { value: 7 }
        );
        assert!(from_str::<Item>(r#"{"value":7,"value":8}"#).is_err());
    }
}

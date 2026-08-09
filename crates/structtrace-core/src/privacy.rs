//! Deterministic JSON Pointer redaction for shareable artifacts.

use serde_json::Value;

/// Fixed marker used wherever a configured pointer resolves.
pub const REDACTION_MARKER: &str = "[REDACTED]";

/// Replace every resolved JSON Pointer with a fixed, non-reversible marker.
pub fn redact_json_pointers(value: &mut Value, pointers: &[String]) {
    for pointer in pointers {
        if pointer.is_empty() {
            *value = Value::String(REDACTION_MARKER.to_owned());
        } else if let Some(target) = value.pointer_mut(pointer) {
            *target = Value::String(REDACTION_MARKER.to_owned());
        }
    }
}

/// Copy values resolved by pointers so repeated echoes can be removed from a report.
pub fn selected_values(value: &Value, pointers: &[String]) -> Vec<Value> {
    pointers
        .iter()
        .filter_map(|pointer| {
            if pointer.is_empty() {
                Some(value.clone())
            } else {
                value.pointer(pointer).cloned()
            }
        })
        .collect()
}

/// Recursively replace values equal to any selected secret, including evaluator echoes.
pub fn redact_matching_values(value: &mut Value, secrets: &[Value]) {
    if secrets.iter().any(|secret| secret == value) {
        *value = Value::String(REDACTION_MARKER.to_owned());
        return;
    }
    match value {
        Value::Array(items) => {
            for item in items {
                redact_matching_values(item, secrets);
            }
        }
        Value::Object(map) => {
            for item in map.values_mut() {
                redact_matching_values(item, secrets);
            }
        }
        Value::String(text) => {
            redact_text(text, secrets);
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

/// Remove configured values echoed into free-form text without replacing every
/// occurrence of tiny, common scalar tokens such as `0`, `1`, `true`, or `false`.
pub fn redact_text(text: &mut String, secrets: &[Value]) {
    for secret in secrets {
        let (needle, may_replace_substring) = match secret {
            Value::String(value) => (value.clone(), !value.is_empty()),
            Value::Number(value) => {
                let value = value.to_string();
                let replace = value.len() >= 4;
                (value, replace)
            }
            Value::Bool(value) => (value.to_string(), false),
            Value::Array(_) | Value::Object(_) => {
                let value = serde_json::to_string(secret).unwrap_or_default();
                let replace = value.len() >= 8;
                (value, replace)
            }
            Value::Null => ("null".to_owned(), false),
        };
        if needle.is_empty() {
            continue;
        }
        if text == &needle {
            *text = REDACTION_MARKER.to_owned();
            return;
        }
        if may_replace_substring {
            *text = text.replace(&needle, REDACTION_MARKER);
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use serde_json::json;

    use super::*;

    #[test]
    fn redacts_a_nested_value_without_touching_siblings() {
        let mut value = json!({"input": {"email": "secret@example.com", "ticket": "T-1"}});
        redact_json_pointers(&mut value, &["/input/email".to_owned()]);
        assert_eq!(value["input"]["email"], REDACTION_MARKER);
        assert_eq!(value["input"]["ticket"], "T-1");
    }

    #[test]
    fn removes_selected_values_repeated_elsewhere() {
        let source = json!({"input": {"email": "secret@example.com"}});
        let secrets = selected_values(&source, &["/input/email".to_owned()]);
        let mut report = json!({
            "case": source,
            "evaluation": {"actual": "secret@example.com"}
        });
        redact_matching_values(&mut report, &secrets);
        assert!(
            !serde_json::to_string(&report)
                .unwrap()
                .contains("secret@example.com")
        );
    }

    #[test]
    fn removes_a_selected_secret_embedded_inside_prompt_text() {
        let source = json!({"input": {"email": "secret@example.com"}});
        let secrets = selected_values(&source, &["/input/email".to_owned()]);
        let mut report = json!({"prompt": "Route the ticket for secret@example.com now"});
        redact_matching_values(&mut report, &secrets);
        assert!(
            !serde_json::to_string(&report)
                .unwrap()
                .contains("secret@example.com")
        );
    }

    #[test]
    fn removes_cross_type_scalar_and_object_echoes() {
        let secrets = vec![json!(123456), json!(true), json!({"account": "private-91"})];
        let mut report = json!({
            "number_as_text": "reference 123456 was processed",
            "boolean_as_text": "true",
            "object_as_text": "payload={\"account\":\"private-91\"}",
        });
        redact_matching_values(&mut report, &secrets);
        let serialized = serde_json::to_string(&report).unwrap();
        assert!(!serialized.contains("123456"));
        assert!(!serialized.contains("private-91"));
        assert_eq!(report["boolean_as_text"], REDACTION_MARKER);
    }

    #[test]
    fn tiny_common_scalars_do_not_over_redact_unrelated_text() {
        let mut value = json!({
            "zero": 0,
            "one": 1,
            "flag": true,
            "sentence": "version 10 is true enough",
        });
        redact_matching_values(&mut value, &[json!(0), json!(1), json!(true)]);
        assert_eq!(value["zero"], REDACTION_MARKER);
        assert_eq!(value["one"], REDACTION_MARKER);
        assert_eq!(value["flag"], REDACTION_MARKER);
        assert_eq!(value["sentence"], "version 10 is true enough");
    }

    proptest! {
        #[test]
        fn selected_string_is_never_present_after_redaction(secret in "[a-zA-Z0-9]{1,64}") {
            let mut value = json!({"input": {"private": secret.clone()}});
            redact_json_pointers(&mut value, &["/input/private".to_owned()]);
            let serialized = serde_json::to_string(&value).unwrap();
            let encoded_secret = serde_json::to_string(&secret).unwrap();
            prop_assert!(!serialized.contains(&encoded_secret));
        }
    }
}

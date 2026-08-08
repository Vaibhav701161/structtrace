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
            for secret in secrets {
                if let Value::String(secret) = secret {
                    if !secret.is_empty() {
                        *text = text.replace(secret, REDACTION_MARKER);
                    }
                }
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
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

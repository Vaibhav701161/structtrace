//! Conservative JSON Schema fact extraction and sensitivity warnings.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Complete non-predictive schema inspection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchemaInspection {
    /// Declared JSON Schema dialect URI when present.
    pub draft: Option<String>,
    /// Discovered object fields.
    pub fields: Vec<SchemaField>,
    /// Every `$ref` and whether it resolved locally.
    pub references: Vec<SchemaReference>,
    /// Neutral, workload-testing recommendations.
    pub warnings: Vec<SensitivityWarning>,
}

/// One field and its declared constraints.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchemaField {
    /// JSON Pointer in instances.
    pub pointer: String,
    /// Whether the parent object requires the field.
    pub required: bool,
    /// Declared JSON types.
    pub types: Vec<String>,
    /// Enum values.
    pub enum_values: Vec<Value>,
    /// Regular-expression constraint.
    pub pattern: Option<String>,
    /// Numeric or length lower bound facts.
    pub minimum: Option<Value>,
    /// Numeric or length upper bound facts.
    pub maximum: Option<Value>,
    /// Nesting depth from the root object.
    pub depth: usize,
}

/// One schema reference.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchemaReference {
    /// Schema location containing the reference.
    pub schema_path: String,
    /// Reference URI.
    pub reference: String,
    /// Whether a fragment-only reference resolves in the supplied document.
    pub locally_resolved: bool,
    /// Whether retrieval would require remote access.
    pub remote: bool,
}

/// Potential representation-sensitive boundary without a quality prediction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SensitivityWarning {
    /// Instance or schema path.
    pub pointer: String,
    /// Stable warning category.
    pub kind: String,
    /// Observed fact.
    pub observation: String,
    /// Neutral recommendation.
    pub recommendation: String,
}

/// Inspect schema facts without transforming or scoring the schema.
pub fn inspect_schema(schema: &Value) -> SchemaInspection {
    let mut inspection = SchemaInspection {
        draft: schema
            .get("$schema")
            .and_then(Value::as_str)
            .map(str::to_owned),
        fields: Vec::new(),
        references: Vec::new(),
        warnings: Vec::new(),
    };
    walk_schema(schema, schema, "", "", 0, &mut inspection);
    inspection
}

fn walk_schema(
    root: &Value,
    node: &Value,
    instance_path: &str,
    schema_path: &str,
    depth: usize,
    inspection: &mut SchemaInspection,
) {
    if let Some(reference) = node.get("$ref").and_then(Value::as_str) {
        let remote = !reference.starts_with('#');
        let locally_resolved = reference
            .strip_prefix('#')
            .is_some_and(|fragment| fragment.is_empty() || root.pointer(fragment).is_some());
        inspection.references.push(SchemaReference {
            schema_path: schema_path.to_owned(),
            reference: reference.to_owned(),
            locally_resolved,
            remote,
        });
        if remote || !locally_resolved {
            inspection.warnings.push(warning(
                schema_path,
                "unresolved_reference",
                if remote {
                    "Reference requires external retrieval, which is disabled by default."
                } else {
                    "Local fragment reference did not resolve in this document."
                },
            ));
        }
    }
    let required = node
        .get("required")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .collect::<std::collections::BTreeSet<_>>()
        })
        .unwrap_or_default();
    if required.len() >= 10 {
        inspection.warnings.push(warning(
            if instance_path.is_empty() {
                "/"
            } else {
                instance_path
            },
            "large_required_surface",
            &format!("Object declares {} required fields.", required.len()),
        ));
    }
    if let Some(properties) = node.get("properties").and_then(Value::as_object) {
        for (name, child) in properties {
            let pointer = format!("{}/{}", instance_path, escape_pointer(name));
            let child_schema_path = format!("{schema_path}/properties/{}", escape_pointer(name));
            let field = SchemaField {
                pointer: pointer.clone(),
                required: required.contains(name.as_str()),
                types: declared_types(child),
                enum_values: child
                    .get("enum")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
                pattern: child
                    .get("pattern")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                minimum: child
                    .get("minimum")
                    .or_else(|| child.get("minLength"))
                    .cloned(),
                maximum: child
                    .get("maximum")
                    .or_else(|| child.get("maxLength"))
                    .cloned(),
                depth: depth + 1,
            };
            add_field_warnings(&field, child, inspection);
            inspection.fields.push(field);
            walk_schema(
                root,
                child,
                &pointer,
                &child_schema_path,
                depth + 1,
                inspection,
            );
        }
    }
    if let Some(items) = node.get("items") {
        walk_schema(
            root,
            items,
            &format!("{instance_path}/*"),
            &format!("{schema_path}/items"),
            depth + 1,
            inspection,
        );
    }
    for keyword in ["anyOf", "oneOf", "allOf"] {
        if let Some(branches) = node.get(keyword).and_then(Value::as_array) {
            if keyword != "allOf"
                && branches
                    .iter()
                    .any(|branch| declared_types(branch).iter().any(|kind| kind == "null"))
            {
                inspection.warnings.push(warning(
                    if instance_path.is_empty() { "/" } else { instance_path },
                    "nullable_union",
                    "Union includes a null branch; confirm that absence and explicit null have intended semantics.",
                ));
            }
            for (index, branch) in branches.iter().enumerate() {
                walk_schema(
                    root,
                    branch,
                    instance_path,
                    &format!("{schema_path}/{keyword}/{index}"),
                    depth,
                    inspection,
                );
            }
        }
    }
}

fn add_field_warnings(field: &SchemaField, schema: &Value, inspection: &mut SchemaInspection) {
    if field.types.iter().any(|kind| kind == "string") {
        if field.pattern.as_deref().is_some_and(looks_numeric_pattern) {
            inspection.warnings.push(warning(
                &field.pointer,
                "numeric_string",
                "External type is string and its pattern resembles a numeric representation.",
            ));
        }
        let enum_strings = field
            .enum_values
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_ascii_lowercase)
            .collect::<std::collections::BTreeSet<_>>();
        if enum_strings == std::collections::BTreeSet::from(["false".to_owned(), "true".to_owned()])
        {
            inspection.warnings.push(warning(
                &field.pointer,
                "boolean_string",
                "External type is string with boolean-like enum labels.",
            ));
        }
    }
    if field.enum_values.len() >= 20 {
        inspection.warnings.push(warning(
            &field.pointer,
            "large_enum",
            &format!("Enum contains {} labels.", field.enum_values.len()),
        ));
    }
    if field.depth >= 5 {
        inspection.warnings.push(warning(
            &field.pointer,
            "deep_nesting",
            &format!(
                "Field is nested {} object levels from the root.",
                field.depth
            ),
        ));
    }
    if schema.get("pattern").is_some() {
        inspection.warnings.push(warning(
            &field.pointer,
            "regex_constraint",
            "Field is constrained by a regular expression.",
        ));
    }
}

fn declared_types(schema: &Value) -> Vec<String> {
    match schema.get("type") {
        Some(Value::String(value)) => vec![value.clone()],
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

fn looks_numeric_pattern(pattern: &str) -> bool {
    let lower = pattern.to_ascii_lowercase();
    (lower.contains("[0-9]") || lower.contains("\\d"))
        && (lower.contains('+')
            || lower.contains('-')
            || lower.contains("decimal")
            || lower.contains('.'))
}

fn warning(pointer: &str, kind: &str, observation: &str) -> SensitivityWarning {
    SensitivityWarning {
        pointer: pointer.to_owned(),
        kind: kind.to_owned(),
        observation: observation.to_owned(),
        recommendation: "Potential sensitivity boundary detected. Observed quality direction cannot be inferred statically. Use a paired StructTrace evaluation before changing this contract.".to_owned(),
    }
}

fn escape_pointer(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn flags_numeric_string_without_predicting_direction() {
        let result = inspect_schema(&json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "required": ["amount"],
            "properties": {
                "amount": {"type": "string", "pattern": "^-?[0-9]+$"}
            }
        }));
        assert_eq!(result.fields[0].pointer, "/amount");
        assert!(
            result
                .warnings
                .iter()
                .any(|item| item.kind == "numeric_string")
        );
        assert!(
            result
                .warnings
                .iter()
                .all(|item| item.recommendation.contains("cannot be inferred"))
        );
    }

    #[test]
    fn reports_unresolved_refs() {
        let result = inspect_schema(&json!({"$ref": "#/$defs/missing"}));
        assert!(!result.references[0].locally_resolved);
    }
}

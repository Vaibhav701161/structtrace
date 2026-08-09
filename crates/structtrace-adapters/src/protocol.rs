//! Versioned JSONL subprocess protocol.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use structtrace_core::{PROTOCOL_VERSION, dataset::VariantCase, output::Usage};

use crate::VARIANT_PROTOCOL;

/// Request sent on subprocess standard input.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct VariantRequest {
    /// Protocol identity.
    pub protocol: String,
    /// Protocol format version.
    pub protocol_version: u32,
    /// Matched case ID.
    pub case_id: String,
    /// Application input.
    pub input: Value,
    /// Optional metadata explicitly designated as model-visible.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

impl From<&VariantCase> for VariantRequest {
    fn from(case: &VariantCase) -> Self {
        Self {
            protocol: VARIANT_PROTOCOL.to_owned(),
            protocol_version: PROTOCOL_VERSION,
            case_id: case.id.clone(),
            input: case.input.clone(),
            metadata: case.metadata.clone(),
        }
    }
}

/// Successful or failed subprocess response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct VariantResponse {
    /// Protocol identity.
    pub protocol: String,
    /// Protocol format version.
    pub protocol_version: u32,
    /// Must match the request case ID.
    pub case_id: String,
    /// `ok` or `error`.
    pub status: String,
    /// Structured result when returned directly.
    #[serde(default)]
    pub output: Option<Value>,
    /// Original application output text when available.
    #[serde(default)]
    pub raw_output: Option<String>,
    /// Application error envelope.
    #[serde(default)]
    pub error: Option<ProtocolError>,
    /// Optional token usage.
    #[serde(default)]
    pub usage: Option<Usage>,
    /// Redaction-safe application metadata.
    #[serde(default)]
    pub metadata: Value,
}

/// Error returned through a subprocess protocol.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProtocolError {
    /// Stable application error category.
    pub kind: String,
    /// Redaction-safe detail.
    pub message: String,
}

/// Validate immutable protocol fields and response shape.
pub fn validate_response(response: &VariantResponse, case_id: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        response.protocol == VARIANT_PROTOCOL,
        "unexpected protocol `{}`",
        response.protocol
    );
    anyhow::ensure!(
        response.protocol_version == PROTOCOL_VERSION,
        "unsupported protocol version {}",
        response.protocol_version
    );
    anyhow::ensure!(
        response.case_id == case_id,
        "response case ID `{}` did not match request `{case_id}`",
        response.case_id
    );
    match response.status.as_str() {
        "ok" => {
            anyhow::ensure!(
                response.output.is_some() || response.raw_output.is_some(),
                "successful response requires output or raw_output"
            );
            anyhow::ensure!(
                response.error.is_none(),
                "successful response must not contain an error envelope"
            );
            if let (Some(output), Some(raw)) = (&response.output, &response.raw_output) {
                let parsed: Value = serde_json::from_str(raw)
                    .map_err(|error| anyhow::anyhow!("raw_output is not JSON: {error}"))?;
                anyhow::ensure!(
                    &parsed == output,
                    "output and raw_output represent different JSON values"
                );
            }
        }
        "error" => {
            anyhow::ensure!(
                response.error.is_some(),
                "error response requires an error envelope"
            );
            anyhow::ensure!(
                response.output.is_none() && response.raw_output.is_none(),
                "error response must not contain output or raw_output"
            );
        }
        other => anyhow::bail!("unsupported response status `{other}`"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn rejects_wrong_case_identity() {
        let response = VariantResponse {
            protocol: VARIANT_PROTOCOL.to_owned(),
            protocol_version: PROTOCOL_VERSION,
            case_id: "other".to_owned(),
            status: "ok".to_owned(),
            output: Some(json!({})),
            raw_output: None,
            error: None,
            usage: None,
            metadata: Value::Null,
        };
        assert!(validate_response(&response, "expected").is_err());
    }

    #[test]
    fn variant_request_never_contains_expected() {
        let case = VariantCase {
            id: "case-1".to_owned(),
            input: json!({"question": "safe"}),
            metadata: Some(json!({"locale": "en"})),
        };
        let value = serde_json::to_value(VariantRequest::from(&case)).unwrap();
        assert!(value.get("expected").is_none());
        assert_eq!(value.pointer("/metadata/locale"), Some(&json!("en")));
    }

    #[test]
    fn contradictory_and_unknown_response_fields_are_rejected() {
        let contradictory: VariantResponse = serde_json::from_value(serde_json::json!({
            "protocol": VARIANT_PROTOCOL,
            "protocol_version": PROTOCOL_VERSION,
            "case_id": "one",
            "status": "ok",
            "output": {"label": "yes"},
            "raw_output": "{\"label\":\"no\"}"
        }))
        .unwrap();
        assert!(validate_response(&contradictory, "one").is_err());
        assert!(
            serde_json::from_value::<VariantResponse>(serde_json::json!({
                "protocol": VARIANT_PROTOCOL,
                "protocol_version": PROTOCOL_VERSION,
                "case_id": "one",
                "status": "error",
                "error": {"kind": "failed", "message": "safe"},
                "unknown": true
            }))
            .is_err()
        );
    }
}

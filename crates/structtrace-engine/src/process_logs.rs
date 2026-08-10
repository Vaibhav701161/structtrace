//! Explicit, bounded retention for user-controlled process logs.

use serde_json::Value;
use structtrace_core::{
    config::{Config, ProcessLogMode, VariantConfig},
    privacy::{REDACTION_MARKER, redact_text_with_policy},
};

const TRUNCATION_MARKER: &[u8] = b"\n[StructTrace: process log truncated]\n";

/// Sanitize and cap one process log against a run-wide remaining budget.
pub(crate) fn retain(config: &Config, bytes: &[u8], remaining: &mut usize) -> Option<Vec<u8>> {
    if bytes.is_empty()
        || config.storage.process_logs.mode == ProcessLogMode::Off
        || *remaining == 0
    {
        return None;
    }
    let mut value = match config.storage.process_logs.mode {
        ProcessLogMode::Off => return None,
        ProcessLogMode::FullSensitive => bytes.to_vec(),
        ProcessLogMode::Sanitized => sanitize(config, bytes),
    };
    if value.len() > *remaining {
        let marker = if *remaining >= TRUNCATION_MARKER.len() {
            TRUNCATION_MARKER
        } else {
            &[]
        };
        value.truncate((*remaining).saturating_sub(marker.len()));
        while std::str::from_utf8(&value).is_err() {
            value.pop();
        }
        value.extend_from_slice(marker);
    }
    *remaining = (*remaining).saturating_sub(value.len());
    Some(value)
}

fn sanitize(config: &Config, bytes: &[u8]) -> Vec<u8> {
    let mut text = String::from_utf8_lossy(bytes).into_owned();
    let credentials = config
        .variants
        .values()
        .filter_map(|variant| match variant {
            VariantConfig::OpenaiCompatible(adapter) => adapter.api_key_env.as_deref(),
            _ => None,
        })
        .filter_map(|name| std::env::var(name).ok())
        .filter(|value| !value.is_empty())
        .map(Value::String)
        .collect::<Vec<_>>();
    redact_text_with_policy(
        &mut text,
        &credentials,
        true,
        &config.storage.process_logs.custom_patterns,
    );
    text.lines()
        .map(|line| {
            let lower = line.to_ascii_lowercase();
            if lower.contains("authorization:")
                || lower.contains("api-key:")
                || lower.contains("api_key:")
                || lower.contains("bearer ")
            {
                REDACTION_MARKER
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes()
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use structtrace_core::config::{Config, ProcessLogMode};

    use super::*;

    fn config() -> Config {
        serde_json::from_value(json!({
            "version": 3,
            "project": {"name": "log-test"},
            "dataset": {"path": "data.jsonl"},
            "schema": {"path": "schema.json"},
            "variants": {
                "baseline": {"kind": "recorded", "path": "baseline.jsonl"},
                "candidate": {"kind": "recorded", "path": "candidate.jsonl"}
            },
            "evaluators": [{"id": "exact", "kind": "exact_json"}],
            "outcomes": {"correct": {"all_of": ["exact"]}},
            "analysis": {"primary_outcome": "correct"}
        }))
        .unwrap()
    }

    #[test]
    fn log_retention_off_writes_no_logs() {
        let config = config();
        let mut remaining = 100;
        assert_eq!(retain(&config, b"secret output", &mut remaining), None);
        assert_eq!(remaining, 100);
    }

    #[test]
    fn sanitized_log_removes_literals_and_header_shaped_secrets() {
        let mut config = config();
        config.storage.process_logs.mode = ProcessLogMode::Sanitized;
        config.storage.process_logs.custom_patterns = vec!["sentinel-secret".to_owned()];
        let mut remaining = 4096;
        let retained = retain(
            &config,
            b"ordinary line\nsentinel-secret\nAuthorization: Bearer abc\napi-key: xyz",
            &mut remaining,
        )
        .unwrap();
        let retained = String::from_utf8(retained).unwrap();
        assert!(retained.contains("ordinary line"));
        assert!(!retained.contains("sentinel-secret"));
        assert!(!retained.contains("Bearer abc"));
        assert!(!retained.contains("xyz"));
    }

    #[test]
    fn sanitized_log_records_truncation_and_enforces_total_budget() {
        let mut config = config();
        config.storage.process_logs.mode = ProcessLogMode::Sanitized;
        let mut remaining = 64;
        let first = retain(&config, &[b'a'; 100], &mut remaining).unwrap();
        assert_eq!(first.len(), 64);
        assert!(first.ends_with(TRUNCATION_MARKER));
        assert_eq!(remaining, 0);
        assert_eq!(retain(&config, b"later log", &mut remaining), None);
    }
}

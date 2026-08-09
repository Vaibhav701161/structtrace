//! Fail-closed command and Python evaluator execution.

use std::{
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

use serde_json::{Value, json};
use structtrace_core::{
    artifact::ExternalEvaluatorReceipt,
    config::{CommandSpec, EvaluatorKind},
    dataset::Case,
    evaluation::{EvaluationStatus, EvaluatorResult},
    hashing::hash_canonical_json,
    output::VariantOutput,
};
use wait_timeout::ChildExt;

use crate::command::CommandLimits;

/// Evaluator protocol identifier.
pub const EVALUATOR_PROTOCOL: &str = "structtrace.evaluator";
/// Bundled Python evaluator bridge source.
pub const EVALUATOR_BRIDGE_SOURCE: &str =
    include_str!("../../../python/structtrace_evaluator_bridge.py");

/// Result plus capped evaluator diagnostics.
#[derive(Debug, Clone)]
pub struct EvaluatorRun {
    /// Auditable evaluator result.
    pub result: EvaluatorResult,
    /// Capped standard error emitted by the evaluator.
    pub stderr: Vec<u8>,
    /// Hash-bound request, response, and executable-definition receipt.
    pub receipt: ExternalEvaluatorReceipt,
}

/// Per-invocation execution context shared by command and Python evaluators.
#[derive(Debug, Clone, Copy)]
pub struct EvaluatorRuntime<'a> {
    /// Baseline or candidate identity supplied to evaluator metadata.
    pub variant_id: &'a str,
    /// Project root used as the subprocess working directory.
    pub working_directory: &'a Path,
    /// Materialized Python bridge used only by Python evaluator definitions.
    pub python_bridge: &'a Path,
    /// Output and diagnostic retention bounds.
    pub limits: &'a CommandLimits,
}

/// Execute one configured external evaluator for one case and variant output.
pub fn run_external_evaluator(
    evaluator_id: &str,
    kind: &EvaluatorKind,
    case: &Case,
    output: &VariantOutput,
    runtime: EvaluatorRuntime<'_>,
) -> EvaluatorRun {
    let (command, timeout_ms) = match kind {
        EvaluatorKind::Command {
            command,
            timeout_ms,
        } => (command.clone(), *timeout_ms),
        EvaluatorKind::Python {
            interpreter,
            callable,
            timeout_ms,
        } => (
            CommandSpec {
                program: interpreter.clone(),
                args: vec![
                    runtime.python_bridge.display().to_string(),
                    "--callable".to_owned(),
                    callable.clone(),
                ],
            },
            *timeout_ms,
        ),
        _ => {
            let result = error(evaluator_id, "evaluator is not an external evaluator");
            return receipt_run(
                result,
                Vec::new(),
                evaluator_id,
                case,
                output,
                runtime.variant_id,
                kind,
            );
        }
    };
    let request = evaluator_request(evaluator_id, case, output, runtime.variant_id);
    let run = execute(
        evaluator_id,
        &command,
        timeout_ms,
        &request,
        runtime.working_directory,
        runtime.limits,
    );
    receipt_run(
        run.result,
        run.stderr,
        evaluator_id,
        case,
        output,
        runtime.variant_id,
        kind,
    )
}

/// Canonical request object used by execution and replay receipt verification.
pub fn evaluator_request(
    evaluator_id: &str,
    case: &Case,
    output: &VariantOutput,
    variant_id: &str,
) -> Value {
    json!({
        "protocol": EVALUATOR_PROTOCOL,
        "protocol_version": structtrace_core::PROTOCOL_VERSION,
        "evaluator_id": evaluator_id,
        "case_id": case.id,
        "case": case,
        "model_output": output,
        "variant_metadata": {"variant_id": variant_id},
    })
}

fn receipt_run(
    result: EvaluatorResult,
    stderr: Vec<u8>,
    evaluator_id: &str,
    case: &Case,
    output: &VariantOutput,
    variant_id: &str,
    kind: &EvaluatorKind,
) -> EvaluatorRun {
    let request = evaluator_request(evaluator_id, case, output, variant_id);
    let response = serde_json::to_value(&result).unwrap_or(Value::Null);
    let definition = serde_json::to_value(kind).unwrap_or(Value::Null);
    EvaluatorRun {
        receipt: ExternalEvaluatorReceipt {
            evaluator_id: evaluator_id.to_owned(),
            case_id: case.id.clone(),
            variant_id: variant_id.to_owned(),
            request_hash: hash_canonical_json(&request).unwrap_or_default(),
            response_hash: hash_canonical_json(&response).unwrap_or_default(),
            definition_hash: hash_canonical_json(&definition).unwrap_or_default(),
            result: result.clone(),
        },
        result,
        stderr,
    }
}

struct EvaluatorProcessResult {
    result: EvaluatorResult,
    stderr: Vec<u8>,
}

fn execute(
    evaluator_id: &str,
    spec: &CommandSpec,
    timeout_ms: u64,
    request: &Value,
    working_directory: &Path,
    limits: &CommandLimits,
) -> EvaluatorProcessResult {
    let mut child = match Command::new(&spec.program)
        .args(&spec.args)
        .current_dir(working_directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(failure) => {
            return EvaluatorProcessResult {
                result: error(
                    evaluator_id,
                    format!("could not start evaluator: {failure}"),
                ),
                stderr: Vec::new(),
            };
        }
    };
    let write_result = child.stdin.take().map_or_else(
        || Err(std::io::Error::other("evaluator stdin unavailable")),
        |mut stdin| {
            serde_json::to_writer(&mut stdin, request).map_err(std::io::Error::other)?;
            stdin.write_all(b"\n")?;
            stdin.flush()
        },
    );
    if let Err(failure) = write_result {
        let _ = child.kill();
        let _ = child.wait();
        return EvaluatorProcessResult {
            result: error(
                evaluator_id,
                format!("could not write evaluator request: {failure}"),
            ),
            stderr: Vec::new(),
        };
    }
    let stdout_task = child.stdout.take().map(|stream| {
        let limit = limits.max_output_bytes;
        std::thread::spawn(move || read_capped(stream, limit))
    });
    let stderr_task = child.stderr.take().map(|stream| {
        let limit = limits.max_stderr_bytes;
        std::thread::spawn(move || read_capped(stream, limit))
    });
    let status = child.wait_timeout(Duration::from_millis(timeout_ms));
    let timed_out = matches!(status, Ok(None));
    if timed_out {
        let _ = child.kill();
        let _ = child.wait();
    }
    let stdout = stdout_task
        .and_then(|task| task.join().ok())
        .unwrap_or_default();
    let stderr = stderr_task
        .and_then(|task| task.join().ok())
        .unwrap_or_default();
    if timed_out {
        return EvaluatorProcessResult {
            result: error(
                evaluator_id,
                format!("evaluator exceeded configured timeout of {timeout_ms} ms"),
            ),
            stderr,
        };
    }
    match status {
        Err(failure) => {
            return EvaluatorProcessResult {
                result: error(
                    evaluator_id,
                    format!("could not wait for evaluator: {failure}"),
                ),
                stderr,
            };
        }
        Ok(Some(exit_status)) if !exit_status.success() => {
            return EvaluatorProcessResult {
                result: error(
                    evaluator_id,
                    format!("evaluator exited unsuccessfully with {exit_status}"),
                ),
                stderr,
            };
        }
        Ok(_) => {}
    }
    EvaluatorProcessResult {
        result: parse_response(evaluator_id, &stdout),
        stderr,
    }
}

fn parse_response(evaluator_id: &str, bytes: &[u8]) -> EvaluatorResult {
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(failure) => {
            return error(
                evaluator_id,
                format!("evaluator stdout is not UTF-8: {failure}"),
            );
        }
    };
    let lines = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    if lines.len() != 1 {
        return error(
            evaluator_id,
            format!(
                "evaluator must emit exactly one nonblank JSONL response; observed {}",
                lines.len()
            ),
        );
    }
    let value: Value = match serde_json::from_str(lines[0]) {
        Ok(value) => value,
        Err(failure) => {
            return error(
                evaluator_id,
                format!("invalid evaluator response: {failure}"),
            );
        }
    };
    if value.pointer("/protocol").and_then(Value::as_str) != Some(EVALUATOR_PROTOCOL)
        || value.pointer("/protocol_version").and_then(Value::as_u64)
            != Some(u64::from(structtrace_core::PROTOCOL_VERSION))
        || value.pointer("/evaluator_id").and_then(Value::as_str) != Some(evaluator_id)
    {
        return error(
            evaluator_id,
            "evaluator response identity or protocol did not match",
        );
    }
    let status = match value.pointer("/status").and_then(Value::as_str) {
        Some("passed") => EvaluationStatus::Passed,
        Some("failed") => EvaluationStatus::Failed,
        Some("error") => EvaluationStatus::Error,
        Some("not_applicable") => EvaluationStatus::NotApplicable,
        _ => return error(evaluator_id, "evaluator response has an unsupported status"),
    };
    let score = value.pointer("/score").and_then(Value::as_f64);
    if score.is_some_and(|score| !(0.0..=1.0).contains(&score)) {
        return error(evaluator_id, "evaluator score must be between zero and one");
    }
    EvaluatorResult {
        evaluator_id: evaluator_id.to_owned(),
        status,
        passed: status == EvaluationStatus::Passed,
        score,
        message: value
            .pointer("/message")
            .and_then(Value::as_str)
            .unwrap_or("External evaluator returned no message.")
            .to_owned(),
        details: value.pointer("/details").cloned().unwrap_or(Value::Null),
        fields: Vec::new(),
    }
}

fn read_capped(mut stream: impl Read, limit: usize) -> Vec<u8> {
    let mut retained = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(count) => {
                let remaining = limit.saturating_sub(retained.len());
                retained.extend_from_slice(&chunk[..count.min(remaining)]);
            }
        }
    }
    retained
}

fn error(evaluator_id: &str, message: impl Into<String>) -> EvaluatorResult {
    EvaluatorResult {
        evaluator_id: evaluator_id.to_owned(),
        status: EvaluationStatus::Error,
        passed: false,
        score: None,
        message: message.into(),
        details: Value::Null,
        fields: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    use tempfile::tempdir;

    #[test]
    fn malformed_response_fails_closed() {
        let result = parse_response("business", b"not-json\n");
        assert_eq!(result.status, EvaluationStatus::Error);
        assert!(!result.passed);
    }

    #[cfg(unix)]
    #[test]
    fn command_evaluator_returns_auditable_result() {
        let root = tempdir().unwrap();
        let script = root.path().join("evaluator.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\nread line\nprintf '%s\\n' '{\"protocol\":\"structtrace.evaluator\",\"protocol_version\":1,\"evaluator_id\":\"business\",\"status\":\"passed\",\"score\":1,\"message\":\"receipt verified\"}'\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let case: Case = serde_json::from_value(json!({
            "id": "one", "input": {}, "source_line": 1
        }))
        .unwrap();
        let output: VariantOutput = serde_json::from_value(json!({
            "case_id": "one", "status": "ok", "raw_output": "{}", "metadata": {}, "retries": []
        }))
        .unwrap();
        let run = run_external_evaluator(
            "business",
            &EvaluatorKind::Command {
                command: CommandSpec {
                    program: script.display().to_string(),
                    args: vec![],
                },
                timeout_ms: 1000,
            },
            &case,
            &output,
            EvaluatorRuntime {
                variant_id: "baseline",
                working_directory: root.path(),
                python_bridge: Path::new("unused"),
                limits: &CommandLimits::default(),
            },
        );
        assert!(run.result.passed);
        assert_eq!(run.result.message, "receipt verified");
    }

    #[cfg(unix)]
    #[test]
    fn nonzero_evaluator_exit_is_error() {
        let root = tempdir().unwrap();
        let script = root.path().join("evaluator.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\nread line\nprintf '%s\\n' '{\"protocol\":\"structtrace.evaluator\",\"protocol_version\":1,\"evaluator_id\":\"business\",\"status\":\"passed\",\"score\":1}'\nexit 9\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let case: Case = serde_json::from_value(json!({
            "id": "one", "input": {}, "source_line": 1
        }))
        .unwrap();
        let output: VariantOutput = serde_json::from_value(json!({
            "case_id": "one", "status": "ok", "raw_output": "{}", "metadata": {}, "retries": []
        }))
        .unwrap();
        let run = run_external_evaluator(
            "business",
            &EvaluatorKind::Command {
                command: CommandSpec {
                    program: script.display().to_string(),
                    args: vec![],
                },
                timeout_ms: 1000,
            },
            &case,
            &output,
            EvaluatorRuntime {
                variant_id: "baseline",
                working_directory: root.path(),
                python_bridge: Path::new("unused"),
                limits: &CommandLimits::default(),
            },
        );
        assert_eq!(run.result.status, EvaluationStatus::Error);
        assert!(run.result.message.contains("exited unsuccessfully"));
    }
}

//! Fail-closed command and Python evaluator execution.

use std::{
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

use serde::Deserialize;
use serde_json::{Value, json};
use structtrace_core::{
    artifact::ExternalEvaluatorReceipt,
    config::{CommandSpec, EvaluatorKind, ImplementationConfig, ProcessMode},
    dataset::Case,
    evaluation::{EvaluationStatus, EvaluatorResult, FieldEvaluationFact},
    hashing::hash_canonical_json,
    output::VariantOutput,
};
use tokio::{
    io::{AsyncWriteExt, BufReader},
    time::timeout,
};
use wait_timeout::ChildExt;

use crate::command::CommandLimits;

/// Evaluator protocol identifier.
pub const EVALUATOR_PROTOCOL: &str = "structtrace.evaluator";
/// Bundled Python evaluator bridge source.
pub const EVALUATOR_BRIDGE_SOURCE: &str =
    include_str!("../../../python/structtrace_evaluator_bridge.py");

const MAX_EVALUATOR_FIELDS: usize = 10_000;
const MAX_EVALUATOR_MESSAGE_BYTES: usize = 16 * 1024;
const MAX_EVALUATOR_DETAILS_BYTES: usize = 1024 * 1024;
const MAX_EVALUATOR_FIELD_VALUE_BYTES: usize = 256 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvaluatorResponse {
    protocol: String,
    protocol_version: u32,
    evaluator_id: String,
    #[serde(default)]
    case_id: Option<String>,
    status: String,
    #[serde(default)]
    score: Option<f64>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    details: Option<Value>,
    #[serde(default)]
    fields: Vec<FieldEvaluationFact>,
}

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

/// One case/output pair evaluated by a shared external worker.
#[derive(Debug, Clone, Copy)]
pub struct EvaluatorInvocation<'a> {
    /// Golden case, including expected values visible only to evaluators.
    pub case: &'a Case,
    /// Variant output being scored.
    pub output: &'a VariantOutput,
}

/// Execute an evaluator across a complete variant, reusing one bounded worker
/// when the evaluator is configured for persistent operation.
pub fn run_external_evaluator_batch(
    evaluator_id: &str,
    kind: &EvaluatorKind,
    implementation_version: Option<&str>,
    invocations: &[EvaluatorInvocation<'_>],
    runtime: EvaluatorRuntime<'_>,
) -> Vec<EvaluatorRun> {
    let mode = match kind {
        EvaluatorKind::Command { process_mode, .. }
        | EvaluatorKind::Python { process_mode, .. } => *process_mode,
        _ => ProcessMode::PerCase,
    };
    if matches!(mode, ProcessMode::PerCase) {
        return invocations
            .iter()
            .map(|invocation| {
                run_external_evaluator(
                    evaluator_id,
                    kind,
                    implementation_version,
                    invocation.case,
                    invocation.output,
                    runtime,
                )
            })
            .collect();
    }
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                let runtime_handle = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("Tokio evaluator runtime");
                runtime_handle.block_on(run_persistent_batch(
                    evaluator_id,
                    kind,
                    implementation_version,
                    invocations,
                    runtime,
                ))
            })
            .join()
            .unwrap_or_else(|_| {
                invocations
                    .iter()
                    .map(|invocation| {
                        receipt_run(
                            error(evaluator_id, "persistent evaluator worker panicked"),
                            Vec::new(),
                            evaluator_id,
                            invocation.case,
                            invocation.output,
                            runtime.variant_id,
                            kind,
                            implementation_version,
                        )
                    })
                    .collect()
            })
    })
}

async fn run_persistent_batch(
    evaluator_id: &str,
    kind: &EvaluatorKind,
    implementation_version: Option<&str>,
    invocations: &[EvaluatorInvocation<'_>],
    runtime: EvaluatorRuntime<'_>,
) -> Vec<EvaluatorRun> {
    let (command, timeout_ms) = evaluator_command(kind, runtime.python_bridge);
    let Some((command, timeout_ms)) = command.zip(timeout_ms) else {
        return invocations
            .iter()
            .map(|invocation| {
                receipt_run(
                    error(evaluator_id, "evaluator is not an external evaluator"),
                    Vec::new(),
                    evaluator_id,
                    invocation.case,
                    invocation.output,
                    runtime.variant_id,
                    kind,
                    implementation_version,
                )
            })
            .collect();
    };
    let mut child = match crate::command::spawn(&command, runtime.working_directory) {
        Ok(child) => child,
        Err(failure) => {
            return invocations
                .iter()
                .map(|invocation| {
                    receipt_run(
                        error(
                            evaluator_id,
                            format!("could not start evaluator: {failure}"),
                        ),
                        Vec::new(),
                        evaluator_id,
                        invocation.case,
                        invocation.output,
                        runtime.variant_id,
                        kind,
                        implementation_version,
                    )
                })
                .collect();
        }
    };
    let process_id = child.id();
    let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        crate::command::terminate_process_tree(&mut child, process_id).await;
        return invocations
            .iter()
            .map(|invocation| {
                receipt_run(
                    error(evaluator_id, "evaluator standard streams were unavailable"),
                    Vec::new(),
                    evaluator_id,
                    invocation.case,
                    invocation.output,
                    runtime.variant_id,
                    kind,
                    implementation_version,
                )
            })
            .collect();
    };
    let stderr_task = child.stderr.take().map(|stderr| {
        tokio::spawn(crate::command::drain_stderr(
            stderr,
            runtime.limits.max_stderr_bytes,
        ))
    });
    let mut stdout = BufReader::new(stdout);
    let mut results = Vec::with_capacity(invocations.len());
    let mut terminal_failure: Option<String> = None;
    for invocation in invocations {
        if let Some(message) = &terminal_failure {
            results.push(error(evaluator_id, message));
            continue;
        }
        let request = evaluator_request(
            evaluator_id,
            invocation.case,
            invocation.output,
            runtime.variant_id,
        );
        let encoded = match serde_json::to_vec(&request) {
            Ok(encoded) => encoded,
            Err(failure) => {
                let message = format!("could not encode evaluator request: {failure}");
                terminal_failure = Some(message.clone());
                results.push(error(evaluator_id, message));
                crate::command::terminate_process_tree(&mut child, process_id).await;
                continue;
            }
        };
        if let Err(failure) = stdin.write_all(&encoded).await {
            let message = format!("could not write evaluator request: {failure}");
            terminal_failure = Some(message.clone());
            results.push(error(evaluator_id, message));
            continue;
        }
        if let Err(failure) = stdin.write_all(b"\n").await {
            let message = format!("could not terminate evaluator request: {failure}");
            terminal_failure = Some(message.clone());
            results.push(error(evaluator_id, message));
            continue;
        }
        if let Err(failure) = stdin.flush().await {
            let message = format!("could not flush evaluator request: {failure}");
            terminal_failure = Some(message.clone());
            results.push(error(evaluator_id, message));
            continue;
        }
        let response = timeout(
            Duration::from_millis(timeout_ms),
            crate::command::read_limited_line(&mut stdout, runtime.limits.max_output_bytes),
        )
        .await;
        let parsed = match response {
            Err(_) => {
                let message = format!("evaluator exceeded configured timeout of {timeout_ms} ms");
                terminal_failure = Some(message.clone());
                crate::command::terminate_process_tree(&mut child, process_id).await;
                error(evaluator_id, message)
            }
            Ok(Err(failure)) => {
                let message = format!("could not read evaluator response: {failure:?}");
                terminal_failure = Some(message.clone());
                crate::command::terminate_process_tree(&mut child, process_id).await;
                error(evaluator_id, message)
            }
            Ok(Ok(None)) => {
                let message = "evaluator closed stdout before returning a response".to_owned();
                terminal_failure = Some(message.clone());
                error(evaluator_id, message)
            }
            Ok(Ok(Some(line))) => {
                parse_response_for_case(evaluator_id, &invocation.case.id, line.as_bytes())
            }
        };
        results.push(parsed);
    }
    drop(stdin);
    let mut invalidate_all = None;
    if terminal_failure.is_none() {
        match timeout(
            Duration::from_millis(20),
            crate::command::read_limited_line(&mut stdout, runtime.limits.max_output_bytes),
        )
        .await
        {
            Ok(Ok(Some(extra))) if !extra.trim().is_empty() => {
                invalidate_all = Some("evaluator emitted unsolicited extra stdout".to_owned())
            }
            Ok(Err(failure)) => {
                invalidate_all = Some(format!(
                    "could not verify evaluator completion: {failure:?}"
                ))
            }
            _ => {}
        }
    }
    match timeout(crate::command::PROCESS_SHUTDOWN_GRACE, child.wait()).await {
        Ok(Ok(status)) if !status.success() && terminal_failure.is_none() => {
            invalidate_all = Some(format!(
                "persistent evaluator exited unsuccessfully with {status}"
            ));
        }
        Ok(Err(failure)) if terminal_failure.is_none() => {
            invalidate_all = Some(format!(
                "could not wait for persistent evaluator: {failure}"
            ));
        }
        Err(_) if terminal_failure.is_none() => {
            invalidate_all = Some("persistent evaluator ignored EOF".to_owned());
            crate::command::terminate_process_tree(&mut child, process_id).await;
        }
        Err(_) => crate::command::terminate_process_tree(&mut child, process_id).await,
        _ => {}
    }
    if terminal_failure.is_none() && invalidate_all.is_none() {
        match timeout(
            Duration::from_millis(20),
            crate::command::read_limited_line(&mut stdout, runtime.limits.max_output_bytes),
        )
        .await
        {
            Ok(Ok(Some(extra))) if !extra.trim().is_empty() => {
                invalidate_all = Some("evaluator emitted unsolicited extra stdout".to_owned())
            }
            Ok(Err(failure)) => {
                invalidate_all = Some(format!(
                    "could not verify evaluator completion: {failure:?}"
                ))
            }
            _ => {}
        }
    }
    crate::command::terminate_remaining_process_tree(process_id).await;
    let mut diagnostics = Vec::new();
    let stderr = match stderr_task {
        Some(task) => crate::command::bounded_stderr_task(task, &mut diagnostics).await,
        None => Vec::new(),
    };
    if let Some(message) = invalidate_all {
        results.fill(error(evaluator_id, message));
    }
    invocations
        .iter()
        .zip(results)
        .enumerate()
        .map(|(index, (invocation, result))| {
            receipt_run(
                result,
                if index == 0 {
                    stderr.clone()
                } else {
                    Vec::new()
                },
                evaluator_id,
                invocation.case,
                invocation.output,
                runtime.variant_id,
                kind,
                implementation_version,
            )
        })
        .collect()
}

fn evaluator_command(
    kind: &EvaluatorKind,
    python_bridge: &Path,
) -> (Option<CommandSpec>, Option<u64>) {
    match kind {
        EvaluatorKind::Command {
            command,
            timeout_ms,
            ..
        } => (Some(command.clone()), Some(*timeout_ms)),
        EvaluatorKind::Python {
            interpreter,
            callable,
            timeout_ms,
            ..
        } => (
            Some(CommandSpec {
                program: interpreter.clone(),
                args: vec![
                    python_bridge.display().to_string(),
                    "--callable".to_owned(),
                    callable.clone(),
                ],
            }),
            Some(*timeout_ms),
        ),
        _ => (None, None),
    }
}

/// Execute one configured external evaluator for one case and variant output.
pub fn run_external_evaluator(
    evaluator_id: &str,
    kind: &EvaluatorKind,
    implementation_version: Option<&str>,
    case: &Case,
    output: &VariantOutput,
    runtime: EvaluatorRuntime<'_>,
) -> EvaluatorRun {
    let (command, timeout_ms) = match kind {
        EvaluatorKind::Command {
            command,
            timeout_ms,
            ..
        } => (command.clone(), *timeout_ms),
        EvaluatorKind::Python {
            interpreter,
            callable,
            timeout_ms,
            ..
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
                implementation_version,
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
        implementation_version,
    )
}

/// Canonical request object used by execution and replay receipt verification.
pub fn evaluator_request(
    evaluator_id: &str,
    case: &Case,
    output: &VariantOutput,
    variant_id: &str,
) -> Value {
    let output = stable_evaluator_output(output);
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

/// Build the retention-independent model-output view supplied to custom evaluators.
/// Valid JSON is canonicalized; provider envelopes, prompts, and raw formatting are excluded.
fn stable_evaluator_output(output: &VariantOutput) -> VariantOutput {
    let mut stable = output.clone();
    let parsed = stable.parsed_output.clone().or_else(|| {
        stable
            .raw_output
            .as_deref()
            .and_then(|raw| serde_json::from_str(raw).ok())
    });
    stable.parsed_output.clone_from(&parsed);
    stable.raw_output = parsed
        .as_ref()
        .and_then(|value| serde_json::to_string(value).ok());
    if let Some(metadata) = stable.metadata.as_object_mut() {
        metadata.remove("provider_response");
        metadata.remove("rendered_prompt");
    }
    for retry in &mut stable.retries {
        if let Some(object) = retry.as_object_mut() {
            object.remove("response");
        }
    }
    if let Some(error) = stable
        .error
        .as_mut()
        .filter(|error| error.kind == "provider_error")
    {
        error.message = "Provider rejected the request.".to_owned();
    }
    stable
}

#[allow(clippy::too_many_arguments)]
fn receipt_run(
    result: EvaluatorResult,
    stderr: Vec<u8>,
    evaluator_id: &str,
    case: &Case,
    output: &VariantOutput,
    variant_id: &str,
    kind: &EvaluatorKind,
    implementation_version: Option<&str>,
) -> EvaluatorRun {
    let request = evaluator_request(evaluator_id, case, output, variant_id);
    let response = serde_json::to_value(&result).unwrap_or(Value::Null);
    let definition = evaluator_definition(kind, implementation_version);
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

/// Canonical external-evaluator definition bound into replay receipts.
pub fn evaluator_definition(kind: &EvaluatorKind, implementation_version: Option<&str>) -> Value {
    evaluator_definition_with_implementation(
        kind,
        implementation_version,
        &ImplementationConfig::default(),
    )
}

/// Canonical evaluator definition including explicitly declared implementation inputs.
pub fn evaluator_definition_with_implementation(
    kind: &EvaluatorKind,
    implementation_version: Option<&str>,
    implementation: &ImplementationConfig,
) -> Value {
    json!({
        "kind": kind,
        "implementation_version": implementation_version,
        "implementation": implementation,
    })
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
    parse_response_inner(evaluator_id, None, bytes)
}

fn parse_response_for_case(evaluator_id: &str, case_id: &str, bytes: &[u8]) -> EvaluatorResult {
    parse_response_inner(evaluator_id, Some(case_id), bytes)
}

fn parse_response_inner(
    evaluator_id: &str,
    expected_case_id: Option<&str>,
    bytes: &[u8],
) -> EvaluatorResult {
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
    let response: EvaluatorResponse = match serde_json::from_str(lines[0]) {
        Ok(value) => value,
        Err(failure) => {
            return error(
                evaluator_id,
                format!("invalid evaluator response: {failure}"),
            );
        }
    };
    if response.protocol != EVALUATOR_PROTOCOL
        || response.protocol_version != structtrace_core::PROTOCOL_VERSION
        || response.evaluator_id != evaluator_id
    {
        return error(
            evaluator_id,
            "evaluator response identity or protocol did not match",
        );
    }
    if expected_case_id.is_some() && response.case_id.as_deref() != expected_case_id {
        return error(
            evaluator_id,
            "evaluator response case identity did not match",
        );
    }
    let status = match response.status.as_str() {
        "passed" => EvaluationStatus::Passed,
        "failed" => EvaluationStatus::Failed,
        "error" => EvaluationStatus::Error,
        "not_applicable" => EvaluationStatus::NotApplicable,
        _ => return error(evaluator_id, "evaluator response has an unsupported status"),
    };
    let score = response.score;
    if score.is_some_and(|score| !(0.0..=1.0).contains(&score)) {
        return error(evaluator_id, "evaluator score must be between zero and one");
    }
    let fields = response.fields;
    if fields.len() > MAX_EVALUATOR_FIELDS {
        return error(
            evaluator_id,
            "evaluator response contains too many field facts",
        );
    }
    if !fields.iter().all(|field| {
        valid_json_pointer(&field.pointer)
            && field
                .expected_pointer
                .as_ref()
                .is_none_or(|pointer| valid_json_pointer(pointer))
    }) {
        return error(
            evaluator_id,
            "evaluator field facts contain invalid JSON Pointers",
        );
    }
    if response
        .message
        .as_ref()
        .is_some_and(|message| message.len() > MAX_EVALUATOR_MESSAGE_BYTES)
        || response.details.as_ref().is_some_and(|details| {
            serde_json::to_vec(details)
                .map_or(true, |bytes| bytes.len() > MAX_EVALUATOR_DETAILS_BYTES)
        })
        || fields.iter().any(|field| {
            field.message.len() > MAX_EVALUATOR_MESSAGE_BYTES
                || [&field.expected, &field.actual].into_iter().any(|value| {
                    serde_json::to_vec(value)
                        .map_or(true, |bytes| bytes.len() > MAX_EVALUATOR_FIELD_VALUE_BYTES)
                })
        })
    {
        return error(
            evaluator_id,
            "evaluator response exceeds a semantic field limit",
        );
    }
    let has_failed = fields
        .iter()
        .any(|field| field.status == EvaluationStatus::Failed);
    let has_error = fields
        .iter()
        .any(|field| field.status == EvaluationStatus::Error);
    let has_resolved = fields.iter().any(|field| {
        matches!(
            field.status,
            EvaluationStatus::Passed | EvaluationStatus::Failed
        )
    });
    let consistent = match status {
        EvaluationStatus::Passed => !has_failed && !has_error,
        EvaluationStatus::Failed => {
            has_failed
                || response
                    .message
                    .as_deref()
                    .is_some_and(|message| !message.trim().is_empty())
        }
        EvaluationStatus::Error => score != Some(1.0) && !has_resolved,
        EvaluationStatus::NotApplicable => !has_resolved,
    };
    if !consistent {
        return error(
            evaluator_id,
            "evaluator status, score, message, and field facts are contradictory",
        );
    }
    EvaluatorResult {
        evaluator_id: evaluator_id.to_owned(),
        status,
        passed: status == EvaluationStatus::Passed,
        score,
        message: response
            .message
            .as_deref()
            .unwrap_or("External evaluator returned no message.")
            .to_owned(),
        details: response.details.unwrap_or(Value::Null),
        fields,
    }
}

fn valid_json_pointer(pointer: &str) -> bool {
    if !pointer.is_empty() && !pointer.starts_with('/') {
        return false;
    }
    let bytes = pointer.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'~' {
            if index + 1 >= bytes.len() || !matches!(bytes[index + 1], b'0' | b'1') {
                return false;
            }
            index += 2;
        } else {
            index += 1;
        }
    }
    true
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

    #[test]
    fn external_evaluator_field_facts_are_validated_and_preserved() {
        let response = serde_json::json!({
            "protocol": EVALUATOR_PROTOCOL,
            "protocol_version": structtrace_core::PROTOCOL_VERSION,
            "evaluator_id": "invoice",
            "case_id": "one",
            "status": "failed",
            "fields": [{
                "pointer": "/tax",
                "status": "failed",
                "expected": "18.00",
                "actual": "8.00",
                "message": "tax mismatch"
            }]
        });
        let result = parse_response_for_case(
            "invoice",
            "one",
            serde_json::to_string(&response).unwrap().as_bytes(),
        );
        assert_eq!(result.fields.len(), 1);
        assert_eq!(result.fields[0].pointer, "/tax");
        assert_eq!(result.fields[0].status, EvaluationStatus::Failed);
    }

    #[test]
    fn external_evaluator_unknown_fields_are_rejected() {
        let response = serde_json::json!({
            "protocol": EVALUATOR_PROTOCOL,
            "protocol_version": structtrace_core::PROTOCOL_VERSION,
            "evaluator_id": "business",
            "case_id": "one",
            "status": "passed",
            "unexpected": true
        });
        let result = parse_response_for_case(
            "business",
            "one",
            serde_json::to_string(&response).unwrap().as_bytes(),
        );
        assert_eq!(result.status, EvaluationStatus::Error);
        assert!(result.message.contains("unknown field"));
    }

    #[test]
    fn contradictory_field_facts_and_invalid_pointer_escapes_are_rejected() {
        for fields in [
            serde_json::json!([{
                "pointer": "/tax",
                "status": "failed",
                "expected": 18,
                "actual": 8,
                "message": "mismatch"
            }]),
            serde_json::json!([{
                "pointer": "/bad~escape",
                "status": "passed",
                "message": "invalid pointer"
            }]),
        ] {
            let response = serde_json::json!({
                "protocol": EVALUATOR_PROTOCOL,
                "protocol_version": structtrace_core::PROTOCOL_VERSION,
                "evaluator_id": "business",
                "case_id": "one",
                "status": "passed",
                "fields": fields
            });
            let result = parse_response_for_case(
                "business",
                "one",
                serde_json::to_string(&response).unwrap().as_bytes(),
            );
            assert_eq!(result.status, EvaluationStatus::Error);
        }
    }

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
            "#!/bin/sh\nread line\nprintf '%s\\n' '{\"protocol\":\"structtrace.evaluator\",\"protocol_version\":2,\"evaluator_id\":\"business\",\"status\":\"passed\",\"score\":1,\"message\":\"receipt verified\"}'\n",
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
                process_mode: structtrace_core::config::ProcessMode::Persistent,
                timeout_ms: 1000,
            },
            Some("test-command-v1"),
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
            "#!/bin/sh\nread line\nprintf '%s\\n' '{\"protocol\":\"structtrace.evaluator\",\"protocol_version\":2,\"evaluator_id\":\"business\",\"status\":\"passed\",\"score\":1}'\nexit 9\n",
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
                process_mode: structtrace_core::config::ProcessMode::Persistent,
                timeout_ms: 1000,
            },
            Some("test-command-v1"),
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

    #[cfg(unix)]
    #[test]
    fn persistent_evaluator_extra_stdout_invalidates_every_result() {
        let root = tempdir().unwrap();
        let python = ["python3", "python"]
            .into_iter()
            .find(|program| {
                std::process::Command::new(program)
                    .arg("--version")
                    .output()
                    .is_ok_and(|output| output.status.success())
            })
            .unwrap();
        std::fs::write(
            root.path().join("worker.py"),
            "import json,sys,time\nfor line in sys.stdin:\n r=json.loads(line); print(json.dumps({'protocol':'structtrace.evaluator','protocol_version':2,'evaluator_id':r['evaluator_id'],'case_id':r['case_id'],'status':'passed'}),flush=True)\ntime.sleep(0.1); print('extra',flush=True)\n",
        )
        .unwrap();
        let cases = [fixture_case("one"), fixture_case("two")];
        let outputs = [fixture_output("one"), fixture_output("two")];
        let invocations = cases
            .iter()
            .zip(&outputs)
            .map(|(case, output)| EvaluatorInvocation { case, output })
            .collect::<Vec<_>>();
        let kind = EvaluatorKind::Command {
            command: CommandSpec {
                program: python.to_owned(),
                args: vec!["worker.py".to_owned()],
            },
            process_mode: ProcessMode::Persistent,
            timeout_ms: 1000,
        };
        let runs = run_external_evaluator_batch(
            "business",
            &kind,
            Some("extra-v1"),
            &invocations,
            EvaluatorRuntime {
                variant_id: "baseline",
                working_directory: root.path(),
                python_bridge: Path::new("unused"),
                limits: &CommandLimits::default(),
            },
        );
        assert!(
            runs.iter()
                .all(|run| run.result.status == EvaluationStatus::Error)
        );
    }

    #[cfg(unix)]
    #[test]
    fn persistent_evaluator_kills_descendants_inheriting_streams() {
        let root = tempdir().unwrap();
        let python = ["python3", "python"]
            .into_iter()
            .find(|program| {
                std::process::Command::new(program)
                    .arg("--version")
                    .output()
                    .is_ok_and(|output| output.status.success())
            })
            .unwrap();
        std::fs::write(
            root.path().join("worker.py"),
            "import json,subprocess,sys\nfor line in sys.stdin:\n r=json.loads(line); print(json.dumps({'protocol':'structtrace.evaluator','protocol_version':2,'evaluator_id':r['evaluator_id'],'case_id':r['case_id'],'status':'passed'}),flush=True)\nsubprocess.Popen([sys.executable,'-c','import time; time.sleep(60)'])\n",
        )
        .unwrap();
        let cases = [fixture_case("one")];
        let outputs = [fixture_output("one")];
        let invocations = [EvaluatorInvocation {
            case: &cases[0],
            output: &outputs[0],
        }];
        let kind = EvaluatorKind::Command {
            command: CommandSpec {
                program: python.to_owned(),
                args: vec!["worker.py".to_owned()],
            },
            process_mode: ProcessMode::Persistent,
            timeout_ms: 1000,
        };
        let started = std::time::Instant::now();
        let runs = run_external_evaluator_batch(
            "business",
            &kind,
            Some("descendant-v1"),
            &invocations,
            EvaluatorRuntime {
                variant_id: "baseline",
                working_directory: root.path(),
                python_bridge: Path::new("unused"),
                limits: &CommandLimits::default(),
            },
        );
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(runs[0].result.status, EvaluationStatus::Passed);
    }

    fn fixture_case(id: &str) -> Case {
        serde_json::from_value(json!({"id": id, "input": {}, "source_line": 1})).unwrap()
    }

    fn fixture_output(id: &str) -> VariantOutput {
        serde_json::from_value(json!({
            "case_id": id,
            "status": "ok",
            "raw_output": "{}",
            "metadata": {},
            "retries": []
        }))
        .unwrap()
    }
}

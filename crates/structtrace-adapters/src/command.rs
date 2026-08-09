//! Language-agnostic command adapter using a strict JSONL protocol.

use std::{path::Path, process::Stdio, time::Duration};

use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
    time::{Instant, timeout},
};

use serde_json::Value;
use structtrace_core::{
    config::{CommandSpec, ProcessMode},
    dataset::VariantCase,
    output::{OutputError, OutputStatus, VariantOutput},
};

use crate::protocol::{VariantRequest, VariantResponse, validate_response};

/// Safety and retention limits for a subprocess adapter.
#[derive(Debug, Clone)]
pub struct CommandLimits {
    /// Maximum response line bytes.
    pub max_output_bytes: usize,
    /// Maximum retained standard-error bytes. Remaining bytes are drained.
    pub max_stderr_bytes: usize,
}

impl Default for CommandLimits {
    fn default() -> Self {
        Self {
            max_output_bytes: 4 * 1024 * 1024,
            max_stderr_bytes: 1024 * 1024,
        }
    }
}

/// Complete adapter result including retained process diagnostics.
#[derive(Debug, Clone)]
pub struct AdapterRun {
    /// One complete-denominator result per case in dataset order.
    pub rows: Vec<VariantOutput>,
    /// Retained standard error.
    pub stderr: Vec<u8>,
    /// Protocol-level diagnostics not attributable to one successful output.
    pub protocol_errors: Vec<String>,
}

/// Execute a configured command without a shell.
pub async fn run_command(
    spec: &CommandSpec,
    mode: ProcessMode,
    timeout_ms: u64,
    cases: &[VariantCase],
    working_directory: &Path,
    limits: &CommandLimits,
) -> AdapterRun {
    match mode {
        ProcessMode::Persistent => {
            run_persistent(spec, timeout_ms, cases, working_directory, limits).await
        }
        ProcessMode::PerCase => {
            run_per_case(spec, timeout_ms, cases, working_directory, limits).await
        }
    }
}

async fn run_persistent(
    spec: &CommandSpec,
    timeout_ms: u64,
    cases: &[VariantCase],
    working_directory: &Path,
    limits: &CommandLimits,
) -> AdapterRun {
    let mut child = match spawn(spec, working_directory) {
        Ok(child) => child,
        Err(error) => {
            return all_failed(cases, "process_spawn", &error.to_string());
        }
    };
    let Some(mut stdin) = child.stdin.take() else {
        return all_failed(cases, "process_spawn", "could not open child stdin");
    };
    let Some(stdout) = child.stdout.take() else {
        return all_failed(cases, "process_spawn", "could not open child stdout");
    };
    let stderr_task = child
        .stderr
        .take()
        .map(|stderr| tokio::spawn(drain_stderr(stderr, limits.max_stderr_bytes)));
    let mut stdout = BufReader::new(stdout);
    let duration = Duration::from_millis(timeout_ms);
    let mut rows = Vec::with_capacity(cases.len());
    let mut protocol_errors = Vec::new();
    let mut terminal_failure: Option<(String, String)> = None;
    for case in cases {
        if let Some((kind, message)) = &terminal_failure {
            rows.push(error_output(&case.id, kind, message, None));
            continue;
        }
        let request = VariantRequest::from(case);
        let serialized = match serde_json::to_vec(&request) {
            Ok(value) => value,
            Err(error) => {
                rows.push(error_output(
                    &case.id,
                    "protocol_encode",
                    &error.to_string(),
                    None,
                ));
                continue;
            }
        };
        let started = Instant::now();
        if let Err(error) = stdin.write_all(&serialized).await {
            let message = format!("could not write request: {error}");
            rows.push(error_output(
                &case.id,
                "process_terminated",
                &message,
                Some(elapsed_ms(started)),
            ));
            terminal_failure = Some(("process_terminated".to_owned(), message));
            continue;
        }
        if let Err(error) = stdin.write_all(b"\n").await {
            let message = format!("could not terminate request line: {error}");
            rows.push(error_output(
                &case.id,
                "process_terminated",
                &message,
                Some(elapsed_ms(started)),
            ));
            terminal_failure = Some(("process_terminated".to_owned(), message));
            continue;
        }
        if let Err(error) = stdin.flush().await {
            let message = format!("could not flush request: {error}");
            rows.push(error_output(
                &case.id,
                "process_terminated",
                &message,
                Some(elapsed_ms(started)),
            ));
            terminal_failure = Some(("process_terminated".to_owned(), message));
            continue;
        }
        match timeout(
            duration,
            read_limited_line(&mut stdout, limits.max_output_bytes),
        )
        .await
        {
            Err(_) => {
                let message = "variant exceeded the configured timeout".to_owned();
                rows.push(error_output(
                    &case.id,
                    "timeout",
                    &message,
                    Some(elapsed_ms(started)),
                ));
                terminal_failure = Some(("process_terminated_after_timeout".to_owned(), message));
                let _ = child.kill().await;
            }
            Ok(Err(LimitedLineError::Io(message))) => {
                let message = format!("could not read response: {message}");
                rows.push(error_output(
                    &case.id,
                    "protocol_io",
                    &message,
                    Some(elapsed_ms(started)),
                ));
                terminal_failure = Some(("process_terminated".to_owned(), message));
            }
            Ok(Err(LimitedLineError::InvalidUtf8(message))) => {
                protocol_errors.push(format!("{}: {message}", case.id));
                rows.push(error_output(
                    &case.id,
                    "protocol_violation",
                    &message,
                    Some(elapsed_ms(started)),
                ));
                terminal_failure = Some(("protocol_aborted".to_owned(), message));
                let _ = child.kill().await;
            }
            Ok(Err(LimitedLineError::Limit)) => {
                let message = format!(
                    "response exceeded the configured {}-byte limit",
                    limits.max_output_bytes
                );
                rows.push(error_output(
                    &case.id,
                    "output_limit",
                    &message,
                    Some(elapsed_ms(started)),
                ));
                terminal_failure = Some(("protocol_aborted".to_owned(), message));
                let _ = child.kill().await;
            }
            Ok(Ok(None)) => {
                let message = "process closed stdout before returning a response".to_owned();
                rows.push(error_output(
                    &case.id,
                    "process_terminated",
                    &message,
                    Some(elapsed_ms(started)),
                ));
                terminal_failure = Some(("process_terminated".to_owned(), message));
            }
            Ok(Ok(Some(line))) => {
                let latency = elapsed_ms(started);
                match parse_response(&line, &case.id, latency) {
                    Ok(row) => rows.push(row),
                    Err(error) => {
                        let message = error.to_string();
                        protocol_errors.push(format!("{}: {message}", case.id));
                        rows.push(error_output(
                            &case.id,
                            "protocol_violation",
                            &message,
                            Some(latency),
                        ));
                        terminal_failure = Some(("protocol_aborted".to_owned(), message));
                        let _ = child.kill().await;
                    }
                }
            }
        }
    }
    drop(stdin);
    if terminal_failure.is_none() {
        match timeout(
            Duration::from_millis(20),
            read_limited_line(&mut stdout, limits.max_output_bytes),
        )
        .await
        {
            Ok(Ok(Some(extra))) if !extra.trim().is_empty() => {
                protocol_errors.push("process emitted unsolicited extra stdout".to_owned());
            }
            _ => {}
        }
    }
    match timeout(Duration::from_secs(2), child.wait()).await {
        Ok(Ok(status)) if !status.success() && terminal_failure.is_none() => {
            let message = format!("persistent process exited unsuccessfully with {status}");
            protocol_errors.push(message.clone());
            for (case, row) in cases.iter().zip(&mut rows) {
                *row = error_output(&case.id, "process_exit", &message, row.latency_ms);
            }
        }
        Ok(Err(error)) if terminal_failure.is_none() => {
            protocol_errors.push(format!("could not wait for persistent process: {error}"));
        }
        _ => {}
    }
    let stderr = match stderr_task {
        Some(task) => task.await.unwrap_or_default(),
        None => Vec::new(),
    };
    AdapterRun {
        rows,
        stderr,
        protocol_errors,
    }
}

#[derive(Debug)]
enum LimitedLineError {
    Io(String),
    InvalidUtf8(String),
    Limit,
}

async fn read_limited_line<R>(
    reader: &mut BufReader<R>,
    max_bytes: usize,
) -> Result<Option<String>, LimitedLineError>
where
    R: AsyncRead + Unpin,
{
    let mut retained = Vec::new();
    loop {
        let available = reader
            .fill_buf()
            .await
            .map_err(|error| LimitedLineError::Io(error.to_string()))?;
        if available.is_empty() {
            if retained.is_empty() {
                return Ok(None);
            }
            break;
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let take = newline.map_or(available.len(), |index| index + 1);
        let content_bytes = newline.map_or(take, |index| index);
        if retained.len().saturating_add(content_bytes) > max_bytes {
            return Err(LimitedLineError::Limit);
        }
        retained.extend_from_slice(&available[..take]);
        reader.consume(take);
        if newline.is_some() {
            break;
        }
    }
    if retained.last() == Some(&b'\n') {
        retained.pop();
    }
    if retained.last() == Some(&b'\r') {
        retained.pop();
    }
    String::from_utf8(retained)
        .map(Some)
        .map_err(|error| LimitedLineError::InvalidUtf8(error.to_string()))
}

async fn run_per_case(
    spec: &CommandSpec,
    timeout_ms: u64,
    cases: &[VariantCase],
    working_directory: &Path,
    limits: &CommandLimits,
) -> AdapterRun {
    let mut rows = Vec::with_capacity(cases.len());
    let mut stderr = Vec::new();
    let mut protocol_errors = Vec::new();
    for case in cases {
        let started = Instant::now();
        let mut child = match spawn(spec, working_directory) {
            Ok(child) => child,
            Err(error) => {
                rows.push(error_output(
                    &case.id,
                    "process_spawn",
                    &error.to_string(),
                    None,
                ));
                continue;
            }
        };
        let request = VariantRequest::from(case);
        let encoded = match serde_json::to_vec(&request) {
            Ok(mut encoded) => {
                encoded.push(b'\n');
                encoded
            }
            Err(error) => {
                rows.push(error_output(
                    &case.id,
                    "protocol_encode",
                    &error.to_string(),
                    None,
                ));
                continue;
            }
        };
        let Some(mut child_stdin) = child.stdin.take() else {
            rows.push(error_output(
                &case.id,
                "process_spawn",
                "could not open child stdin",
                None,
            ));
            continue;
        };
        if let Err(error) = child_stdin.write_all(&encoded).await {
            rows.push(error_output(
                &case.id,
                "protocol_io",
                &error.to_string(),
                Some(elapsed_ms(started)),
            ));
            continue;
        }
        drop(child_stdin);
        let Some(child_stdout) = child.stdout.take() else {
            rows.push(error_output(
                &case.id,
                "process_spawn",
                "could not open child stdout",
                Some(elapsed_ms(started)),
            ));
            continue;
        };
        let Some(child_stderr) = child.stderr.take() else {
            rows.push(error_output(
                &case.id,
                "process_spawn",
                "could not open child stderr",
                Some(elapsed_ms(started)),
            ));
            continue;
        };
        let output_limit = limits.max_output_bytes;
        let stderr_limit = limits.max_stderr_bytes;
        let stdout_task = tokio::spawn(drain_bounded(child_stdout, output_limit));
        let stderr_task = tokio::spawn(drain_bounded(child_stderr, stderr_limit));
        let wait = timeout(Duration::from_millis(timeout_ms), child.wait()).await;
        let timed_out = wait.is_err();
        if timed_out {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        let stdout_result = stdout_task.await.unwrap_or_else(|error| {
            Err(std::io::Error::other(format!(
                "stdout reader task failed: {error}"
            )))
        });
        let stderr_result = stderr_task.await.unwrap_or_else(|error| {
            Err(std::io::Error::other(format!(
                "stderr reader task failed: {error}"
            )))
        });
        if let Ok((diagnostics, _)) = stderr_result {
            retain_bytes(&mut stderr, &diagnostics, limits.max_stderr_bytes);
        }
        if timed_out {
            rows.push(error_output(
                &case.id,
                "timeout",
                "variant exceeded the configured timeout",
                Some(elapsed_ms(started)),
            ));
            continue;
        }
        if let Ok(Err(error)) = &wait {
            rows.push(error_output(
                &case.id,
                "process_terminated",
                &error.to_string(),
                Some(elapsed_ms(started)),
            ));
            continue;
        }
        if let Ok(Ok(status)) = &wait {
            if !status.success() {
                rows.push(error_output(
                    &case.id,
                    "process_exit",
                    &format!("per-case process exited unsuccessfully with {status}"),
                    Some(elapsed_ms(started)),
                ));
                continue;
            }
        }
        let (stdout, overflowed) = match stdout_result {
            Ok(result) => result,
            Err(error) => {
                rows.push(error_output(
                    &case.id,
                    "protocol_io",
                    &error.to_string(),
                    Some(elapsed_ms(started)),
                ));
                continue;
            }
        };
        if overflowed {
            rows.push(error_output(
                &case.id,
                "output_limit",
                "response exceeded the configured byte limit",
                Some(elapsed_ms(started)),
            ));
            continue;
        }
        let stdout = match std::str::from_utf8(&stdout) {
            Ok(stdout) => stdout,
            Err(error) => {
                let message = format!("response was not valid UTF-8: {error}");
                protocol_errors.push(format!("{}: {message}", case.id));
                rows.push(error_output(
                    &case.id,
                    "protocol_violation",
                    &message,
                    Some(elapsed_ms(started)),
                ));
                continue;
            }
        };
        let nonempty = stdout
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect::<Vec<_>>();
        if nonempty.len() != 1 {
            let message = format!(
                "expected exactly one response line, observed {}",
                nonempty.len()
            );
            protocol_errors.push(format!("{}: {message}", case.id));
            rows.push(error_output(
                &case.id,
                "protocol_violation",
                &message,
                Some(elapsed_ms(started)),
            ));
        } else {
            match parse_response(nonempty[0], &case.id, elapsed_ms(started)) {
                Ok(row) => rows.push(row),
                Err(error) => {
                    protocol_errors.push(format!("{}: {error}", case.id));
                    rows.push(error_output(
                        &case.id,
                        "protocol_violation",
                        &error.to_string(),
                        Some(elapsed_ms(started)),
                    ));
                }
            }
        }
    }
    AdapterRun {
        rows,
        stderr,
        protocol_errors,
    }
}

fn spawn(spec: &CommandSpec, working_directory: &Path) -> std::io::Result<Child> {
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .current_dir(working_directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    command.spawn()
}

fn parse_response(line: &str, case_id: &str, latency_ms: u64) -> anyhow::Result<VariantOutput> {
    let response: VariantResponse = serde_json::from_str(line)?;
    validate_response(&response, case_id)?;
    let status = if response.status == "ok" {
        OutputStatus::Ok
    } else {
        OutputStatus::Error
    };
    let raw_output = response.raw_output.or_else(|| {
        response
            .output
            .as_ref()
            .and_then(|value| serde_json::to_string(value).ok())
    });
    let error = response.error.map(|error| OutputError {
        kind: error.kind,
        message: error.message,
    });
    Ok(VariantOutput {
        case_id: case_id.to_owned(),
        status,
        raw_output,
        parsed_output: response.output,
        error,
        latency_ms: Some(latency_ms),
        usage: response.usage,
        cost: None,
        metadata: response.metadata,
        retries: Vec::new(),
    })
}

fn error_output(
    case_id: &str,
    kind: &str,
    message: &str,
    latency_ms: Option<u64>,
) -> VariantOutput {
    VariantOutput {
        case_id: case_id.to_owned(),
        status: OutputStatus::Error,
        raw_output: None,
        parsed_output: None,
        error: Some(OutputError {
            kind: kind.to_owned(),
            message: message.to_owned(),
        }),
        latency_ms,
        usage: None,
        cost: None,
        metadata: Value::Object(serde_json::Map::new()),
        retries: Vec::new(),
    }
}

fn all_failed(cases: &[VariantCase], kind: &str, message: &str) -> AdapterRun {
    AdapterRun {
        rows: cases
            .iter()
            .map(|case| error_output(&case.id, kind, message, None))
            .collect(),
        stderr: Vec::new(),
        protocol_errors: vec![message.to_owned()],
    }
}

async fn drain_stderr(mut reader: impl AsyncRead + Unpin, limit: usize) -> Vec<u8> {
    drain_bounded(&mut reader, limit)
        .await
        .map_or_else(|_| Vec::new(), |(retained, _)| retained)
}

async fn drain_bounded(
    mut reader: impl AsyncRead + Unpin,
    limit: usize,
) -> std::io::Result<(Vec<u8>, bool)> {
    let mut retained = Vec::with_capacity(limit.min(8192));
    let mut overflowed = false;
    let mut buffer = [0_u8; 8192];
    loop {
        let count = reader.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        overflowed |= retained.len().saturating_add(count) > limit;
        retain_bytes(&mut retained, &buffer[..count], limit);
    }
    Ok((retained, overflowed))
}

fn retain_bytes(destination: &mut Vec<u8>, source: &[u8], limit: usize) {
    let available = limit.saturating_sub(destination.len());
    destination.extend_from_slice(&source[..source.len().min(available)]);
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::{Value, json};
    use tempfile::tempdir;

    use super::*;

    // Cold Python startup on Windows CI can exceed one second. These tests
    // exercise protocol behavior, not startup latency, so keep a generous
    // platform-independent deadline and test timeout behavior separately.
    const FIXTURE_TIMEOUT_MS: u64 = 10_000;

    const HELPER: &str = r#"import argparse
import json
import sys
import time

parser = argparse.ArgumentParser()
parser.add_argument("--mode", default="success")
args = parser.parse_args()

for line in sys.stdin:
    request = json.loads(line)
    case_id = request["case_id"]
    if args.mode == "crash":
        sys.exit(17)
    if args.mode == "timeout":
        time.sleep(1)
    if args.mode == "stderr":
        print("diagnostic for " + case_id, file=sys.stderr, flush=True)
    if args.mode == "malformed":
        print("not-json", flush=True)
        continue
    if args.mode == "invalid-utf8":
        sys.stdout.buffer.write(b"\xff\n")
        sys.stdout.buffer.flush()
        continue
    response = {
        "protocol": "structtrace.variant",
        "protocol_version": 1,
        "case_id": "wrong" if args.mode == "wrong-id" else case_id,
        "status": "ok",
        "output": {"label": "accepted"},
    }
    if args.mode == "oversized":
        response["output"]["label"] = "x" * 4096
    print(json.dumps(response), flush=True)
    if args.mode == "duplicate":
        print(json.dumps(response), flush=True)
    if args.mode == "nonzero":
        sys.exit(9)
"#;

    fn cases() -> Vec<VariantCase> {
        vec![
            VariantCase {
                id: "one".to_owned(),
                input: json!({"text": "first"}),
                metadata: None,
            },
            VariantCase {
                id: "two".to_owned(),
                input: json!({"text": "second"}),
                metadata: None,
            },
        ]
    }

    fn fixture(mode: &str) -> (tempfile::TempDir, CommandSpec) {
        let directory = tempdir().unwrap();
        fs::write(directory.path().join("helper.py"), HELPER).unwrap();
        let python = ["python3", "python"]
            .into_iter()
            .find(|program| {
                std::process::Command::new(program)
                    .arg("--version")
                    .output()
                    .is_ok_and(|output| output.status.success())
            })
            .expect("Python is required for adapter fixture tests");
        let spec = CommandSpec {
            program: python.to_owned(),
            args: vec!["helper.py".to_owned(), "--mode".to_owned(), mode.to_owned()],
        };
        (directory, spec)
    }

    #[tokio::test]
    async fn persistent_process_returns_one_matching_row_per_case() {
        let (directory, spec) = fixture("success");
        let run = run_command(
            &spec,
            ProcessMode::Persistent,
            FIXTURE_TIMEOUT_MS,
            &cases(),
            directory.path(),
            &CommandLimits::default(),
        )
        .await;
        assert_eq!(run.rows.len(), 2);
        assert!(run.rows.iter().all(|row| row.status == OutputStatus::Ok));
        assert_eq!(
            run.rows[0].parsed_output,
            Some(json!({"label": "accepted"}))
        );
    }

    #[tokio::test]
    async fn per_case_process_is_supported() {
        let (directory, spec) = fixture("success");
        let run = run_command(
            &spec,
            ProcessMode::PerCase,
            FIXTURE_TIMEOUT_MS,
            &cases(),
            directory.path(),
            &CommandLimits::default(),
        )
        .await;
        assert_eq!(run.rows.len(), 2);
        assert!(run.rows.iter().all(|row| row.status == OutputStatus::Ok));
    }

    #[tokio::test]
    async fn malformed_and_wrong_id_responses_fail_closed() {
        for mode in ["malformed", "wrong-id"] {
            let (directory, spec) = fixture(mode);
            let run = run_command(
                &spec,
                ProcessMode::Persistent,
                FIXTURE_TIMEOUT_MS,
                &cases(),
                directory.path(),
                &CommandLimits::default(),
            )
            .await;
            assert_eq!(run.rows.len(), 2);
            assert_eq!(run.rows[0].status, OutputStatus::Error);
            assert_eq!(
                run.rows[0].error.as_ref().map(|error| error.kind.as_str()),
                Some("protocol_violation")
            );
            assert!(!run.protocol_errors.is_empty());
        }
    }

    #[tokio::test]
    async fn crash_marks_pending_cases_as_failures() {
        let (directory, spec) = fixture("crash");
        let run = run_command(
            &spec,
            ProcessMode::Persistent,
            FIXTURE_TIMEOUT_MS,
            &cases(),
            directory.path(),
            &CommandLimits::default(),
        )
        .await;
        assert_eq!(run.rows.len(), 2);
        assert!(run.rows.iter().all(|row| row.status == OutputStatus::Error));
    }

    #[tokio::test]
    async fn timeout_remains_in_the_denominator() {
        let (directory, spec) = fixture("timeout");
        let run = run_command(
            &spec,
            ProcessMode::Persistent,
            25,
            &cases(),
            directory.path(),
            &CommandLimits::default(),
        )
        .await;
        assert_eq!(run.rows.len(), 2);
        assert_eq!(
            run.rows[0].error.as_ref().map(|error| error.kind.as_str()),
            Some("timeout")
        );
        assert_eq!(run.rows[1].status, OutputStatus::Error);
    }

    #[tokio::test]
    async fn stderr_is_captured_separately() {
        let (directory, spec) = fixture("stderr");
        let run = run_command(
            &spec,
            ProcessMode::Persistent,
            FIXTURE_TIMEOUT_MS,
            &cases(),
            directory.path(),
            &CommandLimits::default(),
        )
        .await;
        assert!(String::from_utf8_lossy(&run.stderr).contains("diagnostic for one"));
        assert!(run.rows.iter().all(|row| row.status == OutputStatus::Ok));
    }

    #[tokio::test]
    async fn duplicate_per_case_response_is_a_protocol_violation() {
        let (directory, spec) = fixture("duplicate");
        let run = run_command(
            &spec,
            ProcessMode::PerCase,
            FIXTURE_TIMEOUT_MS,
            &cases()[..1],
            directory.path(),
            &CommandLimits::default(),
        )
        .await;
        assert_eq!(run.rows[0].status, OutputStatus::Error);
        assert!(!run.protocol_errors.is_empty());
    }

    #[tokio::test]
    async fn output_limit_is_enforced_while_streaming_in_both_process_modes() {
        for mode in [ProcessMode::Persistent, ProcessMode::PerCase] {
            let (directory, spec) = fixture("oversized");
            let run = run_command(
                &spec,
                mode,
                FIXTURE_TIMEOUT_MS,
                &cases()[..1],
                directory.path(),
                &CommandLimits {
                    max_output_bytes: 256,
                    max_stderr_bytes: 1024,
                },
            )
            .await;
            assert_eq!(
                run.rows[0].error.as_ref().map(|error| error.kind.as_str()),
                Some("output_limit")
            );
        }
    }

    #[tokio::test]
    async fn invalid_utf8_fails_protocol_in_both_process_modes() {
        for mode in [ProcessMode::Persistent, ProcessMode::PerCase] {
            let (directory, spec) = fixture("invalid-utf8");
            let run = run_command(
                &spec,
                mode,
                FIXTURE_TIMEOUT_MS,
                &cases()[..1],
                directory.path(),
                &CommandLimits::default(),
            )
            .await;
            assert_eq!(
                run.rows[0].error.as_ref().map(|error| error.kind.as_str()),
                Some("protocol_violation")
            );
        }
    }

    #[tokio::test]
    async fn nonzero_variant_exit_is_an_adapter_error() {
        for mode in [ProcessMode::Persistent, ProcessMode::PerCase] {
            let (directory, spec) = fixture("nonzero");
            let run = run_command(
                &spec,
                mode,
                FIXTURE_TIMEOUT_MS,
                &cases()[..1],
                directory.path(),
                &CommandLimits::default(),
            )
            .await;
            assert_eq!(run.rows[0].status, OutputStatus::Error);
            assert_eq!(
                run.rows[0].error.as_ref().map(|error| error.kind.as_str()),
                Some("process_exit")
            );
        }
    }

    #[test]
    fn error_output_has_empty_object_metadata() {
        assert_eq!(
            error_output("id", "kind", "message", None).metadata,
            Value::Object(serde_json::Map::new())
        );
    }
}

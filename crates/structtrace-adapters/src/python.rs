//! Python-callable adapter implemented through the command protocol.

use std::path::Path;

use structtrace_core::{
    config::{CommandSpec, ProcessMode},
    dataset::VariantCase,
};

use crate::command::{AdapterRun, CommandLimits, run_command};

/// Invoke a Python callable through the bundled bridge script.
pub async fn run_python(
    interpreter: &str,
    callable: &str,
    timeout_ms: u64,
    cases: &[VariantCase],
    working_directory: &Path,
    bridge_path: &Path,
    limits: &CommandLimits,
) -> AdapterRun {
    let spec = CommandSpec {
        program: interpreter.to_owned(),
        args: vec![
            bridge_path.display().to_string(),
            "--callable".to_owned(),
            callable.to_owned(),
        ],
    };
    run_command(
        &spec,
        ProcessMode::Persistent,
        timeout_ms,
        cases,
        working_directory,
        limits,
    )
    .await
}

/// Bundled bridge source for installation beside local run state.
pub const BRIDGE_SOURCE: &str = include_str!("../../../python/structtrace_bridge.py");

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;
    use tempfile::tempdir;

    use structtrace_core::{
        dataset::{ExecutionToken, VariantCase},
        output::OutputStatus,
    };

    use super::*;

    // Windows CI can spend more than one second starting a cold Python
    // interpreter. These tests exercise bridge semantics, not startup latency;
    // timeout behavior is covered independently by the command adapter tests.
    const BRIDGE_TEST_TIMEOUT_MS: u64 = 10_000;

    fn python_program() -> &'static str {
        ["python3", "python"]
            .into_iter()
            .find(|program| {
                std::process::Command::new(program)
                    .arg("--version")
                    .output()
                    .is_ok_and(|output| output.status.success())
            })
            .expect("Python is required for bridge tests")
    }

    fn case() -> VariantCase {
        VariantCase::from_parts(
            ExecutionToken::new("python-test", 0),
            json!({"text": "hello"}),
            None,
        )
    }

    #[tokio::test]
    async fn dictionary_and_string_results_are_supported() {
        let root = tempdir().unwrap();
        fs::write(root.path().join("bridge.py"), BRIDGE_SOURCE).unwrap();
        fs::write(
            root.path().join("app.py"),
            "def dictionary(case):\n    return {'label': case['input']['text']}\n\ndef string(case):\n    return '{\"label\":\"raw\"}'\n",
        )
        .unwrap();
        for (callable, expected) in [
            ("app:dictionary", json!({"label": "hello"})),
            ("app:string", json!({"label": "raw"})),
        ] {
            let run = run_python(
                python_program(),
                callable,
                BRIDGE_TEST_TIMEOUT_MS,
                &[case()],
                root.path(),
                &root.path().join("bridge.py"),
                &CommandLimits::default(),
            )
            .await;
            assert_eq!(run.rows[0].status, OutputStatus::Ok);
            let parsed: serde_json::Value =
                serde_json::from_str(run.rows[0].raw_output.as_deref().unwrap()).unwrap();
            assert_eq!(parsed, expected);
        }
    }

    #[tokio::test]
    async fn python_exception_becomes_a_retained_error() {
        let root = tempdir().unwrap();
        fs::write(root.path().join("bridge.py"), BRIDGE_SOURCE).unwrap();
        fs::write(
            root.path().join("app.py"),
            "def fail(case):\n    raise RuntimeError('intentional')\n",
        )
        .unwrap();
        let run = run_python(
            python_program(),
            "app:fail",
            BRIDGE_TEST_TIMEOUT_MS,
            &[case()],
            root.path(),
            &root.path().join("bridge.py"),
            &CommandLimits::default(),
        )
        .await;
        assert_eq!(run.rows[0].status, OutputStatus::Error);
        assert_eq!(
            run.rows[0].error.as_ref().map(|error| error.kind.as_str()),
            Some("python_exception")
        );
        assert!(run.stderr.is_empty());
        assert_eq!(
            run.rows[0]
                .error
                .as_ref()
                .map(|error| error.message.as_str()),
            Some("Python callable failed with RuntimeError")
        );
    }

    #[tokio::test]
    async fn python_callable_cannot_receive_expected() {
        let root = tempdir().unwrap();
        fs::write(root.path().join("bridge.py"), BRIDGE_SOURCE).unwrap();
        fs::write(
            root.path().join("app.py"),
            "def keys(case):\n    return {'keys': sorted(case.keys()), 'has_expected': 'expected' in case}\n",
        )
        .unwrap();
        let run = run_python(
            python_program(),
            "app:keys",
            BRIDGE_TEST_TIMEOUT_MS,
            &[case()],
            root.path(),
            &root.path().join("bridge.py"),
            &CommandLimits::default(),
        )
        .await;
        let parsed: serde_json::Value =
            serde_json::from_str(run.rows[0].raw_output.as_deref().unwrap()).unwrap();
        assert_eq!(parsed.pointer("/has_expected"), Some(&json!(false)));
        assert_eq!(parsed.pointer("/keys"), Some(&json!(["input", "metadata"])));
    }

    #[tokio::test]
    async fn async_and_nonserializable_results_are_isolated_per_case() {
        let root = tempdir().unwrap();
        fs::write(root.path().join("bridge.py"), BRIDGE_SOURCE).unwrap();
        fs::write(
            root.path().join("app.py"),
            "async def async_ok(case):\n    return {'label': case['input']['text']}\n\ndef mixed(case):\n    return {1, 2} if case['input']['text'] == 'hello' else {'label': 'ok'}\n",
        )
        .unwrap();
        let async_run = run_python(
            python_program(),
            "app:async_ok",
            BRIDGE_TEST_TIMEOUT_MS,
            &[case()],
            root.path(),
            &root.path().join("bridge.py"),
            &CommandLimits::default(),
        )
        .await;
        assert_eq!(async_run.rows[0].status, OutputStatus::Ok);

        let mut second = case();
        second.id = "two".to_owned();
        second.input = json!({"text": "later"});
        let mixed = run_python(
            python_program(),
            "app:mixed",
            BRIDGE_TEST_TIMEOUT_MS,
            &[case(), second],
            root.path(),
            &root.path().join("bridge.py"),
            &CommandLimits::default(),
        )
        .await;
        assert_eq!(mixed.rows[0].status, OutputStatus::Error);
        assert_eq!(
            mixed.rows[0]
                .error
                .as_ref()
                .map(|error| error.kind.as_str()),
            Some("non_serializable_output")
        );
        assert_eq!(mixed.rows[1].status, OutputStatus::Ok);
        assert!(mixed.stderr.is_empty());
    }

    #[tokio::test]
    async fn persistent_loop_stdout_capture_and_common_values_are_supported() {
        let root = tempdir().unwrap();
        fs::write(root.path().join("bridge.py"), BRIDGE_SOURCE).unwrap();
        fs::write(
            root.path().join("app.py"),
            r#"print("IMPORT_BANNER")
import asyncio
from dataclasses import dataclass

first_loop = None

async def stable_loop(case):
    global first_loop
    current = asyncio.get_running_loop()
    if first_loop is None:
        first_loop = current
    if current is not first_loop:
        raise RuntimeError("loop changed")
    print("USER_DEBUG_ON_STDOUT")
    return {"same_loop": True}

@dataclass
class Result:
    label: str

def dataclass_result(case):
    return Result("ok")

def ordinary_protocol_field(case):
    return {"protocol": "structtrace.variant", "business_value": 7}
"#,
        )
        .unwrap();
        let mut second = case();
        second.id = "two".to_owned();
        let loop_run = run_python(
            python_program(),
            "app:stable_loop",
            BRIDGE_TEST_TIMEOUT_MS,
            &[case(), second],
            root.path(),
            &root.path().join("bridge.py"),
            &CommandLimits::default(),
        )
        .await;
        assert!(
            loop_run
                .rows
                .iter()
                .all(|row| row.status == OutputStatus::Ok)
        );
        let stderr = String::from_utf8(loop_run.stderr).unwrap();
        assert!(stderr.contains("IMPORT_BANNER"));
        assert!(stderr.contains("USER_DEBUG_ON_STDOUT"));

        for (callable, pointer) in [
            ("app:dataclass_result", "/label"),
            ("app:ordinary_protocol_field", "/business_value"),
        ] {
            let run = run_python(
                python_program(),
                callable,
                BRIDGE_TEST_TIMEOUT_MS,
                &[case()],
                root.path(),
                &root.path().join("bridge.py"),
                &CommandLimits::default(),
            )
            .await;
            assert_eq!(run.rows[0].status, OutputStatus::Ok);
            let parsed: serde_json::Value =
                serde_json::from_str(run.rows[0].raw_output.as_deref().unwrap()).unwrap();
            assert!(parsed.pointer(pointer).is_some());
        }
    }

    #[tokio::test]
    async fn startup_import_failure_is_sanitized_per_case() {
        let root = tempdir().unwrap();
        fs::write(root.path().join("bridge.py"), BRIDGE_SOURCE).unwrap();
        let run = run_python(
            python_program(),
            "missing_private_module:run",
            BRIDGE_TEST_TIMEOUT_MS,
            &[case()],
            root.path(),
            &root.path().join("bridge.py"),
            &CommandLimits::default(),
        )
        .await;
        assert_eq!(run.rows[0].status, OutputStatus::Error);
        let error = run.rows[0].error.as_ref().unwrap();
        assert_eq!(error.kind, "startup_error");
        assert_eq!(
            error.message,
            "Python callable failed with ModuleNotFoundError"
        );
        assert!(error.fingerprint.is_some());
        assert!(!String::from_utf8(run.stderr).unwrap().contains("Traceback"));
    }

    #[tokio::test]
    async fn python_values_follow_the_documented_json_policy_and_bad_cases_are_isolated() {
        let root = tempdir().unwrap();
        fs::write(root.path().join("bridge.py"), BRIDGE_SOURCE).unwrap();
        fs::write(
            root.path().join("app.py"),
            r#"import asyncio
import datetime
import decimal
import uuid

def values(case):
    mode = case['input']['mode']
    if mode == 'nan': return {'value': float('nan')}
    if mode == 'infinity': return {'value': float('inf')}
    if mode == 'key': return {1: 'not allowed'}
    if mode == 'bytes': return {'value': b'private'}
    if mode == 'datetime': return {'value': datetime.datetime(2026, 8, 10, 12, 30, tzinfo=datetime.timezone.utc)}
    if mode == 'uuid': return {'value': uuid.UUID('12345678-1234-5678-1234-567812345678')}
    if mode == 'decimal': return {'value': decimal.Decimal('1.2300')}
    return {'value': 'later'}

async def leaves_task(case):
    asyncio.create_task(asyncio.sleep(60))
    return {'value': 'ok'}
"#,
        )
        .unwrap();
        let cases = [
            "nan", "infinity", "key", "bytes", "datetime", "uuid", "decimal", "ok",
        ]
        .into_iter()
        .enumerate()
        .map(|(index, mode)| {
            VariantCase::from_parts(
                ExecutionToken::new("normalization-test", index),
                json!({"mode": mode}),
                None,
            )
        })
        .collect::<Vec<_>>();
        let run = run_python(
            python_program(),
            "app:values",
            BRIDGE_TEST_TIMEOUT_MS,
            &cases,
            root.path(),
            &root.path().join("bridge.py"),
            &CommandLimits::default(),
        )
        .await;
        for (index, kind) in [
            "non_finite_number",
            "non_finite_number",
            "non_string_key",
            "bytes_require_wrapper",
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(run.rows[index].status, OutputStatus::Error);
            assert_eq!(run.rows[index].error.as_ref().unwrap().kind, kind);
        }
        assert!(
            run.rows[4..]
                .iter()
                .all(|row| row.status == OutputStatus::Ok)
        );
        let values = run.rows[4..7]
            .iter()
            .map(|row| {
                structtrace_core::strict_json::value_from_str(row.raw_output.as_deref().unwrap())
                    .unwrap()["value"]
                    .clone()
            })
            .collect::<Vec<_>>();
        assert_eq!(values[0], json!("2026-08-10T12:30:00+00:00"));
        assert_eq!(values[1], json!("12345678-1234-5678-1234-567812345678"));
        assert_eq!(values[2], json!("1.2300"));

        let pending = run_python(
            python_program(),
            "app:leaves_task",
            BRIDGE_TEST_TIMEOUT_MS,
            &cases[..1],
            root.path(),
            &root.path().join("bridge.py"),
            &CommandLimits::default(),
        )
        .await;
        assert_eq!(pending.rows[0].status, OutputStatus::Ok);
        assert!(
            !String::from_utf8(pending.stderr)
                .unwrap()
                .contains("Task was destroyed")
        );
    }
}

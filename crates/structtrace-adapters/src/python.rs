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

    use structtrace_core::{dataset::VariantCase, output::OutputStatus};

    use super::*;

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
        VariantCase {
            id: "one".to_owned(),
            input: json!({"text": "hello"}),
            metadata: None,
        }
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
                1_000,
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
            1_000,
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
            Some("RuntimeError")
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
            1_000,
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
            1_000,
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
            1_000,
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
}

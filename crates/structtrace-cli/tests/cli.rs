//! Real-binary acceptance tests for the stable command surface.

use std::process::Command;

use tempfile::tempdir;

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_structtrace"))
}

#[test]
fn help_and_doctor_succeed() {
    assert!(binary().arg("--help").status().unwrap().success());
    assert!(binary().arg("doctor").status().unwrap().success());
}

#[test]
fn documented_deployment_ci_command_is_authorization_safe() {
    let docs = include_str!("../../../docs/src/ci-integration.md");
    let video = include_str!("../../../docs/video-scripts/ci-integration.md");
    for source in [docs, video] {
        assert!(source.contains("release-check latest"));
        assert!(!source.contains("Enforce release thresholds\n  run: structtrace gate"));
    }
}

#[test]
fn strict_doctor_fails_expected_leaf_leakage() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("data.jsonl"),
        "{\"id\":\"currency-usd\",\"input\":{\"hint\":\"USD\"},\"expected\":{\"currency\":\"USD\"}}\n",
    )
    .unwrap();
    std::fs::write(root.path().join("schema.json"), "{\"type\":\"object\"}").unwrap();
    let output = "{\"case_id\":\"currency-usd\",\"status\":\"ok\",\"raw_output\":\"{}\"}\n";
    std::fs::write(root.path().join("baseline.jsonl"), output).unwrap();
    std::fs::write(root.path().join("candidate.jsonl"), output).unwrap();
    std::fs::write(
        root.path().join("structtrace.yaml"),
        r#"version: 3
project: {name: leakage-test}
dataset: {path: data.jsonl}
schema: {path: schema.json}
variants:
  baseline: {kind: recorded, path: baseline.jsonl}
  candidate: {kind: recorded, path: candidate.jsonl}
evaluators: [{id: exact, kind: exact_json}]
outcomes: {correct: {all_of: [exact]}}
analysis: {primary_outcome: correct}
"#,
    )
    .unwrap();
    let status = binary()
        .args([
            "--quiet",
            "--project-root",
            root.path().to_str().unwrap(),
            "doctor",
            "--strict",
        ])
        .status()
        .unwrap();
    assert!(!status.success());
}

#[test]
fn strict_doctor_requires_a_project_but_opaque_case_ids_are_allowed() {
    let empty = tempdir().unwrap();
    let strict_missing = binary()
        .args([
            "--quiet",
            "--project-root",
            empty.path().to_str().unwrap(),
            "doctor",
            "--strict",
        ])
        .status()
        .unwrap();
    assert!(!strict_missing.success());
    assert!(
        binary()
            .args([
                "--quiet",
                "--project-root",
                empty.path().to_str().unwrap(),
                "doctor",
            ])
            .status()
            .unwrap()
            .success()
    );

    let root = tempdir().unwrap();
    std::fs::write(
        root.path().join("data.jsonl"),
        "{\"id\":\"currency-usd\",\"input\":{\"text\":\"invoice\"},\"expected\":{\"currency\":\"USD\"}}\n",
    )
    .unwrap();
    std::fs::write(root.path().join("schema.json"), "{\"type\":\"object\"}").unwrap();
    let output = "{\"case_id\":\"currency-usd\",\"status\":\"ok\",\"raw_output\":\"{\\\"currency\\\":\\\"USD\\\"}\"}\n";
    std::fs::write(root.path().join("baseline.jsonl"), output).unwrap();
    std::fs::write(root.path().join("candidate.jsonl"), output).unwrap();
    std::fs::write(
        root.path().join("structtrace.yaml"),
        r#"version: 3
project: {name: opaque-id-test}
dataset: {path: data.jsonl}
schema: {path: schema.json}
variants:
  baseline: {kind: recorded, path: baseline.jsonl}
  candidate: {kind: recorded, path: candidate.jsonl}
evaluators: [{id: exact, kind: exact_json}]
outcomes: {correct: {all_of: [exact]}}
analysis: {primary_outcome: correct}
gate: {max_primary_regression_pp: 0}
"#,
    )
    .unwrap();
    assert!(
        binary()
            .args([
                "--quiet",
                "--project-root",
                root.path().to_str().unwrap(),
                "doctor",
                "--strict",
            ])
            .status()
            .unwrap()
            .success()
    );
}

#[test]
fn strict_doctor_is_static_and_handshake_warns_before_importing() {
    let root = tempdir().unwrap();
    let project = root.path().join("doctor-project");
    assert!(
        binary()
            .args(["init", project.to_str().unwrap(), "--template", "python"])
            .status()
            .unwrap()
            .success()
    );
    std::fs::write(
        project.join("variants/app.py"),
        r#"from pathlib import Path
def _run(case):
    Path("business-case-executed").write_text("yes")
    return {"label": "accepted", "reason": "explicit doctor fixture"}
def baseline(case): return _run(case)
def candidate(case): return _run(case)
"#,
    )
    .unwrap();
    for extra in [vec![], vec!["--handshake"]] {
        assert!(
            binary()
                .args([
                    "--quiet",
                    "--project-root",
                    project.to_str().unwrap(),
                    "doctor",
                    "--strict",
                ])
                .args(extra)
                .status()
                .unwrap()
                .success()
        );
        assert!(!project.join("business-case-executed").exists());
    }
    let handshake = binary()
        .args([
            "--format",
            "json",
            "--project-root",
            project.to_str().unwrap(),
            "doctor",
            "--strict",
            "--handshake",
        ])
        .output()
        .unwrap();
    assert!(handshake.status.success());
    assert!(String::from_utf8_lossy(&handshake.stdout).contains("Import-time code will execute"));
    let output = binary()
        .args([
            "--format",
            "json",
            "--project-root",
            project.to_str().unwrap(),
            "doctor",
            "--strict",
            "--execute-cases",
            "1",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(project.join("business-case-executed").is_file());
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("may make network calls or cause side effects")
    );
}

#[test]
fn initialized_recorded_project_runs_reports_replays_and_rejects_small_sample() {
    let root = tempdir().unwrap();
    let project = root.path().join("project");
    assert!(
        binary()
            .args(["init", project.to_str().unwrap(), "--template", "recorded"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        binary()
            .args([
                "--project-root",
                project.to_str().unwrap(),
                "compare",
                "--dataset",
                project.join("data/golden.jsonl").to_str().unwrap(),
                "--baseline",
                project.join("outputs/baseline.jsonl").to_str().unwrap(),
                "--candidate",
                project.join("outputs/candidate.jsonl").to_str().unwrap(),
                "--schema",
                project.join("schemas/output.schema.json").to_str().unwrap(),
            ])
            .status()
            .unwrap()
            .success()
    );
    let run_dir = std::fs::read_dir(project.join(".structtrace/runs"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let summary_markdown = std::fs::read_to_string(run_dir.join("summary.md")).unwrap();
    assert!(summary_markdown.contains("This is not release authorization."));
    assert!(!summary_markdown.contains("**Release gate:"));
    let incomplete_dir = project.join(".structtrace/runs/ZZZZ-INCOMPLETE");
    std::fs::create_dir_all(&incomplete_dir).unwrap();
    let mut incomplete_manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(run_dir.join("manifest.json")).unwrap()).unwrap();
    incomplete_manifest["status"] = serde_json::Value::String("failed".to_owned());
    std::fs::write(
        incomplete_dir.join("manifest.json"),
        serde_json::to_vec_pretty(&incomplete_manifest).unwrap(),
    )
    .unwrap();
    let report = run_dir.join("report/index.html");
    let finalized_report = std::fs::read(&report).unwrap();
    let export = root.path().join("report.html");
    assert!(
        binary()
            .args([
                "--project-root",
                project.to_str().unwrap(),
                "report",
                "latest",
                "--export",
                export.to_str().unwrap(),
            ])
            .status()
            .unwrap()
            .success()
    );
    assert!(export.is_file());
    assert_eq!(std::fs::read(&report).unwrap(), finalized_report);
    let share = root.path().join("share-report");
    assert!(
        binary()
            .args([
                "--project-root",
                project.to_str().unwrap(),
                "report",
                "latest",
                "--export-share",
                share.to_str().unwrap(),
            ])
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(std::fs::read(share.join("case-index.json")).unwrap(), b"[]");
    let share_html = std::fs::read_to_string(share.join("index.html")).unwrap();
    assert!(share_html.contains("Aggregate-only share derivative"));
    assert!(!share_html.contains("case-001"));
    assert!(!share_html.contains("case-search"));
    let case_chunk = run_dir.join("report/cases/00000.json");
    let original_chunk = std::fs::read(&case_chunk).unwrap();
    let mut tampered_chunk = original_chunk.clone();
    tampered_chunk.extend_from_slice(b" ");
    std::fs::write(&case_chunk, tampered_chunk).unwrap();
    let report_status = binary()
        .args([
            "--quiet",
            "--project-root",
            project.to_str().unwrap(),
            "report",
            "latest",
        ])
        .status()
        .unwrap();
    assert!(!report_status.success());
    std::fs::write(&case_chunk, original_chunk).unwrap();
    let unbound = run_dir.join("report/unbound.html");
    std::fs::write(&unbound, "untrusted").unwrap();
    let report_status = binary()
        .args([
            "--quiet",
            "--project-root",
            project.to_str().unwrap(),
            "report",
            "latest",
        ])
        .status()
        .unwrap();
    assert!(!report_status.success());
    std::fs::remove_file(unbound).unwrap();
    assert!(
        binary()
            .args([
                "--project-root",
                project.to_str().unwrap(),
                "replay",
                "latest",
            ])
            .status()
            .unwrap()
            .success()
    );
    let replay_verified_gate = binary()
        .args([
            "--quiet",
            "--project-root",
            project.to_str().unwrap(),
            "gate",
            "latest",
            "--verify",
            "replay",
        ])
        .status()
        .unwrap();
    assert_eq!(replay_verified_gate.code(), Some(12));
    let github_summary = root.path().join("github-step-summary.md");
    let github_gate = binary()
        .args([
            "--format",
            "github",
            "--project-root",
            project.to_str().unwrap(),
            "gate",
            "latest",
        ])
        .env("GITHUB_STEP_SUMMARY", &github_summary)
        .output()
        .unwrap();
    assert_eq!(github_gate.status.code(), Some(12));
    assert!(String::from_utf8_lossy(&github_gate.stdout).contains("::error title="));
    let github_summary = std::fs::read_to_string(github_summary).unwrap();
    assert!(github_summary.contains("## StructTrace regression check: INSUFFICIENT EVIDENCE"));
    assert!(github_summary.contains("THIS IS NOT RELEASE AUTHORIZATION"));
    assert!(!github_summary.contains("DEPLOYMENT AUTHORIZED"));
    assert!(github_summary.contains("Quality thresholds failed"));
    assert!(github_summary.contains("Evidence requirements are also insufficient"));
    assert!(github_summary.contains("| Metric | Baseline | Candidate |"));
    assert!(github_summary.contains("| Deployment success |"));
    let authorization = binary()
        .args([
            "--quiet",
            "--project-root",
            project.to_str().unwrap(),
            "gate",
            "latest",
            "--require-release-authorization",
        ])
        .status()
        .unwrap();
    assert_eq!(authorization.code(), Some(10));
    let release_check = binary()
        .args([
            "--quiet",
            "--project-root",
            project.to_str().unwrap(),
            "release-check",
            "latest",
        ])
        .status()
        .unwrap();
    assert_eq!(release_check.code(), Some(10));
    let json_gate = binary()
        .args([
            "--format",
            "json",
            "--project-root",
            project.to_str().unwrap(),
            "gate",
            "latest",
        ])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&json_gate.stdout).unwrap();
    assert_eq!(json["gate_mode"], "regression");
    assert_eq!(json["deployment_authorized"], false);

    let summary_path = run_dir.join("summary.json");
    let original_summary = std::fs::read(&summary_path).unwrap();
    let mut tampered_summary = original_summary.clone();
    tampered_summary.extend_from_slice(b"\n");
    std::fs::write(&summary_path, tampered_summary).unwrap();
    let gate_status = binary()
        .args([
            "--quiet",
            "--project-root",
            project.to_str().unwrap(),
            "gate",
            "latest",
        ])
        .status()
        .unwrap();
    assert!(!gate_status.success());
    std::fs::write(&summary_path, original_summary).unwrap();

    let mut html = std::fs::read(&report).unwrap();
    html.extend_from_slice(b"<!-- deliberate artifact tamper -->");
    std::fs::write(report, html).unwrap();
    let replay_status = binary()
        .args([
            "--quiet",
            "--project-root",
            project.to_str().unwrap(),
            "replay",
            "latest",
        ])
        .status()
        .unwrap();
    assert_eq!(replay_status.code(), Some(4));
}

#[test]
fn offline_demos_complete_without_credentials() {
    let root = tempdir().unwrap();
    for demo in ["support-ticket", "research"] {
        assert!(
            binary()
                .args([
                    "--project-root",
                    root.path().to_str().unwrap(),
                    "demo",
                    demo,
                ])
                .status()
                .unwrap()
                .success()
        );
    }
}

#[test]
fn release_check_zero_means_an_authorized_release() {
    let root = tempdir().unwrap();
    let project = root.path().join("release-project");
    assert!(
        binary()
            .args(["init", project.to_str().unwrap(), "--template", "recorded"])
            .status()
            .unwrap()
            .success()
    );
    let mut dataset = String::new();
    let mut outputs = String::new();
    for index in 0..100 {
        dataset.push_str(&format!(
            "{{\"id\":\"case-{index:03}\",\"input\":{{\"text\":\"unique-{index:03}\"}},\"expected\":{{\"label\":\"accepted\"}}}}\n"
        ));
        outputs.push_str(&format!(
            "{{\"case_id\":\"case-{index:03}\",\"status\":\"ok\",\"parsed_output\":{{\"label\":\"accepted\",\"reason\":\"deterministic\"}}}}\n"
        ));
    }
    std::fs::write(project.join("data/golden.jsonl"), dataset).unwrap();
    std::fs::write(project.join("outputs/baseline.jsonl"), &outputs).unwrap();
    std::fs::write(project.join("outputs/candidate.jsonl"), outputs).unwrap();
    let config = std::fs::read_to_string(project.join("structtrace.yaml")).unwrap();
    let config = config.split_once("gate:").unwrap().0.to_owned()
        + r#"gate:
  mode: release
  min_cases: 100
  min_unique_cases: 100
  max_duplicate_case_rate: 0.01
  min_primary_fully_evaluated_rate: 0.99
  max_primary_component_error_rate: 0.01
  max_primary_component_not_applicable_rate: 0.0
  max_primary_component_unscored_rate: 0.0
  max_deployment_regression_pp: 1.0
  min_candidate_deployment_success_rate: 0.95
  min_candidate_parse_validity: 0.99
  min_candidate_schema_validity: 0.99
  max_candidate_valid_but_wrong_rate: 0.02
"#;
    std::fs::write(project.join("structtrace.yaml"), config).unwrap();
    assert!(
        binary()
            .args([
                "--quiet",
                "--project-root",
                project.to_str().unwrap(),
                "run",
            ])
            .status()
            .unwrap()
            .success()
    );
    let release_check = binary()
        .args([
            "--quiet",
            "--project-root",
            project.to_str().unwrap(),
            "release-check",
            "latest",
        ])
        .status()
        .unwrap();
    assert_eq!(release_check.code(), Some(0));
}

#[test]
fn ordinary_jsonl_import_generates_a_runnable_strict_project() {
    let root = tempdir().unwrap();
    let source = root.path().join("source");
    let project = root.path().join("imported");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(
        source.join("dataset.jsonl"),
        "{\"document_id\":\"doc-1\",\"payload\":{\"text\":\"invoice alpha\"},\"ground_truth\":{\"vendor\":\"Acme\",\"total\":\"10.00\"}}\n{\"document_id\":\"doc-2\",\"payload\":{\"text\":\"invoice beta\"},\"ground_truth\":{\"vendor\":\"Beta\",\"total\":\"20.00\"}}\n",
    )
    .unwrap();
    std::fs::write(
        source.join("baseline.jsonl"),
        "{\"record_id\":\"doc-1\",\"result\":{\"vendor\":\" ACME \",\"total\":\"10.00\"}}\n{\"record_id\":\"doc-2\",\"result\":{\"vendor\":\"Beta\",\"total\":\"20.00\"}}\n",
    )
    .unwrap();
    std::fs::copy(
        source.join("baseline.jsonl"),
        source.join("candidate.jsonl"),
    )
    .unwrap();
    std::fs::write(
        source.join("schema.json"),
        r#"{"type":"object","required":["vendor","total"],"properties":{"vendor":{"type":"string"},"total":{"type":"string"}},"additionalProperties":false}"#,
    )
    .unwrap();
    let initialized = binary()
        .args([
            "init",
            project.to_str().unwrap(),
            "--from-outputs",
            "--dataset",
            source.join("dataset.jsonl").to_str().unwrap(),
            "--baseline",
            source.join("baseline.jsonl").to_str().unwrap(),
            "--candidate",
            source.join("candidate.jsonl").to_str().unwrap(),
            "--schema",
            source.join("schema.json").to_str().unwrap(),
            "--dataset-id-pointer",
            "/document_id",
            "--dataset-input-pointer",
            "/payload",
            "--dataset-expected-pointer",
            "/ground_truth",
            "--output-id-pointer",
            "/record_id",
            "--output-value-pointer",
            "/result",
            "--field-evaluator",
            "/vendor=normalized_string",
            "--field-evaluator",
            "/total=decimal_exact",
        ])
        .status()
        .unwrap();
    assert!(initialized.success());
    assert!(project.join("ONBOARDING.md").is_file());
    for command in [["doctor", "--strict"].as_slice(), ["run"].as_slice()] {
        assert!(
            binary()
                .arg("--quiet")
                .arg("--project-root")
                .arg(&project)
                .args(command)
                .status()
                .unwrap()
                .success()
        );
    }
    assert!(
        binary()
            .args([
                "--quiet",
                "--project-root",
                project.to_str().unwrap(),
                "replay",
                "latest",
            ])
            .status()
            .unwrap()
            .success()
    );
}

#[test]
fn demos_do_not_pollute_production_latest() {
    let root = tempdir().unwrap();
    let project = root.path().join("project");
    assert!(
        binary()
            .args(["init", project.to_str().unwrap(), "--template", "recorded"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        binary()
            .args([
                "--quiet",
                "--project-root",
                project.to_str().unwrap(),
                "run",
            ])
            .status()
            .unwrap()
            .success()
    );
    let manifests = || {
        std::fs::read_dir(project.join(".structtrace/runs"))
            .unwrap()
            .filter_map(Result::ok)
            .filter_map(|entry| std::fs::read(entry.path().join("manifest.json")).ok())
            .filter_map(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .collect::<Vec<_>>()
    };
    let production_id = manifests()
        .into_iter()
        .find(|manifest| manifest["run_kind"] == "production")
        .unwrap()["run_id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        binary()
            .args([
                "--quiet",
                "--project-root",
                project.to_str().unwrap(),
                "demo",
                "invoice",
            ])
            .status()
            .unwrap()
            .success()
    );
    let output = binary()
        .args([
            "--format",
            "json",
            "--project-root",
            project.to_str().unwrap(),
            "report",
            "latest",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains(&production_id));
    let demo_id = manifests()
        .into_iter()
        .find(|manifest| manifest["run_kind"] == "demo")
        .unwrap()["run_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let output = binary()
        .args([
            "--format",
            "json",
            "--project-root",
            project.to_str().unwrap(),
            "report",
            "latest-demo",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains(&demo_id));
}

#[test]
fn interrupted_run_resumes_without_reinvoking_completed_baseline() {
    let root = tempdir().unwrap();
    let project = root.path().join("resume-project");
    assert!(
        binary()
            .args(["init", project.to_str().unwrap(), "--template", "command"])
            .status()
            .unwrap()
            .success()
    );
    std::fs::write(
        project.join("variants/adapter.py"),
        r#"import argparse
import json
import pathlib
import sys
import time

parser = argparse.ArgumentParser()
parser.add_argument("--variant", required=True)
args = parser.parse_args()
root = pathlib.Path.cwd()
count_path = root / f"{args.variant}-count.txt"
count = int(count_path.read_text()) if count_path.exists() else 0
count_path.write_text(str(count + 1))
marker = root / "candidate-started"
if args.variant == "candidate" and not marker.exists():
    marker.write_text("started")
    time.sleep(10)
for line in sys.stdin:
    request = json.loads(line)
    text = request["input"]["text"]
    label = "rejected" if "negative" in text else "accepted"
    response = {"protocol":"structtrace.variant","protocol_version": 3,"case_id":request["case_id"],"status":"ok","output":{"label":label,"reason":"resume fixture"}}
    print(json.dumps(response), flush=True)
"#,
    )
    .unwrap();
    let mut interrupted = binary()
        .args(["--project-root", project.to_str().unwrap(), "run"])
        .spawn()
        .unwrap();
    let marker = project.join("candidate-started");
    for _ in 0..500 {
        if marker.is_file() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(
        marker.is_file(),
        "candidate did not begin within ten seconds"
    );
    interrupted.kill().unwrap();
    interrupted.wait().unwrap();
    let runs = project.join(".structtrace/runs");
    let run_dir = std::fs::read_dir(&runs)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.is_dir())
        .unwrap();
    let run_id = run_dir.file_name().unwrap().to_str().unwrap();
    assert!(run_dir.join("execution-checkpoint.json").is_file());
    assert!(
        binary()
            .args([
                "--project-root",
                project.to_str().unwrap(),
                "run",
                "--resume",
                run_id,
            ])
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(
        std::fs::read_to_string(project.join("baseline-count.txt")).unwrap(),
        "1"
    );
    assert!(!run_dir.join("execution-checkpoint.json").exists());
    assert!(run_dir.join("manifest.json").is_file());
    assert!(
        binary()
            .args([
                "--project-root",
                project.to_str().unwrap(),
                "replay",
                run_id,
            ])
            .status()
            .unwrap()
            .success()
    );
}

#[test]
fn run_management_archives_and_deletes_only_an_inactive_run() {
    let root = tempdir().unwrap();
    let project = root.path().join("project");
    assert!(
        binary()
            .args(["init", project.to_str().unwrap(), "--template", "recorded"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        binary()
            .args([
                "--quiet",
                "--project-root",
                project.to_str().unwrap(),
                "run",
            ])
            .status()
            .unwrap()
            .success()
    );
    let run_dir = std::fs::read_dir(project.join(".structtrace/runs"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let run_id = run_dir.file_name().unwrap().to_str().unwrap().to_owned();
    for args in [
        vec!["runs", "list"],
        vec!["runs", "show", run_id.as_str()],
        vec!["runs", "latest", "--kind", "production"],
    ] {
        assert!(
            binary()
                .args(["--quiet", "--project-root", project.to_str().unwrap()])
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    }
    let archive = root.path().join("archive");
    std::fs::write(run_dir.join("private-debug.txt"), "DO_NOT_ARCHIVE_SECRET").unwrap();
    assert!(
        binary()
            .args([
                "--quiet",
                "--project-root",
                project.to_str().unwrap(),
                "runs",
                "archive",
                "latest",
                archive.to_str().unwrap(),
            ])
            .status()
            .unwrap()
            .success()
    );
    assert!(archive.join("archive-verification.json").is_file());
    assert!(archive.join("run/manifest.json").is_file());
    assert!(!archive.join("run/private-debug.txt").exists());
    let receipt: serde_json::Value =
        serde_json::from_slice(&std::fs::read(archive.join("archive-verification.json")).unwrap())
            .unwrap();
    assert_eq!(receipt["run_id"], run_id);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&archive).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(archive.join("archive-verification.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    assert!(
        binary()
            .args([
                "--quiet",
                "--project-root",
                project.to_str().unwrap(),
                "runs",
                "delete",
                &run_id,
                "--yes",
            ])
            .status()
            .unwrap()
            .success()
    );
    assert!(!run_dir.exists());
}

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
    let status = binary()
        .args([
            "--format",
            "github",
            "--project-root",
            project.to_str().unwrap(),
            "gate",
            "latest",
        ])
        .env("GITHUB_STEP_SUMMARY", &github_summary)
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(12));
    let github_summary = std::fs::read_to_string(github_summary).unwrap();
    assert!(github_summary.contains("## StructTrace release gate: insufficient evidence"));
    assert!(github_summary.contains("| Metric | Baseline | Candidate |"));
    assert!(github_summary.contains("| Primary outcome |"));

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
    response = {"protocol":"structtrace.variant","protocol_version":1,"case_id":request["case_id"],"status":"ok","output":{"label":label,"reason":"resume fixture"}}
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

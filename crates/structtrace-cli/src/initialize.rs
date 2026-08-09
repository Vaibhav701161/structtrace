//! Fail-closed project initialization.

use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::InitTemplate;

const EXTRACTION_CONFIG: &str =
    include_str!("../../../examples/document-extraction/structtrace.yaml");
const EXTRACTION_SCHEMA: &str =
    include_str!("../../../examples/document-extraction/schemas/output.schema.json");
const EXTRACTION_DATASET: &str =
    include_str!("../../../examples/document-extraction/data/golden.jsonl");
const EXTRACTION_BASELINE: &str =
    include_str!("../../../examples/document-extraction/outputs/baseline.jsonl");
const EXTRACTION_CANDIDATE: &str =
    include_str!("../../../examples/document-extraction/outputs/candidate.jsonl");
const EXTRACTION_README: &str = include_str!("../../../examples/document-extraction/README.md");

/// Materialize one complete integration template.
pub fn initialize(destination: &Path, template: InitTemplate) -> anyhow::Result<PathBuf> {
    let mut protected = vec![
        "structtrace.yaml",
        "schemas/output.schema.json",
        "data/golden.jsonl",
        "README.md",
        ".gitignore",
    ];
    match template {
        InitTemplate::Recorded => {
            protected.extend(["outputs/baseline.jsonl", "outputs/candidate.jsonl"]);
        }
        InitTemplate::Python => protected.push("variants/app.py"),
        InitTemplate::Command => protected.push("variants/adapter.py"),
        InitTemplate::OpenaiCompatible => protected.push("variants/README.md"),
    }
    let conflicts = protected
        .iter()
        .filter(|relative| destination.join(relative).exists())
        .copied()
        .collect::<Vec<_>>();
    anyhow::ensure!(
        conflicts.is_empty(),
        "refusing to overwrite existing StructTrace files: {}",
        conflicts.join(", ")
    );
    for directory in [
        "schemas",
        "data",
        "evaluators",
        "variants",
        "outputs",
        ".structtrace",
    ] {
        std::fs::create_dir_all(destination.join(directory))?;
    }
    let project_name = destination
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("structured-output-project");
    write_new(
        &destination.join("structtrace.yaml"),
        &configuration(project_name, template),
    )?;
    write_new(&destination.join("schemas/output.schema.json"), SCHEMA)?;
    write_new(&destination.join("data/golden.jsonl"), DATASET)?;
    write_new(
        &destination.join("README.md"),
        &readme(project_name, template),
    )?;
    write_new(&destination.join(".gitignore"), ".structtrace/\n")?;
    match template {
        InitTemplate::Recorded => {
            write_new(&destination.join("outputs/baseline.jsonl"), BASELINE)?;
            write_new(&destination.join("outputs/candidate.jsonl"), CANDIDATE)?;
        }
        InitTemplate::Python => {
            write_new(&destination.join("variants/app.py"), PYTHON_VARIANTS)?;
        }
        InitTemplate::Command => {
            write_new(&destination.join("variants/adapter.py"), COMMAND_VARIANT)?;
        }
        InitTemplate::OpenaiCompatible => {
            write_new(&destination.join("variants/README.md"), OPENAI_NOTES)?;
        }
    }
    destination
        .canonicalize()
        .with_context(|| format!("could not resolve {}", destination.display()))
}

/// Materialize the production-shaped invoice extraction preset.
pub fn initialize_extraction(destination: &Path) -> anyhow::Result<PathBuf> {
    let files = [
        ("structtrace.yaml", EXTRACTION_CONFIG),
        ("schemas/output.schema.json", EXTRACTION_SCHEMA),
        ("data/golden.jsonl", EXTRACTION_DATASET),
        ("outputs/baseline.jsonl", EXTRACTION_BASELINE),
        ("outputs/candidate.jsonl", EXTRACTION_CANDIDATE),
        ("README.md", EXTRACTION_README),
        (".gitignore", ".structtrace/\n"),
    ];
    let conflicts = files
        .iter()
        .filter(|(relative, _)| destination.join(relative).exists())
        .map(|(relative, _)| *relative)
        .collect::<Vec<_>>();
    anyhow::ensure!(
        conflicts.is_empty(),
        "refusing to overwrite existing StructTrace files: {}",
        conflicts.join(", ")
    );
    for (relative, contents) in files {
        let path = destination.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        write_new(&path, contents)?;
    }
    destination
        .canonicalize()
        .with_context(|| format!("could not resolve {}", destination.display()))
}

fn write_new(path: &Path, contents: &str) -> anyhow::Result<()> {
    anyhow::ensure!(!path.exists(), "refusing to overwrite {}", path.display());
    std::fs::write(path, contents)
        .with_context(|| format!("could not create {}", path.display()))?;
    Ok(())
}

fn configuration(project_name: &str, template: InitTemplate) -> String {
    let project_name = serde_json::to_string(project_name).expect("project name is serializable");
    let python = if cfg!(windows) { "python" } else { "python3" };
    let variants = match template {
        InitTemplate::Recorded => r#"  baseline:
    kind: recorded
    path: outputs/baseline.jsonl
  candidate:
    kind: recorded
    path: outputs/candidate.jsonl"#
            .to_owned(),
        InitTemplate::Python => {
            format!(
                r#"  baseline:
    kind: python
    interpreter: {python}
    callable: variants.app:baseline
  candidate:
    kind: python
    interpreter: {python}
    callable: variants.app:candidate"#
            )
        }
        InitTemplate::Command => {
            format!(
                r#"  baseline:
    kind: command
    command:
      program: {python}
      args: [variants/adapter.py, --variant, baseline]
    process_mode: persistent
    timeout_ms: 60000
  candidate:
    kind: command
    command:
      program: {python}
      args: [variants/adapter.py, --variant, candidate]
    process_mode: persistent
    timeout_ms: 60000"#
            )
        }
        InitTemplate::OpenaiCompatible => r#"  baseline:
    kind: openai_compatible
    base_url: http://127.0.0.1:8000/v1
    api_key_env: LOCAL_LLM_API_KEY
    model: baseline-model
    request:
      system: Return only the required structured object.
      user_template: "{{ input.text }}"
      temperature: 0
      max_output_tokens: 200
    structured_output:
      mode: json_schema
      schema: schemas/output.schema.json
    timeout_ms: 120000
    concurrency: 4
  candidate:
    kind: openai_compatible
    base_url: http://127.0.0.1:8000/v1
    api_key_env: LOCAL_LLM_API_KEY
    model: candidate-model
    request:
      system: Return only the required structured object.
      user_template: "{{ input.text }}"
      temperature: 0
      max_output_tokens: 200
    structured_output:
      mode: json_schema
      schema: schemas/output.schema.json
    timeout_ms: 120000
    concurrency: 4"#
            .to_owned(),
    };
    format!(
        r#"version: 1

project:
  name: {project_name}
  description: Paired regression testing for a structured-output change

storage:
  root: .structtrace
  retain_raw_outputs: true
  retain_provider_responses: false

limits:
  max_output_bytes_per_case: 4194304
  max_stderr_bytes_per_process: 1048576
  max_report_raw_bytes_per_case: 262144

dataset:
  path: data/golden.jsonl
  format: jsonl

schema:
  path: schemas/output.schema.json

variants:
{variants}

evaluators:
  - id: exact_label
    kind: json_pointer_exact
    pointer: /label
    expected_pointer: /label

outcomes:
  semantic_correct:
    all_of: [exact_label]

analysis:
  primary_outcome: semantic_correct
  bootstrap:
    samples: 10000
    confidence: 0.95
    seed: 17

gate:
  # The generated two-case fixture is a demonstration, not release evidence.
  min_cases: 100
  min_primary_scored_rate: 0.99
  max_primary_evaluator_error_rate: 0.01
  max_primary_not_applicable_rate: 0.0
  max_primary_unscored_rate: 0.0
  max_primary_regression_pp: 1.0
  max_valid_but_wrong_increase_pp: 0.5
  min_candidate_schema_validity: 1.0
"#
    )
}

fn readme(project_name: &str, template: InitTemplate) -> String {
    format!(
        "# {project_name}\n\nStructTrace paired regression project using the `{}` integration.\n\n```bash\nstructtrace doctor\nstructtrace run\nstructtrace report latest --open\nstructtrace gate latest\n```\n",
        match template {
            InitTemplate::Recorded => "recorded-output",
            InitTemplate::Python => "Python-callable",
            InitTemplate::Command => "command",
            InitTemplate::OpenaiCompatible => "OpenAI-compatible",
        }
    )
}

const SCHEMA: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "required": ["label", "reason"],
  "properties": {
    "label": {"type": "string", "enum": ["accepted", "rejected"]},
    "reason": {"type": "string", "minLength": 1}
  },
  "additionalProperties": false
}
"#;

const DATASET: &str = r#"{"id":"case-001","input":{"text":"A clear positive example."},"expected":{"label":"accepted"},"metadata":{"split":"golden"}}
{"id":"case-002","input":{"text":"A clear negative example."},"expected":{"label":"rejected"},"metadata":{"split":"golden"}}
"#;

const BASELINE: &str = r#"{"case_id":"case-001","status":"ok","raw_output":"{\"label\":\"accepted\",\"reason\":\"Matched the positive rule.\"}"}
{"case_id":"case-002","status":"ok","raw_output":"{\"label\":\"rejected\",\"reason\":\"Matched the negative rule.\"}"}
"#;

const CANDIDATE: &str = r#"{"case_id":"case-001","status":"ok","raw_output":"{\"label\":\"accepted\",\"reason\":\"Matched the positive rule.\"}"}
{"case_id":"case-002","status":"ok","raw_output":"{\"label\":\"accepted\",\"reason\":\"Candidate regression.\"}"}
"#;

const PYTHON_VARIANTS: &str = r#"def baseline(case: dict) -> dict:
    text = case["input"]["text"]
    label = "rejected" if "negative" in text else "accepted"
    return {"label": label, "reason": "Baseline deterministic example."}


def candidate(case: dict) -> dict:
    return {"label": "accepted", "reason": "Candidate deterministic example."}
"#;

const COMMAND_VARIANT: &str = r#"import argparse
import json
import sys

parser = argparse.ArgumentParser()
parser.add_argument("--variant", choices=("baseline", "candidate"), required=True)
args = parser.parse_args()

for line in sys.stdin:
    request = json.loads(line)
    text = request["input"]["text"]
    if args.variant == "baseline":
        label = "rejected" if "negative" in text else "accepted"
    else:
        label = "accepted"
    response = {
        "protocol": "structtrace.variant",
        "protocol_version": 1,
        "case_id": request["case_id"],
        "status": "ok",
        "output": {"label": label, "reason": f"{args.variant} deterministic example."},
    }
    print(json.dumps(response), flush=True)
"#;

const OPENAI_NOTES: &str = r#"# OpenAI-compatible example

Set `LOCAL_LLM_API_KEY` and edit the endpoint and model names in
`structtrace.yaml`. StructTrace sends requests only when `structtrace run` is
explicitly invoked. `structtrace doctor` does not call the endpoint.
"#;

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn recorded_template_runs_end_to_end() {
        let root = tempdir().unwrap();
        let project = root.path().join("project");
        initialize(&project, InitTemplate::Recorded).unwrap();
        let run =
            structtrace_engine::run_recorded(&project, Path::new("structtrace.yaml")).unwrap();
        assert_eq!(run.summary.baseline.primary_pass, 2);
        assert_eq!(run.summary.candidate.primary_pass, 1);
        assert!(!run.summary.gate.status.is_passed());
    }

    #[test]
    fn extraction_preset_runs_with_production_evaluators() {
        let root = tempdir().unwrap();
        let project = root.path().join("invoice-project");
        initialize_extraction(&project).unwrap();
        let config =
            structtrace_core::config::Config::load(&project.join("structtrace.yaml")).unwrap();
        assert!(config.evaluators.iter().any(|evaluator| matches!(
            evaluator.kind,
            structtrace_core::config::EvaluatorKind::CanonicalDate { .. }
        )));
        assert!(config.evaluators.iter().any(|evaluator| matches!(
            evaluator.kind,
            structtrace_core::config::EvaluatorKind::FinancialInvariants { .. }
        )));
        let run =
            structtrace_engine::run_recorded(&project, Path::new("structtrace.yaml")).unwrap();
        assert_eq!(run.summary.baseline.total, 12);
        assert_eq!(run.summary.field_hotspots[0].pointer, "/total");
    }

    #[test]
    fn refuses_to_overwrite_an_existing_project() {
        let root = tempdir().unwrap();
        let project = root.path().join("project");
        initialize(&project, InitTemplate::Recorded).unwrap();
        assert!(initialize(&project, InitTemplate::Recorded).is_err());
    }

    #[tokio::test]
    async fn python_template_runs_end_to_end() {
        let root = tempdir().unwrap();
        let project = root.path().join("python-project");
        initialize(&project, InitTemplate::Python).unwrap();
        let run = structtrace_engine::run_configured(&project, Path::new("structtrace.yaml"))
            .await
            .unwrap();
        assert_eq!(run.summary.baseline.primary_pass, 2);
        assert_eq!(run.summary.candidate.primary_pass, 1);
        assert!(run.run_dir.join("report/index.html").is_file());
    }

    #[tokio::test]
    async fn command_template_runs_end_to_end() {
        let root = tempdir().unwrap();
        let project = root.path().join("command-project");
        initialize(&project, InitTemplate::Command).unwrap();
        let run = structtrace_engine::run_configured(&project, Path::new("structtrace.yaml"))
            .await
            .unwrap();
        assert_eq!(run.summary.baseline.primary_pass, 2);
        assert_eq!(run.summary.candidate.primary_pass, 1);
        assert_eq!(run.summary.baseline.operational.latency_observations, 2);
    }
}

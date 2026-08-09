//! Complete score and summary replay from retained run artifacts.

use std::{
    collections::BTreeMap,
    path::{Component, Path},
};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use structtrace_adapters::evaluator::{evaluator_definition, evaluator_request};
use structtrace_core::{
    ARTIFACT_FORMAT_VERSION,
    artifact::{ExternalEvaluatorReceipt, PairedCaseRecord, RunManifest, RunSummary},
    config::Config,
    dataset::Dataset,
    evaluation::{CaseEvaluation, EvaluatorResult, compile_schema, evaluate_case_with_external},
    hashing::{hash_canonical_json, hash_file},
    output::RecordedOutputs,
};

use crate::recorded::build_summary;

/// One stored-versus-recomputed case mismatch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScoreMismatch {
    /// Dataset case ID.
    pub case_id: String,
    /// Baseline or candidate.
    pub variant: String,
    /// Stored score object.
    pub stored: CaseEvaluation,
    /// Recomputed score object.
    pub replayed: CaseEvaluation,
}

/// Full artifact replay verdict.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplayReport {
    /// Run identity.
    pub run_id: String,
    /// Complete cases replayed.
    pub cases_replayed: usize,
    /// Variant outputs replayed.
    pub variant_outputs_replayed: usize,
    /// Built-in evaluator results independently recomputed.
    pub built_in_evaluator_results_recomputed: usize,
    /// Hash-bound external evaluator results verified without re-execution.
    pub external_evaluator_receipts_verified: usize,
    /// Side-effecting external evaluator programs re-executed. Always zero.
    pub external_evaluator_programs_reexecuted: usize,
    /// Missing or changed files bound by the manifest.
    pub artifact_hash_mismatches: Vec<String>,
    /// Disagreements between independently retained source and derived artifacts.
    pub cross_artifact_mismatches: Vec<String>,
    /// Stored case scores that did not reproduce.
    pub row_score_mismatches: Vec<ScoreMismatch>,
    /// Stored summary components that did not reproduce.
    pub summary_mismatches: Vec<String>,
    /// True only when every integrity and score check passed.
    pub verified: bool,
}

/// Verify hashes and recompute all scores, metrics, intervals, and gate rules.
pub fn replay_run(run_dir: &Path) -> anyhow::Result<ReplayReport> {
    let manifest: RunManifest = read_json(&run_dir.join("manifest.json"))?;
    anyhow::ensure!(
        manifest.artifact_format_version == ARTIFACT_FORMAT_VERSION,
        "artifact format {} is incompatible with supported version {}",
        manifest.artifact_format_version,
        ARTIFACT_FORMAT_VERSION
    );
    let mut artifact_hash_mismatches = Vec::new();
    for (relative, expected) in &manifest.artifacts {
        let path = safe_artifact_path(run_dir, relative)?;
        match hash_file(&path) {
            Ok(actual) if actual == *expected => {}
            Ok(actual) => artifact_hash_mismatches.push(format!(
                "{relative}: expected {expected}, observed {actual}"
            )),
            Err(error) => artifact_hash_mismatches.push(format!("{relative}: {error}")),
        }
    }
    for (relative, expected) in &manifest.input_artifacts {
        let path = safe_artifact_path(run_dir, relative)?;
        match hash_file(&path) {
            Ok(actual) if actual == *expected => {}
            Ok(actual) => artifact_hash_mismatches.push(format!(
                "input artifact {relative}: expected {expected}, observed {actual}"
            )),
            Err(error) => {
                artifact_hash_mismatches.push(format!("input artifact {relative}: {error}"))
            }
        }
    }

    let config: Config = read_json(&run_dir.join("inputs/configuration.json"))?;
    let config = Config::validate(config)?;
    let normalized_hash = hash_canonical_json(&config)?;
    if normalized_hash != manifest.normalized_configuration_hash {
        artifact_hash_mismatches.push(format!(
            "normalized configuration: expected {}, observed {}",
            manifest.normalized_configuration_hash, normalized_hash
        ));
    }
    let source_config_hash = hash_file(&run_dir.join("inputs/configuration.source"))?;
    if source_config_hash != manifest.configuration_file_hash {
        artifact_hash_mismatches.push(format!(
            "configuration source: expected {}, observed {}",
            manifest.configuration_file_hash, source_config_hash
        ));
    }
    let dataset = Dataset::read(
        &run_dir.join("inputs/dataset.jsonl"),
        &config.dataset.fields,
    )?;
    if dataset.source_hash != manifest.dataset_hash {
        artifact_hash_mismatches.push(format!(
            "dataset source: expected {}, observed {}",
            manifest.dataset_hash, dataset.source_hash
        ));
    }
    let schema_value: Value = read_json(&run_dir.join("inputs/schema.json"))?;
    let schema_hash = hash_file(&run_dir.join("inputs/schema.json"))?;
    if schema_hash != manifest.schema_hash {
        artifact_hash_mismatches.push(format!(
            "schema source: expected {}, observed {}",
            manifest.schema_hash, schema_hash
        ));
    }
    let schema = compile_schema(&schema_value)?;
    let stored_summary: RunSummary = read_json(&run_dir.join("summary.json"))?;
    let stored_records: Vec<PairedCaseRecord> = read_jsonl(&run_dir.join("cases.jsonl"))?;
    let baseline_outputs = RecordedOutputs::read(&run_dir.join("inputs/baseline.jsonl"), &dataset)?;
    let candidate_outputs =
        RecordedOutputs::read(&run_dir.join("inputs/candidate.jsonl"), &dataset)?;
    let external_count = config
        .evaluators
        .iter()
        .filter(|evaluator| is_external(&evaluator.kind))
        .count();
    let mut receipts = BTreeMap::new();
    if external_count > 0 {
        let stored_receipts: Vec<ExternalEvaluatorReceipt> =
            read_jsonl(&run_dir.join("external-evaluator-receipts.jsonl"))?;
        for receipt in stored_receipts {
            let key = (
                receipt.case_id.clone(),
                receipt.variant_id.clone(),
                receipt.evaluator_id.clone(),
            );
            if receipts.insert(key, receipt).is_some() {
                artifact_hash_mismatches
                    .push("external evaluator receipts contain a duplicate identity".to_owned());
            }
        }
    }
    let mut cross_artifact_mismatches = Vec::new();
    compare_manifest_to_config(&manifest, &config, &mut cross_artifact_mismatches)?;
    let mut stored_by_id = BTreeMap::new();
    for record in stored_records {
        if stored_by_id
            .insert(record.case.id.clone(), record)
            .is_some()
        {
            cross_artifact_mismatches.push("cases.jsonl contains a duplicate case ID".to_owned());
        }
    }
    let mut replayed_records = Vec::with_capacity(dataset.cases.len());
    let mut row_score_mismatches = Vec::new();
    let mut built_in_evaluator_results_recomputed = 0;
    let mut external_evaluator_receipts_verified = 0;
    for ((case, baseline_output), candidate_output) in dataset
        .cases
        .iter()
        .zip(&baseline_outputs.rows)
        .zip(&candidate_outputs.rows)
    {
        let Some(record) = stored_by_id.remove(&case.id) else {
            cross_artifact_mismatches.push(format!(
                "case {} exists in retained inputs but not cases.jsonl",
                case.id
            ));
            continue;
        };
        if &record.case != case {
            cross_artifact_mismatches.push(format!(
                "case {} differs between inputs/dataset.jsonl and cases.jsonl",
                case.id
            ));
        }
        if &record.baseline_output != baseline_output {
            cross_artifact_mismatches.push(format!(
                "case {} baseline differs between inputs/baseline.jsonl and cases.jsonl",
                case.id
            ));
        }
        if &record.candidate_output != candidate_output {
            cross_artifact_mismatches.push(format!(
                "case {} candidate differs between inputs/candidate.jsonl and cases.jsonl",
                case.id
            ));
        }
        let baseline_external = verified_external_results(
            &config,
            case,
            baseline_output,
            "baseline",
            &record.baseline_evaluation,
            &mut receipts,
            &mut cross_artifact_mismatches,
        );
        let candidate_external = verified_external_results(
            &config,
            case,
            candidate_output,
            "candidate",
            &record.candidate_evaluation,
            &mut receipts,
            &mut cross_artifact_mismatches,
        );
        external_evaluator_receipts_verified += baseline_external.len() + candidate_external.len();
        let baseline = evaluate_case_with_external(
            case,
            baseline_output,
            &schema,
            &config.evaluators,
            &config.outcomes,
            &config.analysis.primary_outcome,
            &baseline_external,
        );
        let candidate = evaluate_case_with_external(
            case,
            candidate_output,
            &schema,
            &config.evaluators,
            &config.outcomes,
            &config.analysis.primary_outcome,
            &candidate_external,
        );
        built_in_evaluator_results_recomputed +=
            baseline.evaluators.len() + candidate.evaluators.len() - (2 * external_count);
        if baseline != record.baseline_evaluation {
            row_score_mismatches.push(ScoreMismatch {
                case_id: case.id.clone(),
                variant: "baseline".to_owned(),
                stored: record.baseline_evaluation.clone(),
                replayed: baseline.clone(),
            });
        }
        if candidate != record.candidate_evaluation {
            row_score_mismatches.push(ScoreMismatch {
                case_id: case.id.clone(),
                variant: "candidate".to_owned(),
                stored: record.candidate_evaluation.clone(),
                replayed: candidate.clone(),
            });
        }
        replayed_records.push(PairedCaseRecord {
            case: case.clone(),
            baseline_output: baseline_output.clone(),
            candidate_output: candidate_output.clone(),
            transition: transition_name(baseline.primary_pass, candidate.primary_pass).to_owned(),
            baseline_evaluation: baseline,
            candidate_evaluation: candidate,
        });
    }
    for case_id in stored_by_id.keys() {
        cross_artifact_mismatches.push(format!(
            "case {case_id} exists in cases.jsonl but not retained dataset inputs"
        ));
    }
    for (case_id, variant_id, evaluator_id) in receipts.keys() {
        cross_artifact_mismatches.push(format!(
            "unused external evaluator receipt for case {case_id}, variant {variant_id}, evaluator {evaluator_id}"
        ));
    }
    let replayed_summary = build_summary(&manifest.run_id, &config, &replayed_records)?;
    let mut summary_mismatches = Vec::new();
    if replayed_summary != stored_summary {
        summary_mismatches.push("summary.json does not match recomputed case scores".to_owned());
    }
    let verified = artifact_hash_mismatches.is_empty()
        && cross_artifact_mismatches.is_empty()
        && row_score_mismatches.is_empty()
        && summary_mismatches.is_empty();
    Ok(ReplayReport {
        run_id: manifest.run_id,
        cases_replayed: replayed_records.len(),
        variant_outputs_replayed: replayed_records.len() * 2,
        built_in_evaluator_results_recomputed,
        external_evaluator_receipts_verified,
        external_evaluator_programs_reexecuted: 0,
        artifact_hash_mismatches,
        cross_artifact_mismatches,
        row_score_mismatches,
        summary_mismatches,
        verified,
    })
}

fn compare_manifest_to_config(
    manifest: &RunManifest,
    config: &Config,
    mismatches: &mut Vec<String>,
) -> anyhow::Result<()> {
    let expected_variants = serde_json::to_value(&config.variants)?;
    let expected_evaluation = serde_json::json!({
        "evaluators": config.evaluators,
        "outcomes": config.outcomes,
        "primary_outcome": config.analysis.primary_outcome,
    });
    let expected_gate = serde_json::to_value(&config.gate)?;
    let observed_gate = serde_json::to_value(&manifest.gate)?;
    let expected_bootstrap = serde_json::to_value(&config.analysis.bootstrap)?;
    let observed_bootstrap = serde_json::to_value(&manifest.bootstrap)?;
    let expected_schedule = RunManifest::new(String::new(), String::new()).execution_schedule;
    let checks = [
        (
            "manifest project_name differs from retained configuration",
            manifest.project_name == config.project.name,
        ),
        (
            "manifest variants differ from retained configuration",
            manifest.variants == expected_variants,
        ),
        (
            "manifest evaluation_definition differs from retained configuration",
            manifest.evaluation_definition == expected_evaluation,
        ),
        (
            "manifest gate differs from retained configuration",
            observed_gate == expected_gate,
        ),
        (
            "manifest bootstrap differs from retained configuration",
            observed_bootstrap == expected_bootstrap,
        ),
        (
            "manifest dataset_path differs from retained configuration",
            manifest.dataset_path == config.dataset.path.display().to_string(),
        ),
        (
            "manifest schema_path differs from retained configuration",
            manifest.schema_path == config.schema.path.display().to_string(),
        ),
        (
            "manifest execution_schedule is not the supported fixed schedule",
            manifest.execution_schedule == expected_schedule,
        ),
    ];
    mismatches.extend(
        checks
            .into_iter()
            .filter_map(|(message, matches)| (!matches).then_some(message.to_owned())),
    );
    Ok(())
}

fn is_external(kind: &structtrace_core::config::EvaluatorKind) -> bool {
    matches!(
        kind,
        structtrace_core::config::EvaluatorKind::Command { .. }
            | structtrace_core::config::EvaluatorKind::Python { .. }
    )
}

#[allow(clippy::too_many_arguments)]
fn verified_external_results(
    config: &Config,
    case: &structtrace_core::dataset::Case,
    output: &structtrace_core::output::VariantOutput,
    variant_id: &str,
    stored: &CaseEvaluation,
    receipts: &mut BTreeMap<(String, String, String), ExternalEvaluatorReceipt>,
    mismatches: &mut Vec<String>,
) -> std::collections::BTreeMap<String, EvaluatorResult> {
    config
        .evaluators
        .iter()
        .filter(|evaluator| is_external(&evaluator.kind))
        .filter_map(|evaluator| {
            let key = (case.id.clone(), variant_id.to_owned(), evaluator.id.clone());
            let Some(receipt) = receipts.remove(&key) else {
                mismatches.push(format!(
                    "missing external evaluator receipt for case {}, variant {variant_id}, evaluator {}",
                    case.id, evaluator.id
                ));
                return None;
            };
            let request = evaluator_request(&evaluator.id, case, output, variant_id);
            let definition = evaluator_definition(
                &evaluator.kind,
                evaluator.implementation_version.as_deref(),
            );
            let response = serde_json::to_value(&receipt.result).unwrap_or(Value::Null);
            let checks = [
                (
                    "request",
                    receipt.request_hash.as_str(),
                    hash_canonical_json(&request).unwrap_or_default(),
                ),
                (
                    "response",
                    receipt.response_hash.as_str(),
                    hash_canonical_json(&response).unwrap_or_default(),
                ),
                (
                    "definition",
                    receipt.definition_hash.as_str(),
                    hash_canonical_json(&definition).unwrap_or_default(),
                ),
            ];
            for (name, retained, recomputed) in checks {
                if retained != recomputed {
                    mismatches.push(format!(
                        "external evaluator {name} hash mismatch for case {}, variant {variant_id}, evaluator {}",
                        case.id, evaluator.id
                    ));
                }
            }
            if stored.evaluators.get(&evaluator.id) != Some(&receipt.result) {
                mismatches.push(format!(
                    "external evaluator receipt disagrees with cases.jsonl for case {}, variant {variant_id}, evaluator {}",
                    case.id, evaluator.id
                ));
            }
            Some((evaluator.id.clone(), receipt.result))
        })
        .collect()
}

fn safe_artifact_path(run_dir: &Path, relative: &str) -> anyhow::Result<std::path::PathBuf> {
    let relative = Path::new(relative);
    anyhow::ensure!(
        !relative.is_absolute()
            && relative
                .components()
                .all(|component| matches!(component, Component::Normal(_))),
        "manifest contains unsafe artifact path `{}`",
        relative.display()
    );
    let canonical_root = run_dir
        .canonicalize()
        .with_context(|| format!("could not canonicalize run directory {}", run_dir.display()))?;
    let mut current = canonical_root.clone();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            unreachable!()
        };
        current.push(component);
        let metadata = std::fs::symlink_metadata(&current)
            .with_context(|| format!("artifact {} is missing", current.display()))?;
        anyhow::ensure!(
            !metadata.file_type().is_symlink(),
            "manifest artifact path contains a symbolic link: {}",
            current.display()
        );
    }
    let canonical = current
        .canonicalize()
        .with_context(|| format!("could not canonicalize artifact {}", current.display()))?;
    anyhow::ensure!(
        canonical.starts_with(&canonical_root),
        "manifest artifact escaped the run directory: {}",
        canonical.display()
    );
    Ok(canonical)
}

fn transition_name(baseline: bool, candidate: bool) -> &'static str {
    match (baseline, candidate) {
        (true, true) => "both_pass",
        (true, false) => "baseline_only_pass",
        (false, true) => "candidate_only_pass",
        (false, false) => "both_fail",
    }
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    let bytes =
        std::fs::read(path).with_context(|| format!("could not read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("invalid JSON in {}", path.display()))
}

fn read_jsonl<T: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<Vec<T>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("could not read {}", path.display()))?;
    text.lines()
        .enumerate()
        .map(|(index, line)| {
            serde_json::from_str(line)
                .with_context(|| format!("invalid JSON at {}:{}", path.display(), index + 1))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tempfile::{TempDir, tempdir};

    use super::*;

    #[test]
    fn rejects_path_traversal_from_a_manifest() {
        assert!(safe_artifact_path(Path::new("run"), "../secret").is_err());
        assert!(safe_artifact_path(Path::new("run"), "/secret").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_manifest_artifacts() {
        use std::os::unix::fs::symlink;
        let run = tempdir().unwrap();
        let outside = tempdir().unwrap();
        std::fs::write(outside.path().join("secret"), "sensitive").unwrap();
        symlink(outside.path().join("secret"), run.path().join("artifact")).unwrap();
        let error = safe_artifact_path(run.path(), "artifact").unwrap_err();
        assert!(error.to_string().contains("symbolic link"));
    }

    #[test]
    fn rejects_newer_artifact_format_before_reading_run_contents() {
        let directory = tempdir().unwrap();
        let mut manifest = RunManifest::new("run-1".to_owned(), "compatibility".to_owned());
        manifest.artifact_format_version = ARTIFACT_FORMAT_VERSION + 1;
        std::fs::write(
            directory.path().join("manifest.json"),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        let error = replay_run(directory.path()).unwrap_err();
        assert!(error.to_string().contains("incompatible"));
    }

    fn write(path: &Path, contents: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    fn completed_run() -> (TempDir, std::path::PathBuf) {
        let root = tempdir().unwrap();
        write(
            &root.path().join("data.jsonl"),
            "{\"id\":\"a\",\"input\":{},\"expected\":{\"label\":\"yes\"}}\n",
        );
        write(&root.path().join("schema.json"), "{\"type\":\"object\"}");
        for variant in ["baseline", "candidate"] {
            write(
                &root.path().join(format!("{variant}.jsonl")),
                "{\"case_id\":\"a\",\"status\":\"ok\",\"raw_output\":\"{\\\"label\\\":\\\"yes\\\"}\"}\n",
            );
        }
        write(
            &root.path().join("structtrace.yaml"),
            r#"version: 1
project: {name: replay-adversarial}
dataset: {path: data.jsonl}
schema: {path: schema.json}
variants:
  baseline: {kind: recorded, path: baseline.jsonl}
  candidate: {kind: recorded, path: candidate.jsonl}
evaluators:
  - {id: exact, kind: exact_json}
outcomes:
  correct: {all_of: [exact]}
analysis:
  primary_outcome: correct
  bootstrap: {samples: 100, confidence: 0.95, seed: 17}
"#,
        );
        let run =
            crate::recorded::run_recorded(root.path(), Path::new("structtrace.yaml")).unwrap();
        (root, run.run_dir)
    }

    #[test]
    fn replay_detects_candidate_input_cases_disagreement_after_rehash() {
        let (_root, run_dir) = completed_run();
        let candidate = run_dir.join("inputs/candidate.jsonl");
        write(
            &candidate,
            "{\"case_id\":\"a\",\"status\":\"ok\",\"raw_output\":\"{\\\"label\\\":\\\"no\\\"}\"}\n",
        );
        let digest = hash_file(&candidate).unwrap();
        let manifest_path = run_dir.join("manifest.json");
        let mut manifest: RunManifest = read_json(&manifest_path).unwrap();
        manifest
            .artifacts
            .insert("inputs/candidate.jsonl".to_owned(), digest.clone());
        manifest
            .input_artifacts
            .insert("inputs/candidate.jsonl".to_owned(), digest);
        std::fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        let replay = replay_run(&run_dir).unwrap();
        assert!(!replay.verified);
        assert!(replay.cross_artifact_mismatches.iter().any(|item| {
            item.contains("candidate differs between inputs/candidate.jsonl and cases.jsonl")
        }));
    }

    #[test]
    fn replay_detects_dataset_cases_disagreement_after_rehash() {
        let (_root, run_dir) = completed_run();
        let dataset = run_dir.join("inputs/dataset.jsonl");
        write(
            &dataset,
            "{\"id\":\"a\",\"input\":{},\"expected\":{\"label\":\"no\"}}\n",
        );
        let digest = hash_file(&dataset).unwrap();
        let manifest_path = run_dir.join("manifest.json");
        let mut manifest: RunManifest = read_json(&manifest_path).unwrap();
        manifest.dataset_hash.clone_from(&digest);
        manifest
            .artifacts
            .insert("inputs/dataset.jsonl".to_owned(), digest.clone());
        manifest
            .input_artifacts
            .insert("inputs/dataset.jsonl".to_owned(), digest);
        std::fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        let replay = replay_run(&run_dir).unwrap();
        assert!(!replay.verified);
        assert!(
            replay.cross_artifact_mismatches.iter().any(|item| {
                item.contains("differs between inputs/dataset.jsonl and cases.jsonl")
            })
        );
    }

    #[test]
    fn replay_detects_tampered_summary_after_rehash() {
        let (_root, run_dir) = completed_run();
        let summary_path = run_dir.join("summary.json");
        let mut summary: RunSummary = read_json(&summary_path).unwrap();
        summary.candidate.primary_pass = 0;
        std::fs::write(&summary_path, serde_json::to_vec_pretty(&summary).unwrap()).unwrap();
        let digest = hash_file(&summary_path).unwrap();
        let manifest_path = run_dir.join("manifest.json");
        let mut manifest: RunManifest = read_json(&manifest_path).unwrap();
        manifest.artifacts.insert("summary.json".to_owned(), digest);
        std::fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        let replay = replay_run(&run_dir).unwrap();
        assert!(!replay.verified);
        assert!(!replay.summary_mismatches.is_empty());
    }

    #[test]
    fn replay_detects_manifest_description_tampering() {
        let (_root, run_dir) = completed_run();
        let manifest_path = run_dir.join("manifest.json");
        let mut manifest: RunManifest = read_json(&manifest_path).unwrap();
        manifest.project_name = "tampered-project".to_owned();
        manifest.dataset_path = "tampered-dataset.jsonl".to_owned();
        manifest.variants = serde_json::json!({"tampered": true});
        std::fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let replay = replay_run(&run_dir).unwrap();
        assert!(!replay.verified);
        assert!(
            replay
                .cross_artifact_mismatches
                .iter()
                .any(|item| item.contains("project_name"))
        );
        assert!(
            replay
                .cross_artifact_mismatches
                .iter()
                .any(|item| item.contains("dataset_path"))
        );
        assert!(
            replay
                .cross_artifact_mismatches
                .iter()
                .any(|item| item.contains("variants"))
        );
    }
}

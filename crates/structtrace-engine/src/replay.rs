//! Complete score and summary replay from retained run artifacts.

use std::path::{Component, Path};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use structtrace_core::{
    ARTIFACT_FORMAT_VERSION,
    artifact::{PairedCaseRecord, RunManifest, RunSummary},
    config::Config,
    evaluation::{CaseEvaluation, EvaluatorResult, compile_schema, evaluate_case_with_external},
    hashing::{hash_canonical_json, hash_file},
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
    /// Individual evaluator results recomputed.
    pub evaluator_results_recomputed: usize,
    /// Missing or changed files bound by the manifest.
    pub artifact_hash_mismatches: Vec<String>,
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

    let config: Config = read_json(&run_dir.join("inputs/configuration.json"))?;
    let config = Config::validate(config)?;
    let normalized_hash = hash_canonical_json(&config)?;
    if normalized_hash != manifest.normalized_configuration_hash {
        artifact_hash_mismatches.push(format!(
            "normalized configuration: expected {}, observed {}",
            manifest.normalized_configuration_hash, normalized_hash
        ));
    }
    let schema_value: Value = read_json(&run_dir.join("inputs/schema.json"))?;
    let schema = compile_schema(&schema_value)?;
    let stored_summary: RunSummary = read_json(&run_dir.join("summary.json"))?;
    let stored_records: Vec<PairedCaseRecord> = read_jsonl(&run_dir.join("cases.jsonl"))?;
    let mut replayed_records = Vec::with_capacity(stored_records.len());
    let mut row_score_mismatches = Vec::new();
    let mut evaluator_results_recomputed = 0;
    for record in stored_records {
        let baseline_external = retained_external_results(&config, &record.baseline_evaluation);
        let candidate_external = retained_external_results(&config, &record.candidate_evaluation);
        let baseline = evaluate_case_with_external(
            &record.case,
            &record.baseline_output,
            &schema,
            &config.evaluators,
            &config.outcomes,
            &config.analysis.primary_outcome,
            &baseline_external,
        );
        let candidate = evaluate_case_with_external(
            &record.case,
            &record.candidate_output,
            &schema,
            &config.evaluators,
            &config.outcomes,
            &config.analysis.primary_outcome,
            &candidate_external,
        );
        evaluator_results_recomputed += baseline.evaluators.len() + candidate.evaluators.len();
        if baseline != record.baseline_evaluation {
            row_score_mismatches.push(ScoreMismatch {
                case_id: record.case.id.clone(),
                variant: "baseline".to_owned(),
                stored: record.baseline_evaluation.clone(),
                replayed: baseline.clone(),
            });
        }
        if candidate != record.candidate_evaluation {
            row_score_mismatches.push(ScoreMismatch {
                case_id: record.case.id.clone(),
                variant: "candidate".to_owned(),
                stored: record.candidate_evaluation.clone(),
                replayed: candidate.clone(),
            });
        }
        replayed_records.push(PairedCaseRecord {
            transition: transition_name(baseline.primary_pass, candidate.primary_pass).to_owned(),
            baseline_evaluation: baseline,
            candidate_evaluation: candidate,
            ..record
        });
    }
    let replayed_summary = build_summary(&manifest.run_id, &config, &replayed_records)?;
    let mut summary_mismatches = Vec::new();
    if replayed_summary != stored_summary {
        summary_mismatches.push("summary.json does not match recomputed case scores".to_owned());
    }
    let verified = artifact_hash_mismatches.is_empty()
        && row_score_mismatches.is_empty()
        && summary_mismatches.is_empty();
    Ok(ReplayReport {
        run_id: manifest.run_id,
        cases_replayed: replayed_records.len(),
        variant_outputs_replayed: replayed_records.len() * 2,
        evaluator_results_recomputed,
        artifact_hash_mismatches,
        row_score_mismatches,
        summary_mismatches,
        verified,
    })
}

fn retained_external_results(
    config: &Config,
    stored: &CaseEvaluation,
) -> std::collections::BTreeMap<String, EvaluatorResult> {
    config
        .evaluators
        .iter()
        .filter(|evaluator| {
            matches!(
                evaluator.kind,
                structtrace_core::config::EvaluatorKind::Command { .. }
                    | structtrace_core::config::EvaluatorKind::Python { .. }
            )
        })
        .filter_map(|evaluator| {
            stored
                .evaluators
                .get(&evaluator.id)
                .cloned()
                .map(|result| (evaluator.id.clone(), result))
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
    Ok(run_dir.join(relative))
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
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn rejects_path_traversal_from_a_manifest() {
        assert!(safe_artifact_path(Path::new("run"), "../secret").is_err());
        assert!(safe_artifact_path(Path::new("run"), "/secret").is_err());
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
}

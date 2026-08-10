//! Compile-time bundled, network-free demonstrations.

use std::path::Path;

use anyhow::Context;
use serde::Deserialize;
use structtrace_core::artifact::RunKind;
use structtrace_core::config::{Config, VariantConfig};

const SUPPORT_CONFIG: &str = include_str!("../../../demo/support-ticket/structtrace.yaml");
const SUPPORT_DATASET: &str = include_str!("../../../demo/support-ticket/data/golden.jsonl");
const SUPPORT_SCHEMA: &str =
    include_str!("../../../demo/support-ticket/schemas/ticket.schema.json");
const SUPPORT_BASELINE: &str = include_str!("../../../demo/support-ticket/outputs/baseline.jsonl");
const SUPPORT_CANDIDATE: &str =
    include_str!("../../../demo/support-ticket/outputs/candidate.jsonl");
const INVOICE_CONFIG: &str = include_str!("../../../examples/document-extraction/structtrace.yaml");
const INVOICE_DATASET: &str =
    include_str!("../../../examples/document-extraction/data/golden.jsonl");
const INVOICE_SCHEMA: &str =
    include_str!("../../../examples/document-extraction/schemas/output.schema.json");
const INVOICE_BASELINE: &str =
    include_str!("../../../examples/document-extraction/outputs/baseline.jsonl");
const INVOICE_CANDIDATE: &str =
    include_str!("../../../examples/document-extraction/outputs/candidate.jsonl");
const RESEARCH_CONFIG: &str = include_str!("../../../demo/accepted-research/structtrace.yaml");
const RESEARCH_SCHEMA: &str = include_str!("../../../demo/accepted-research/schema.json");
const RESEARCH_COUNTS: &str = include_str!("../../../demo/accepted-research/expected-counts.json");

#[derive(Debug, Deserialize)]
struct ResearchStudy {
    id: String,
    label: String,
    both_pass: usize,
    baseline_only_pass: usize,
    candidate_only_pass: usize,
    both_fail: usize,
}

/// Separate, non-pooled normalized research runs plus their index.
pub struct ResearchDemo {
    pub runs: Vec<structtrace_engine::CompletedRun>,
    pub index_path: std::path::PathBuf,
}

/// Materialize and run the invoice extraction fixture below local state.
pub fn run_invoice(project_root: &Path) -> anyhow::Result<structtrace_engine::CompletedRun> {
    let root = project_root
        .canonicalize()
        .with_context(|| format!("project root {} does not exist", project_root.display()))?;
    let fixture_root = root.join(".structtrace/demo-inputs/invoice");
    write_fixture(&fixture_root.join("structtrace.yaml"), INVOICE_CONFIG)?;
    write_fixture(&fixture_root.join("data/golden.jsonl"), INVOICE_DATASET)?;
    write_fixture(
        &fixture_root.join("schemas/output.schema.json"),
        INVOICE_SCHEMA,
    )?;
    write_fixture(
        &fixture_root.join("outputs/baseline.jsonl"),
        INVOICE_BASELINE,
    )?;
    write_fixture(
        &fixture_root.join("outputs/candidate.jsonl"),
        INVOICE_CANDIDATE,
    )?;
    let config_path = fixture_root.join("structtrace.yaml");
    let mut config = Config::load(&config_path)?;
    config.storage.root = root.join(".structtrace");
    config.dataset.path = fixture_root.join("data/golden.jsonl");
    config.schema.path = fixture_root.join("schemas/output.schema.json");
    config.variants.insert(
        "baseline".to_owned(),
        VariantConfig::Recorded {
            path: fixture_root.join("outputs/baseline.jsonl"),
        },
    );
    config.variants.insert(
        "candidate".to_owned(),
        VariantConfig::Recorded {
            path: fixture_root.join("outputs/candidate.jsonl"),
        },
    );
    structtrace_engine::run_recorded_with_config_kind(&root, &config_path, config, RunKind::Demo)
}

/// Materialize and run the support-ticket fixture below local state.
pub fn run_support_ticket(project_root: &Path) -> anyhow::Result<structtrace_engine::CompletedRun> {
    let root = project_root
        .canonicalize()
        .with_context(|| format!("project root {} does not exist", project_root.display()))?;
    let fixture_root = root.join(".structtrace/demo-inputs/support-ticket");
    write_fixture(&fixture_root.join("structtrace.yaml"), SUPPORT_CONFIG)?;
    write_fixture(&fixture_root.join("data/golden.jsonl"), SUPPORT_DATASET)?;
    write_fixture(
        &fixture_root.join("schemas/ticket.schema.json"),
        SUPPORT_SCHEMA,
    )?;
    write_fixture(
        &fixture_root.join("outputs/baseline.jsonl"),
        SUPPORT_BASELINE,
    )?;
    write_fixture(
        &fixture_root.join("outputs/candidate.jsonl"),
        SUPPORT_CANDIDATE,
    )?;
    let config_path = fixture_root.join("structtrace.yaml");
    let mut config = Config::load(&config_path)?;
    config.storage.root = root.join(".structtrace");
    config.dataset.path = fixture_root.join("data/golden.jsonl");
    config.schema.path = fixture_root.join("schemas/ticket.schema.json");
    config.variants.insert(
        "baseline".to_owned(),
        VariantConfig::Recorded {
            path: fixture_root.join("outputs/baseline.jsonl"),
        },
    );
    config.variants.insert(
        "candidate".to_owned(),
        VariantConfig::Recorded {
            path: fixture_root.join("outputs/candidate.jsonl"),
        },
    );
    structtrace_engine::run_recorded_with_config_kind(&root, &config_path, config, RunKind::Demo)
}

/// Materialize and verify normalized accepted research outcome matrices.
pub fn run_research(project_root: &Path) -> anyhow::Result<ResearchDemo> {
    let root = project_root
        .canonicalize()
        .with_context(|| format!("project root {} does not exist", project_root.display()))?;
    let fixture_root = root.join(".structtrace/demo-inputs/accepted-research");
    let studies: Vec<ResearchStudy> = serde_json::from_str(RESEARCH_COUNTS)?;
    write_fixture(&fixture_root.join("expected-counts.json"), RESEARCH_COUNTS)?;
    let mut runs = Vec::new();
    for study in &studies {
        let study_root = fixture_root.join(&study.id);
        let (dataset, baseline, candidate) = research_jsonl(std::slice::from_ref(study))?;
        write_fixture(&study_root.join("structtrace.yaml"), RESEARCH_CONFIG)?;
        write_fixture(&study_root.join("schema.json"), RESEARCH_SCHEMA)?;
        write_fixture(&study_root.join("generated/dataset.jsonl"), &dataset)?;
        write_fixture(&study_root.join("generated/baseline.jsonl"), &baseline)?;
        write_fixture(&study_root.join("generated/candidate.jsonl"), &candidate)?;
        let config_path = study_root.join("structtrace.yaml");
        let mut config = Config::load(&config_path)?;
        config.project.name = format!("accepted-research-{}", study.id);
        config.project.description = Some(format!(
            "Normalized replay of the {} study only; no cross-study pooling",
            study.label
        ));
        config.storage.root = root.join(".structtrace");
        config.dataset.path = study_root.join("generated/dataset.jsonl");
        config.dataset.evidence_unit.pointer = Some("/metadata/accepted_case_ordinal".to_owned());
        config.gate = Default::default();
        config.schema.path = study_root.join("schema.json");
        config.variants.insert(
            "baseline".to_owned(),
            VariantConfig::Recorded {
                path: study_root.join("generated/baseline.jsonl"),
            },
        );
        config.variants.insert(
            "candidate".to_owned(),
            VariantConfig::Recorded {
                path: study_root.join("generated/candidate.jsonl"),
            },
        );
        runs.push(structtrace_engine::run_recorded_with_config_kind(
            &root,
            &config_path,
            config,
            RunKind::ResearchFixture,
        )?);
    }
    let index_path = fixture_root.join("index.html");
    let mut index = String::from(
        "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>StructTrace research fixtures</title><style>body{font:16px/1.6 system-ui;max-width:780px;margin:4rem auto;padding:0 1.5rem;color:#172033}li{margin:.8rem 0}code{font-size:.9em}</style><h1>Accepted research fixtures</h1><p><strong>These studies are separate. No pooled effect or release gate is calculated.</strong></p><ul>",
    );
    for (study, run) in studies.iter().zip(&runs) {
        index.push_str(&format!(
            "<li>{}: <a href=\"{}\">open separate report</a></li>",
            study.label,
            run.run_dir.join("report/index.html").display()
        ));
    }
    index.push_str("</ul></html>");
    write_fixture(&index_path, &index)?;
    Ok(ResearchDemo { runs, index_path })
}

fn research_jsonl(studies: &[ResearchStudy]) -> anyhow::Result<(String, String, String)> {
    let mut dataset = String::new();
    let mut baseline = String::new();
    let mut candidate = String::new();
    for study in studies {
        let categories = [
            ("both_pass", study.both_pass, true, true),
            ("baseline_only_pass", study.baseline_only_pass, true, false),
            (
                "candidate_only_pass",
                study.candidate_only_pass,
                false,
                true,
            ),
            ("both_fail", study.both_fail, false, false),
        ];
        let mut ordinal = 0;
        for (transition, count, baseline_pass, candidate_pass) in categories {
            for _ in 0..count {
                ordinal += 1;
                let case_id = format!("{}-{ordinal:03}", study.id);
                append_json_line(
                    &mut dataset,
                    &serde_json::json!({
                        "id": case_id,
                        "input": {"study": study.label},
                        "expected": {"answer": "correct"},
                        "metadata": {
                            "study": study.id,
                            "study_label": study.label,
                            "accepted_transition": transition,
                            "fixture": "normalized paired outcome",
                            "accepted_case_ordinal": ordinal
                        }
                    }),
                )?;
                append_json_line(&mut baseline, &recorded_outcome(&case_id, baseline_pass))?;
                append_json_line(&mut candidate, &recorded_outcome(&case_id, candidate_pass))?;
            }
        }
    }
    Ok((dataset, baseline, candidate))
}

fn recorded_outcome(case_id: &str, passed: bool) -> serde_json::Value {
    let answer = if passed { "correct" } else { "wrong" };
    serde_json::json!({
        "case_id": case_id,
        "status": "ok",
        "raw_output": serde_json::json!({"answer": answer}).to_string(),
        "metadata": {"fixture": "normalized accepted outcome"}
    })
}

fn append_json_line(buffer: &mut String, value: &serde_json::Value) -> anyhow::Result<()> {
    buffer.push_str(&serde_json::to_string(value)?);
    buffer.push('\n');
    Ok(())
}

fn write_fixture(path: &Path, contents: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if path.is_file() && std::fs::read_to_string(path).ok().as_deref() == Some(contents) {
        return Ok(());
    }
    std::fs::write(path, contents)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use structtrace_core::{
        artifact::PairedCaseRecord,
        statistics::{PairedMetrics, paired_metrics},
    };
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn support_demo_exposes_structural_improvement_and_semantic_regression() {
        let root = tempdir().unwrap();
        let run = run_support_ticket(root.path()).unwrap();
        assert_eq!(run.summary.baseline.schema_valid, 11);
        assert_eq!(run.summary.candidate.schema_valid, 12);
        assert_eq!(run.summary.baseline.primary_pass, 10);
        assert_eq!(run.summary.candidate.primary_pass, 8);
        assert_eq!(run.summary.paired.baseline_only_pass, 4);
        assert_eq!(run.summary.paired.candidate_only_pass, 2);
        assert!(!run.summary.gate.status.is_passed());
    }

    #[test]
    fn invoice_demo_exposes_exact_field_level_regressions() {
        let root = tempdir().unwrap();
        let run = run_invoice(root.path()).unwrap();
        assert_eq!(run.summary.baseline.total, 12);
        assert_eq!(run.summary.baseline.primary_pass, 9);
        assert_eq!(run.summary.candidate.primary_pass, 9);
        assert_eq!(run.summary.paired.baseline_only_pass, 3);
        assert_eq!(run.summary.paired.candidate_only_pass, 3);
        assert_eq!(
            run.summary.gate.status,
            structtrace_core::gate::GateStatus::InsufficientEvidence
        );
        assert!(
            run.summary
                .field_hotspots
                .iter()
                .any(|hotspot| hotspot.pointer == "/total" && hotspot.regressions == 2)
        );
    }

    #[test]
    fn research_demo_reproduces_all_accepted_paired_counts() {
        let root = tempdir().unwrap();
        let mut groups: BTreeMap<String, Vec<(bool, bool)>> = BTreeMap::new();
        let research = run_research(root.path()).unwrap();
        assert_eq!(research.runs.len(), 3);
        assert!(research.index_path.is_file());
        for run in &research.runs {
            assert_eq!(
                run.summary.gate.status,
                structtrace_core::gate::GateStatus::NotConfigured
            );
            let text = std::fs::read_to_string(run.run_dir.join("cases.jsonl")).unwrap();
            for line in text.lines() {
                let record: PairedCaseRecord = serde_json::from_str(line).unwrap();
                let study = record
                    .case
                    .metadata
                    .as_ref()
                    .and_then(|value| value.pointer("/study"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap()
                    .to_owned();
                groups.entry(study).or_default().push((
                    record.baseline_evaluation.primary_pass,
                    record.candidate_evaluation.primary_pass,
                ));
            }
            assert!(
                structtrace_engine::replay_run(&run.run_dir)
                    .unwrap()
                    .verified
            );
        }
        let actual = groups
            .into_iter()
            .map(|(name, pairs)| (name, paired_metrics(&pairs)))
            .collect::<BTreeMap<_, _>>();
        assert_counts(&actual["corrected_qwen"], 49, 18, 24, 9, 3);
        assert_counts(&actual["canonical_llama"], 150, 92, 82, 6, 16);
        assert_counts(&actual["tool_call_pilot"], 30, 26, 24, 1, 3);
    }

    fn assert_counts(
        metrics: &PairedMetrics,
        total: usize,
        baseline: usize,
        candidate: usize,
        candidate_only: usize,
        baseline_only: usize,
    ) {
        assert_eq!(metrics.total, total);
        assert_eq!(metrics.baseline_pass, baseline);
        assert_eq!(metrics.candidate_pass, candidate);
        assert_eq!(metrics.candidate_only_pass, candidate_only);
        assert_eq!(metrics.baseline_only_pass, baseline_only);
    }
}

//! End-to-end recorded-output comparison.

use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::Context;
use rust_decimal::{Decimal, prelude::FromPrimitive};
use serde::Serialize;
use serde_json::Value;
use structtrace_adapters::{
    command::CommandLimits,
    evaluator::{EVALUATOR_BRIDGE_SOURCE, EvaluatorRuntime, run_external_evaluator},
};
use structtrace_core::{
    ARTIFACT_FORMAT_VERSION,
    artifact::{
        EvaluatorComparison, ExternalEvaluatorReceipt, FieldHotspot, MatchedOperationalSummary,
        PairedCaseRecord, RunManifest, RunStatus, RunSummary, VariantSummary,
    },
    config::{Config, EvaluatorKind, VariantConfig},
    dataset::Dataset,
    evaluation::{EvaluationStatus, EvaluatorResult, compile_schema, evaluate_case_with_external},
    gate::{GateInputs, evaluate_gate},
    hashing::{hash_bytes, hash_canonical_json, hash_file},
    output::{OutputStatus, RecordedOutputs},
    statistics::{paired_bootstrap, paired_metrics},
};
use tempfile::NamedTempFile;
use ulid::Ulid;

use crate::storage::RunStore;

/// Finalized local run returned to the CLI and report layer.
#[derive(Debug, Clone)]
pub struct CompletedRun {
    /// ULID run identity.
    pub run_id: String,
    /// Artifact directory.
    pub run_dir: PathBuf,
    /// Portable summary.
    pub summary: RunSummary,
    /// Final manifest.
    pub manifest: RunManifest,
}

/// A complete-denominator variant result prepared by any execution adapter.
#[derive(Debug, Clone)]
pub struct PreparedVariant {
    /// Provenance label recorded in the manifest.
    pub source_label: String,
    /// Exact imported/generated input hash before retention policy is applied.
    pub input_hash: String,
    /// Exact bytes retained as the portable variant output artifact.
    pub source_bytes: Vec<u8>,
    /// One output per dataset case, in dataset order.
    pub rows: Vec<structtrace_core::output::VariantOutput>,
    /// Capped standard error retained separately from protocol output.
    pub stderr: Vec<u8>,
    /// Adapter-level protocol diagnostics.
    pub protocol_errors: Vec<String>,
}

/// Run the complete recorded-output workflow from a validated configuration.
pub fn run_recorded(project_root: &Path, config_path: &Path) -> anyhow::Result<CompletedRun> {
    let project_root = project_root
        .canonicalize()
        .with_context(|| format!("project root {} does not exist", project_root.display()))?;
    let config_path = resolve(&project_root, config_path);
    let config_source_bytes = std::fs::read(&config_path)
        .with_context(|| format!("could not read configuration {}", config_path.display()))?;
    let config = Config::from_bytes(&config_path, &config_source_bytes)?;
    run_recorded_with_snapshot(&project_root, config, config_source_bytes)
}

/// Run a comparison with validated CLI overrides while retaining the source config hash.
pub fn run_recorded_with_config(
    project_root: &Path,
    config_path: &Path,
    config: Config,
) -> anyhow::Result<CompletedRun> {
    let project_root = project_root
        .canonicalize()
        .with_context(|| format!("project root {} does not exist", project_root.display()))?;
    let config_path = resolve(&project_root, config_path);
    let config_source_bytes = std::fs::read(&config_path)
        .with_context(|| format!("could not read configuration {}", config_path.display()))?;
    run_recorded_with_snapshot(&project_root, config, config_source_bytes)
}

fn run_recorded_with_snapshot(
    project_root: &Path,
    config: Config,
    config_source_bytes: Vec<u8>,
) -> anyhow::Result<CompletedRun> {
    let config = Config::validate(config)?;
    let dataset_path = resolve(project_root, &config.dataset.path);
    let dataset = Dataset::read(&dataset_path, &config.dataset.fields)?;
    let schema_path = resolve(project_root, &config.schema.path);
    let schema_bytes = std::fs::read(&schema_path)
        .with_context(|| format!("could not read schema {}", schema_path.display()))?;
    let schema_value: Value = serde_json::from_slice(&schema_bytes)
        .with_context(|| format!("schema {} is not valid JSON", schema_path.display()))?;
    compile_schema(&schema_value)?;

    let baseline = read_recorded_variant(
        project_root,
        "baseline",
        config.variants.get("baseline").expect("validated config"),
        &dataset,
    )?;
    let candidate = read_recorded_variant(
        project_root,
        "candidate",
        config.variants.get("candidate").expect("validated config"),
        &dataset,
    )?;

    finalize_prepared(
        project_root,
        config_source_bytes,
        config,
        dataset,
        schema_bytes,
        baseline,
        candidate,
    )
}

/// Evaluate, persist, report, and hash-bind outputs from any pair of adapters.
#[allow(clippy::too_many_arguments)]
pub fn finalize_prepared(
    project_root: &Path,
    config_source_bytes: Vec<u8>,
    config: Config,
    dataset: Dataset,
    schema_bytes: Vec<u8>,
    mut baseline: PreparedVariant,
    mut candidate: PreparedVariant,
) -> anyhow::Result<CompletedRun> {
    apply_storage_retention(
        &mut baseline,
        config.storage.retain_raw_outputs,
        config.storage.retain_provider_responses,
        config.report.include_prompts,
    )?;
    apply_storage_retention(
        &mut candidate,
        config.storage.retain_raw_outputs,
        config.storage.retain_provider_responses,
        config.report.include_prompts,
    )?;
    finalize_prepared_for_run(
        project_root,
        config_source_bytes,
        config,
        dataset,
        schema_bytes,
        baseline,
        candidate,
        None,
    )
}

/// Finalize adapter outputs into a previously allocated resumable run.
#[allow(clippy::too_many_arguments)]
pub fn finalize_prepared_for_run(
    project_root: &Path,
    config_source_bytes: Vec<u8>,
    config: Config,
    dataset: Dataset,
    schema_bytes: Vec<u8>,
    baseline: PreparedVariant,
    candidate: PreparedVariant,
    existing_run_id: Option<String>,
) -> anyhow::Result<CompletedRun> {
    if baseline.rows.len() != dataset.cases.len() || candidate.rows.len() != dataset.cases.len() {
        anyhow::bail!("prepared variants must contain exactly one row per dataset case");
    }
    for (name, prepared) in [("baseline", &baseline), ("candidate", &candidate)] {
        for (case, output) in dataset.cases.iter().zip(&prepared.rows) {
            if case.id != output.case_id {
                anyhow::bail!(
                    "prepared {name} output order mismatch: expected `{}`, received `{}`",
                    case.id,
                    output.case_id
                );
            }
        }
    }
    let schema_value: Value =
        serde_json::from_slice(&schema_bytes).context("retained schema is not valid JSON")?;
    let schema = compile_schema(&schema_value)?;
    let storage_root = resolve(project_root, &config.storage.root);
    let (run_id, store) = if let Some(run_id) = existing_run_id {
        let store = RunStore::open(&storage_root.join("runs").join(&run_id))?;
        store.reset_for_resume(&run_id)?;
        (run_id, store)
    } else {
        let run_id = Ulid::new().to_string();
        let store = RunStore::create(&storage_root, &run_id)?;
        (run_id, store)
    };
    let mut failure_guard = store.failure_guard(&run_id);
    store.set_status(&run_id, RunStatus::Validating)?;

    store.record_event(
        "inputs_validated",
        &serde_json::json!({"cases": dataset.cases.len()}),
    )?;
    for (ordinal, case) in dataset.cases.iter().enumerate() {
        store.insert_case(ordinal, case)?;
    }
    store.insert_variant(
        "baseline",
        config.variants.get("baseline").expect("validated config"),
    )?;
    store.insert_variant(
        "candidate",
        config.variants.get("candidate").expect("validated config"),
    )?;

    store.set_status(&run_id, RunStatus::Running)?;
    let mut records = Vec::with_capacity(dataset.cases.len());
    let evaluator_bridge = materialize_evaluator_bridge(&storage_root)?;
    let mut evaluator_stderr = Vec::new();
    let mut evaluator_receipts = Vec::new();
    for index in 0..dataset.cases.len() {
        let case = &dataset.cases[index];
        let baseline_output = &baseline.rows[index];
        let candidate_output = &candidate.rows[index];
        let baseline_external = execute_external_evaluators(
            &config,
            case,
            baseline_output,
            "baseline",
            project_root,
            &evaluator_bridge,
            &mut evaluator_stderr,
            &mut evaluator_receipts,
        );
        let candidate_external = execute_external_evaluators(
            &config,
            case,
            candidate_output,
            "candidate",
            project_root,
            &evaluator_bridge,
            &mut evaluator_stderr,
            &mut evaluator_receipts,
        );
        let baseline_evaluation = evaluate_case_with_external(
            case,
            baseline_output,
            &schema,
            &config.evaluators,
            &config.outcomes,
            &config.analysis.primary_outcome,
            &baseline_external,
        );
        let candidate_evaluation = evaluate_case_with_external(
            case,
            candidate_output,
            &schema,
            &config.evaluators,
            &config.outcomes,
            &config.analysis.primary_outcome,
            &candidate_external,
        );
        store.insert_output("baseline", baseline_output)?;
        store.insert_output("candidate", candidate_output)?;
        store.insert_evaluation("baseline", &baseline_evaluation)?;
        store.insert_evaluation("candidate", &candidate_evaluation)?;
        records.push(PairedCaseRecord {
            case: case.clone(),
            baseline_output: baseline_output.clone(),
            candidate_output: candidate_output.clone(),
            transition: transition_name(
                baseline_evaluation.primary_pass,
                candidate_evaluation.primary_pass,
            )
            .to_owned(),
            baseline_evaluation,
            candidate_evaluation,
        });
    }

    store.set_status(&run_id, RunStatus::Analyzing)?;
    let summary = build_summary(&run_id, &config, &records)?;
    store.insert_paired_result(&config.analysis.primary_outcome, &summary.paired)?;

    let run_dir = store.run_dir().to_owned();
    atomic_write_json(&run_dir.join("inputs/configuration.json"), &config)?;
    atomic_write(
        &run_dir.join("inputs/configuration.source"),
        &config_source_bytes,
    )?;
    atomic_write(&run_dir.join("inputs/dataset.jsonl"), &dataset.source_bytes)?;
    atomic_write(&run_dir.join("inputs/schema.json"), &schema_bytes)?;
    atomic_write(
        &run_dir.join("inputs/baseline.jsonl"),
        &baseline.source_bytes,
    )?;
    atomic_write(
        &run_dir.join("inputs/candidate.jsonl"),
        &candidate.source_bytes,
    )?;
    if !baseline.stderr.is_empty() {
        atomic_write(&run_dir.join("logs/baseline.stderr.log"), &baseline.stderr)?;
    }
    if !candidate.stderr.is_empty() {
        atomic_write(
            &run_dir.join("logs/candidate.stderr.log"),
            &candidate.stderr,
        )?;
    }
    if !baseline.protocol_errors.is_empty() {
        atomic_write_json(
            &run_dir.join("logs/baseline.protocol-errors.json"),
            &baseline.protocol_errors,
        )?;
    }
    if !candidate.protocol_errors.is_empty() {
        atomic_write_json(
            &run_dir.join("logs/candidate.protocol-errors.json"),
            &candidate.protocol_errors,
        )?;
    }
    if !evaluator_stderr.is_empty() {
        atomic_write(
            &run_dir.join("logs/evaluators.stderr.log"),
            &evaluator_stderr,
        )?;
    }
    if !evaluator_receipts.is_empty() {
        atomic_write_jsonl(
            &run_dir.join("external-evaluator-receipts.jsonl"),
            &evaluator_receipts,
        )?;
    }
    atomic_write_jsonl(&run_dir.join("cases.jsonl"), &records)?;
    let discordances = records
        .iter()
        .filter(|record| {
            record.baseline_evaluation.primary_pass != record.candidate_evaluation.primary_pass
                || record.baseline_evaluation.valid_but_wrong
                || record.candidate_evaluation.valid_but_wrong
        })
        .collect::<Vec<_>>();
    atomic_write_jsonl(&run_dir.join("discordances.jsonl"), &discordances)?;
    atomic_write_json(&run_dir.join("summary.json"), &summary)?;
    atomic_write(
        &run_dir.join("summary.md"),
        summary_markdown(&config.project.name, &summary).as_bytes(),
    )?;

    let mut manifest = RunManifest::new(run_id.clone(), config.project.name.clone());
    manifest.configuration_file_hash = hash_bytes(&config_source_bytes);
    manifest.normalized_configuration_hash = hash_canonical_json(&config)?;
    manifest.dataset_path = config.dataset.path.display().to_string();
    manifest.dataset_hash = dataset.source_hash.clone();
    manifest.schema_path = config.schema.path.display().to_string();
    manifest.schema_hash = hash_bytes(&schema_bytes);
    manifest.variants = serde_json::to_value(&config.variants)?;
    manifest.evaluation_definition = serde_json::json!({
        "evaluators": config.evaluators,
        "outcomes": config.outcomes,
        "primary_outcome": config.analysis.primary_outcome,
    });
    manifest.gate = config.gate.clone();
    manifest.bootstrap = config.analysis.bootstrap.clone();
    manifest.input_artifacts = BTreeMap::from([
        (
            "inputs/baseline.jsonl".to_owned(),
            hash_bytes(&baseline.source_bytes),
        ),
        (
            "inputs/candidate.jsonl".to_owned(),
            hash_bytes(&candidate.source_bytes),
        ),
    ]);
    for variant in config.variants.values() {
        if let VariantConfig::OpenaiCompatible(adapter) = variant {
            if let Some(name) = &adapter.api_key_env {
                manifest
                    .environment
                    .insert(name.clone(), std::env::var_os(name).is_some());
            }
        }
    }
    for relative in [
        "inputs/configuration.json",
        "inputs/configuration.source",
        "inputs/dataset.jsonl",
        "inputs/schema.json",
        "inputs/baseline.jsonl",
        "inputs/candidate.jsonl",
        "cases.jsonl",
        "discordances.jsonl",
        "summary.json",
        "summary.md",
    ] {
        let path = run_dir.join(relative);
        let digest = hash_file(&path)?;
        let length = std::fs::metadata(&path)?.len();
        manifest
            .artifacts
            .insert(relative.to_owned(), digest.clone());
        store.record_artifact(relative, &digest, length)?;
    }
    if run_dir.join("external-evaluator-receipts.jsonl").is_file() {
        let relative = "external-evaluator-receipts.jsonl";
        let path = run_dir.join(relative);
        let digest = hash_file(&path)?;
        let length = std::fs::metadata(&path)?.len();
        manifest
            .artifacts
            .insert(relative.to_owned(), digest.clone());
        store.record_artifact(relative, &digest, length)?;
    }
    for relative in [
        "logs/baseline.stderr.log",
        "logs/candidate.stderr.log",
        "logs/baseline.protocol-errors.json",
        "logs/candidate.protocol-errors.json",
        "logs/evaluators.stderr.log",
    ] {
        let path = run_dir.join(relative);
        if path.is_file() {
            let digest = hash_file(&path)?;
            let length = std::fs::metadata(&path)?.len();
            manifest
                .artifacts
                .insert(relative.to_owned(), digest.clone());
            store.record_artifact(relative, &digest, length)?;
        }
    }
    manifest.completed_at_unix_ms = Some(unix_millis());
    manifest.status = RunStatus::Analyzing;
    atomic_write_json(&run_dir.join("manifest.json"), &manifest)?;
    let report = structtrace_report::generate(&run_dir)?;
    let report_relative = "report/index.html";
    let report_digest = hash_file(&report.index_path)?;
    let report_length = std::fs::metadata(&report.index_path)?.len();
    manifest
        .artifacts
        .insert(report_relative.to_owned(), report_digest.clone());
    store.record_artifact(report_relative, &report_digest, report_length)?;
    store.set_status(&run_id, RunStatus::Complete)?;
    store.checkpoint()?;
    manifest.artifacts.insert(
        "run.sqlite3".to_owned(),
        hash_file(&run_dir.join("run.sqlite3"))?,
    );
    manifest.status = RunStatus::Complete;
    atomic_write_json(&run_dir.join("manifest.json"), &manifest)?;
    failure_guard.disarm();
    drop(failure_guard);

    Ok(CompletedRun {
        run_id,
        run_dir,
        summary,
        manifest,
    })
}

#[allow(clippy::too_many_arguments)]
fn execute_external_evaluators(
    config: &Config,
    case: &structtrace_core::dataset::Case,
    output: &structtrace_core::output::VariantOutput,
    variant_id: &str,
    project_root: &Path,
    bridge_path: &Path,
    retained_stderr: &mut Vec<u8>,
    retained_receipts: &mut Vec<ExternalEvaluatorReceipt>,
) -> BTreeMap<String, EvaluatorResult> {
    let mut results = BTreeMap::new();
    for evaluator in &config.evaluators {
        if !matches!(
            evaluator.kind,
            EvaluatorKind::Command { .. } | EvaluatorKind::Python { .. }
        ) {
            continue;
        }
        let run = run_external_evaluator(
            &evaluator.id,
            &evaluator.kind,
            case,
            output,
            EvaluatorRuntime {
                variant_id,
                working_directory: project_root,
                python_bridge: bridge_path,
                limits: &CommandLimits {
                    max_output_bytes: config.limits.max_output_bytes_per_case,
                    max_stderr_bytes: config.limits.max_stderr_bytes_per_process,
                },
            },
        );
        if !run.stderr.is_empty() {
            retained_stderr.extend_from_slice(
                format!(
                    "\n--- variant={variant_id} case={} evaluator={} ---\n",
                    case.id, evaluator.id
                )
                .as_bytes(),
            );
            retained_stderr.extend_from_slice(&run.stderr);
        }
        retained_receipts.push(run.receipt);
        results.insert(evaluator.id.clone(), run.result);
    }
    results
}

fn materialize_evaluator_bridge(storage_root: &Path) -> anyhow::Result<PathBuf> {
    let path = storage_root
        .join("runtime")
        .join("python-evaluator-bridge-v1.py");
    let parent = path
        .parent()
        .context("evaluator bridge path has no parent")?;
    std::fs::create_dir_all(parent)?;
    if std::fs::read(&path).ok().as_deref() != Some(EVALUATOR_BRIDGE_SOURCE.as_bytes()) {
        std::fs::write(&path, EVALUATOR_BRIDGE_SOURCE)?;
    }
    Ok(path)
}

fn resolve(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_owned()
    } else {
        root.join(path)
    }
}

fn read_recorded_variant(
    project_root: &Path,
    name: &str,
    variant: &VariantConfig,
    dataset: &Dataset,
) -> anyhow::Result<PreparedVariant> {
    let VariantConfig::Recorded { path } = variant else {
        anyhow::bail!(
            "variant `{name}` is not recorded-output mode; use `structtrace run` for configured adapters"
        );
    };
    let path = resolve(project_root, path);
    let outputs = RecordedOutputs::read(&path, dataset)?;
    Ok(PreparedVariant {
        source_label: path.display().to_string(),
        source_bytes: std::fs::read(&path)?,
        input_hash: outputs.source_hash,
        rows: outputs.rows,
        stderr: Vec::new(),
        protocol_errors: Vec::new(),
    })
}

/// Serialize adapter outputs as deterministic JSONL for portable replay.
pub fn outputs_jsonl(rows: &[structtrace_core::output::VariantOutput]) -> anyhow::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    for row in rows {
        serde_json::to_writer(&mut bytes, row)?;
        bytes.push(b'\n');
    }
    Ok(bytes)
}

/// Enforce raw-output retention before any portable output artifact is persisted.
pub fn apply_storage_retention(
    prepared: &mut PreparedVariant,
    retain_raw_outputs: bool,
    retain_provider_responses: bool,
    retain_prompts: bool,
) -> anyhow::Result<()> {
    for row in &mut prepared.rows {
        if !retain_raw_outputs {
            if row.parsed_output.is_none() {
                row.parsed_output = row
                    .raw_output
                    .as_deref()
                    .and_then(|raw| serde_json::from_str(raw).ok());
            }
            row.raw_output = None;
        }
        if !retain_provider_responses {
            if let Some(metadata) = row.metadata.as_object_mut() {
                metadata.remove("provider_response");
            }
            for retry in &mut row.retries {
                if let Some(object) = retry.as_object_mut() {
                    object.remove("response");
                }
            }
        }
        if !retain_prompts {
            if let Some(metadata) = row.metadata.as_object_mut() {
                metadata.remove("rendered_prompt");
            }
        }
    }
    prepared.source_bytes = outputs_jsonl(&prepared.rows)?;
    Ok(())
}

pub(crate) fn build_summary(
    run_id: &str,
    config: &Config,
    records: &[PairedCaseRecord],
) -> anyhow::Result<RunSummary> {
    let pairs = records
        .iter()
        .map(|record| {
            (
                record.baseline_evaluation.primary_pass,
                record.candidate_evaluation.primary_pass,
            )
        })
        .collect::<Vec<_>>();
    let paired = paired_metrics(&pairs);
    let bootstrap = paired_bootstrap(
        &pairs,
        config.analysis.bootstrap.samples,
        config.analysis.bootstrap.confidence,
        config.analysis.bootstrap.seed,
    )?;
    let baseline = variant_summary(records, true, &config.analysis.primary_outcome);
    let candidate = variant_summary(records, false, &config.analysis.primary_outcome);
    let matched_operational = matched_operational_summary(records);
    let evaluator_passes = config
        .evaluators
        .iter()
        .map(|evaluator| {
            let baseline_pass = records
                .iter()
                .filter(|record| {
                    record
                        .baseline_evaluation
                        .evaluators
                        .get(&evaluator.id)
                        .is_some_and(|result| result.passed)
                })
                .count();
            let candidate_pass = records
                .iter()
                .filter(|record| {
                    record
                        .candidate_evaluation
                        .evaluators
                        .get(&evaluator.id)
                        .is_some_and(|result| result.passed)
                })
                .count();
            (
                evaluator.id.clone(),
                EvaluatorComparison {
                    total: records.len(),
                    baseline_pass,
                    candidate_pass,
                },
            )
        })
        .collect();
    let gate = evaluate_gate(
        &config.gate,
        &GateInputs {
            primary: &paired,
            baseline_valid_but_wrong_rate: rate(baseline.valid_but_wrong, baseline.total),
            candidate_valid_but_wrong_rate: rate(candidate.valid_but_wrong, candidate.total),
            candidate_schema_validity: rate(candidate.schema_valid, candidate.total),
            candidate_error_rate: rate(candidate.errors, candidate.total),
            candidate_timeout_rate: rate(candidate.timeouts, candidate.total),
            baseline_p95_latency_ms: matched_operational.baseline_p95_latency_ms,
            candidate_p95_latency_ms: matched_operational.candidate_p95_latency_ms,
            latency_coverage: rate(matched_operational.latency_pairs, records.len()),
            baseline_average_cost: matched_operational
                .baseline_average_cost
                .as_deref()
                .and_then(|value| value.parse().ok()),
            candidate_average_cost: matched_operational
                .candidate_average_cost
                .as_deref()
                .and_then(|value| value.parse().ok()),
            cost_coverage: rate(matched_operational.cost_pairs, records.len()),
        },
    );
    Ok(RunSummary {
        artifact_format_version: ARTIFACT_FORMAT_VERSION,
        run_id: run_id.to_owned(),
        primary_outcome: config.analysis.primary_outcome.clone(),
        baseline,
        candidate,
        matched_operational,
        paired,
        bootstrap,
        gate,
        evaluator_passes,
        field_hotspots: field_hotspots(config, records),
    })
}

fn matched_operational_summary(records: &[PairedCaseRecord]) -> MatchedOperationalSummary {
    let latency_pairs = records
        .iter()
        .filter_map(|record| {
            Some((
                record.baseline_output.latency_ms? as f64,
                record.candidate_output.latency_ms? as f64,
            ))
        })
        .collect::<Vec<_>>();
    let mut cost_pairs = Vec::new();
    for record in records {
        let (Some(baseline), Some(candidate)) = (
            record.baseline_output.cost.as_ref(),
            record.candidate_output.cost.as_ref(),
        ) else {
            continue;
        };
        if baseline.currency != candidate.currency {
            continue;
        }
        let (Ok(baseline_amount), Ok(candidate_amount)) = (
            baseline.amount.parse::<Decimal>(),
            candidate.amount.parse::<Decimal>(),
        ) else {
            continue;
        };
        cost_pairs.push((baseline_amount, candidate_amount, baseline.currency.clone()));
    }
    let currencies = cost_pairs
        .iter()
        .map(|(_, _, currency)| currency.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let comparable_cost_pairs = if currencies.len() == 1 {
        cost_pairs.len()
    } else {
        0
    };
    let (baseline_average_cost, candidate_average_cost, currency) = if comparable_cost_pairs > 0 {
        let denominator = Decimal::from_usize(comparable_cost_pairs).expect("positive count");
        let baseline = cost_pairs
            .iter()
            .map(|(amount, _, _)| *amount)
            .sum::<Decimal>()
            / denominator;
        let candidate = cost_pairs
            .iter()
            .map(|(_, amount, _)| *amount)
            .sum::<Decimal>()
            / denominator;
        (
            Some(baseline.normalize().to_string()),
            Some(candidate.normalize().to_string()),
            currencies.first().map(|value| (*value).to_owned()),
        )
    } else {
        (None, None, None)
    };
    MatchedOperationalSummary {
        total_pairs: records.len(),
        latency_pairs: latency_pairs.len(),
        baseline_p95_latency_ms: percentile(
            &latency_pairs.iter().map(|pair| pair.0).collect::<Vec<_>>(),
            0.95,
        ),
        candidate_p95_latency_ms: percentile(
            &latency_pairs.iter().map(|pair| pair.1).collect::<Vec<_>>(),
            0.95,
        ),
        cost_pairs: comparable_cost_pairs,
        baseline_average_cost,
        candidate_average_cost,
        currency,
    }
}

fn variant_summary(
    records: &[PairedCaseRecord],
    baseline: bool,
    primary_outcome: &str,
) -> VariantSummary {
    let evaluations = records.iter().map(|record| {
        if baseline {
            (&record.baseline_output, &record.baseline_evaluation)
        } else {
            (&record.candidate_output, &record.candidate_evaluation)
        }
    });
    let mut summary = VariantSummary {
        total: records.len(),
        ..VariantSummary::default()
    };
    for (output, evaluation) in evaluations {
        summary.parse_valid += usize::from(evaluation.parse_valid);
        summary.schema_valid += usize::from(evaluation.schema_valid);
        summary.primary_pass += usize::from(evaluation.primary_pass);
        match evaluation.outcomes.get(primary_outcome) {
            Some(structtrace_core::evaluation::OutcomeStatus::True) => {}
            Some(structtrace_core::evaluation::OutcomeStatus::False) => summary.primary_failed += 1,
            Some(structtrace_core::evaluation::OutcomeStatus::Error) => summary.primary_error += 1,
            Some(structtrace_core::evaluation::OutcomeStatus::NotApplicable) => {
                summary.primary_not_applicable += 1;
            }
            None => summary.primary_unscored += 1,
        }
        summary.valid_but_wrong += usize::from(evaluation.valid_but_wrong);
        summary.errors += usize::from(output.status != OutputStatus::Ok);
        summary.timeouts += usize::from(
            output
                .error
                .as_ref()
                .is_some_and(|error| error.kind == "timeout"),
        );
        summary.operational.retry_attempts += output.retries.len();
        if let Some(usage) = &output.usage {
            summary.operational.usage_observations += 1;
            summary.operational.input_tokens += usage.input_tokens.unwrap_or(0);
            summary.operational.output_tokens += usage.output_tokens.unwrap_or(0);
        }
    }
    let outputs = records.iter().map(|record| {
        if baseline {
            &record.baseline_output
        } else {
            &record.candidate_output
        }
    });
    let latencies = outputs
        .clone()
        .filter_map(|output| output.latency_ms.map(|value| value as f64))
        .collect::<Vec<_>>();
    summary.operational.latency_observations = latencies.len();
    summary.operational.mean_latency_ms = mean(&latencies);
    summary.operational.median_latency_ms = percentile(&latencies, 0.5);
    summary.operational.p95_latency_ms = percentile(&latencies, 0.95);
    let costs = outputs
        .filter_map(|output| output.cost.as_ref())
        .collect::<Vec<_>>();
    summary.operational.cost_observations = costs.len();
    let currencies = costs
        .iter()
        .map(|cost| cost.currency.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    summary.operational.mixed_currencies = currencies.len() > 1;
    if currencies.len() == 1 {
        let total = costs
            .iter()
            .map(|cost| cost.amount.parse::<Decimal>())
            .collect::<Result<Vec<_>, _>>()
            .ok()
            .map(|values| values.into_iter().sum::<Decimal>());
        if let Some(total) = total {
            summary.operational.total_cost = Some(total.normalize().to_string());
            summary.operational.average_cost = Decimal::from_usize(costs.len())
                .map(|count| (total / count).normalize().to_string());
            summary.operational.currency = currencies.first().map(|value| (*value).to_owned());
        }
    }
    summary
}

fn field_hotspots(config: &Config, records: &[PairedCaseRecord]) -> Vec<FieldHotspot> {
    let evaluator_ids = config
        .evaluators
        .iter()
        .map(|evaluator| evaluator.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let mut hotspots = BTreeMap::<String, FieldHotspot>::new();
    for record in records {
        let baseline = field_statuses(&record.baseline_evaluation.evaluators, &evaluator_ids);
        let candidate = field_statuses(&record.candidate_evaluation.evaluators, &evaluator_ids);
        for pointer in baseline.keys().chain(candidate.keys()) {
            let hotspot = hotspots
                .entry(pointer.clone())
                .or_insert_with(|| FieldHotspot {
                    pointer: pointer.clone(),
                    regressions: 0,
                    improvements: 0,
                    candidate_failures: 0,
                });
            let baseline_pass = baseline.get(pointer).copied();
            let candidate_pass = candidate.get(pointer).copied();
            hotspot.regressions +=
                usize::from(baseline_pass == Some(true) && candidate_pass == Some(false));
            hotspot.improvements +=
                usize::from(baseline_pass == Some(false) && candidate_pass == Some(true));
            hotspot.candidate_failures += usize::from(candidate_pass == Some(false));
        }
    }
    let mut hotspots = hotspots.into_values().collect::<Vec<_>>();
    hotspots.sort_by(|left, right| {
        right
            .regressions
            .cmp(&left.regressions)
            .then_with(|| right.candidate_failures.cmp(&left.candidate_failures))
            .then_with(|| left.pointer.cmp(&right.pointer))
    });
    hotspots
}

fn field_statuses(
    results: &BTreeMap<String, EvaluatorResult>,
    evaluator_ids: &std::collections::BTreeSet<&str>,
) -> BTreeMap<String, bool> {
    let mut statuses = BTreeMap::<String, bool>::new();
    for (evaluator_id, result) in results {
        if !evaluator_ids.contains(evaluator_id.as_str()) {
            continue;
        }
        for field in &result.fields {
            match field.status {
                EvaluationStatus::Passed => {
                    statuses.entry(field.pointer.clone()).or_insert(true);
                }
                EvaluationStatus::Failed => {
                    statuses.insert(field.pointer.clone(), false);
                }
                EvaluationStatus::Error | EvaluationStatus::NotApplicable => {}
            }
        }
    }
    statuses
}

fn mean(values: &[f64]) -> Option<f64> {
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

fn percentile(values: &[f64], quantile: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut values = values.to_vec();
    values.sort_by(f64::total_cmp);
    Some(values[((values.len() - 1) as f64 * quantile).ceil() as usize])
}

fn rate(count: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        count as f64 / total as f64
    }
}

fn transition_name(baseline: bool, candidate: bool) -> &'static str {
    match (baseline, candidate) {
        (true, true) => "both_pass",
        (true, false) => "baseline_only_pass",
        (false, true) => "candidate_only_pass",
        (false, false) => "both_fail",
    }
}

fn summary_markdown(project_name: &str, summary: &RunSummary) -> String {
    let gate = if summary.gate.passed {
        "PASSED"
    } else {
        "FAILED"
    };
    format!(
        "# StructTrace run: {project_name}\n\n\
         **Release gate: {gate}**\n\n\
         | Metric | Baseline | Candidate |\n\
         |---|---:|---:|\n\
         | Primary outcome | {}/{} | {}/{} |\n\
         | Strict JSON | {}/{} | {}/{} |\n\
         | Schema valid | {}/{} | {}/{} |\n\
         | Valid but wrong | {}/{} | {}/{} |\n\n\
         Paired difference: **{:+.2} percentage points**  \n\
         Candidate-only wins: **{}**  \n\
         Baseline-only wins: **{}**  \n\
         Exact McNemar p: **{:.6}**  \n\
         Paired bootstrap interval: **[{:.2}, {:.2}] pp**\n",
        summary.baseline.primary_pass,
        summary.baseline.total,
        summary.candidate.primary_pass,
        summary.candidate.total,
        summary.baseline.parse_valid,
        summary.baseline.total,
        summary.candidate.parse_valid,
        summary.candidate.total,
        summary.baseline.schema_valid,
        summary.baseline.total,
        summary.candidate.schema_valid,
        summary.candidate.total,
        summary.baseline.valid_but_wrong,
        summary.baseline.total,
        summary.candidate.valid_but_wrong,
        summary.candidate.total,
        summary.paired.difference_pp,
        summary.paired.candidate_only_pass,
        summary.paired.baseline_only_pass,
        summary.paired.mcnemar_exact_p,
        summary.bootstrap.lower_pp,
        summary.bootstrap.upper_pp,
    )
}

fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    atomic_write(path, &bytes)
}

fn atomic_write_jsonl<T: Serialize>(path: &Path, values: &[T]) -> anyhow::Result<()> {
    let mut bytes = Vec::new();
    for value in values {
        serde_json::to_writer(&mut bytes, value)?;
        bytes.push(b'\n');
    }
    atomic_write(path, &bytes)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let parent = path.parent().context("artifact path has no parent")?;
    std::fs::create_dir_all(parent)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file_mut().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

fn unix_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use tempfile::tempdir;

    use super::*;

    fn write(path: &Path, contents: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn recorded_workflow_preserves_missing_output_in_denominator() {
        let root = tempdir().unwrap();
        write(
            &root.path().join("data.jsonl"),
            "{\"id\":\"a\",\"input\":{},\"expected\":{\"label\":\"yes\"}}\n{\"id\":\"b\",\"input\":{},\"expected\":{\"label\":\"no\"}}\n",
        );
        write(
            &root.path().join("schema.json"),
            "{\"type\":\"object\",\"required\":[\"label\"],\"properties\":{\"label\":{\"enum\":[\"yes\",\"no\"]}},\"additionalProperties\":false}",
        );
        write(
            &root.path().join("baseline.jsonl"),
            "{\"case_id\":\"a\",\"status\":\"ok\",\"raw_output\":\"{\\\"label\\\":\\\"yes\\\"}\"}\n{\"case_id\":\"b\",\"status\":\"ok\",\"raw_output\":\"{\\\"label\\\":\\\"no\\\"}\"}\n",
        );
        write(
            &root.path().join("candidate.jsonl"),
            "{\"case_id\":\"a\",\"status\":\"ok\",\"raw_output\":\"{\\\"label\\\":\\\"no\\\"}\"}\n",
        );
        write(
            &root.path().join("structtrace.yaml"),
            r#"version: 1
project:
  name: test
dataset:
  path: data.jsonl
schema:
  path: schema.json
variants:
  baseline:
    kind: recorded
    path: baseline.jsonl
  candidate:
    kind: recorded
    path: candidate.jsonl
evaluators:
  - id: label
    kind: json_pointer_exact
    pointer: /label
    expected_pointer: /label
outcomes:
  semantic_correct:
    all_of: [label]
analysis:
  primary_outcome: semantic_correct
  bootstrap:
    samples: 100
    confidence: 0.95
    seed: 17
gate:
  max_primary_regression_pp: 0
"#,
        );
        let run = run_recorded(root.path(), Path::new("structtrace.yaml")).unwrap();
        assert_eq!(run.summary.baseline.primary_pass, 2);
        assert_eq!(run.summary.candidate.primary_pass, 0);
        assert_eq!(run.summary.candidate.total, 2);
        assert_eq!(run.summary.candidate.errors, 1);
        assert!(!run.summary.gate.passed);
        assert!(run.run_dir.join("manifest.json").is_file());
        assert!(run.run_dir.join("run.sqlite3").is_file());
    }

    #[test]
    fn hotspot_attributes_only_the_failing_pointer() {
        let root = tempdir().unwrap();
        write(
            &root.path().join("data.jsonl"),
            "{\"id\":\"a\",\"input\":{},\"expected\":{\"priority\":\"high\",\"team\":\"ops\",\"human\":true}}\n",
        );
        write(&root.path().join("schema.json"), "{\"type\":\"object\"}");
        write(
            &root.path().join("baseline.jsonl"),
            "{\"case_id\":\"a\",\"status\":\"ok\",\"raw_output\":\"{\\\"priority\\\":\\\"high\\\",\\\"team\\\":\\\"ops\\\",\\\"human\\\":true}\"}\n",
        );
        write(
            &root.path().join("candidate.jsonl"),
            "{\"case_id\":\"a\",\"status\":\"ok\",\"raw_output\":\"{\\\"priority\\\":\\\"low\\\",\\\"team\\\":\\\"ops\\\",\\\"human\\\":true}\"}\n",
        );
        write(
            &root.path().join("structtrace.yaml"),
            r#"version: 1
project: {name: field-hotspot}
dataset: {path: data.jsonl}
schema: {path: schema.json}
variants:
  baseline: {kind: recorded, path: baseline.jsonl}
  candidate: {kind: recorded, path: candidate.jsonl}
evaluators:
  - id: fields
    kind: json_pointers_exact
    pointers:
      - {pointer: /priority, expected_pointer: /priority}
      - {pointer: /team, expected_pointer: /team}
      - {pointer: /human, expected_pointer: /human}
outcomes:
  correct: {all_of: [fields]}
analysis:
  primary_outcome: correct
  bootstrap: {samples: 100, confidence: 0.95, seed: 17}
"#,
        );
        let run = run_recorded(root.path(), Path::new("structtrace.yaml")).unwrap();
        let regressions = run
            .summary
            .field_hotspots
            .iter()
            .filter(|hotspot| hotspot.regressions > 0)
            .collect::<Vec<_>>();
        assert_eq!(regressions.len(), 1);
        assert_eq!(regressions[0].pointer, "/priority");
    }

    #[test]
    fn disabled_raw_retention_removes_source_text_before_persistence() {
        let source = b"{\"case_id\":\"one\",\"status\":\"ok\",\"raw_output\":\"{\\\"label\\\":\\\"yes\\\"}\",\"metadata\":{},\"retries\":[]}\n".to_vec();
        let mut prepared = PreparedVariant {
            source_label: "test".to_owned(),
            input_hash: hash_bytes(&source),
            source_bytes: source,
            rows: vec![structtrace_core::output::VariantOutput {
                case_id: "one".to_owned(),
                status: OutputStatus::Ok,
                raw_output: Some("{\"label\":\"yes\"}".to_owned()),
                parsed_output: None,
                error: None,
                latency_ms: None,
                usage: None,
                cost: None,
                metadata: serde_json::json!({
                    "provider_response": {"secret": "payload"},
                    "rendered_prompt": "private prompt"
                }),
                retries: vec![serde_json::json!({"response": {"secret": "retry"}})],
            }],
            stderr: vec![],
            protocol_errors: vec![],
        };
        apply_storage_retention(&mut prepared, false, false, false).unwrap();
        assert!(prepared.rows[0].raw_output.is_none());
        assert_eq!(
            prepared.rows[0].parsed_output,
            Some(serde_json::json!({"label": "yes"}))
        );
        assert!(
            !String::from_utf8(prepared.source_bytes)
                .unwrap()
                .contains("raw_output")
        );
        assert!(prepared.rows[0].metadata.get("provider_response").is_none());
        assert!(prepared.rows[0].metadata.get("rendered_prompt").is_none());
        assert!(prepared.rows[0].retries[0].get("response").is_none());
    }

    #[test]
    fn custom_python_evaluator_runs_and_replays() {
        let interpreter = ["python3", "python"].into_iter().find(|program| {
            std::process::Command::new(program)
                .arg("--version")
                .output()
                .is_ok_and(|output| output.status.success())
        });
        let Some(interpreter) = interpreter else {
            return;
        };
        let root = tempdir().unwrap();
        write(
            &root.path().join("data.jsonl"),
            "{\"id\":\"a\",\"input\":{},\"expected\":{\"label\":\"yes\"}}\n",
        );
        write(
            &root.path().join("schema.json"),
            "{\"type\":\"object\",\"required\":[\"label\"],\"properties\":{\"label\":{\"type\":\"string\"}}}",
        );
        write(
            &root.path().join("baseline.jsonl"),
            "{\"case_id\":\"a\",\"status\":\"ok\",\"raw_output\":\"{\\\"label\\\":\\\"yes\\\"}\"}\n",
        );
        write(
            &root.path().join("candidate.jsonl"),
            "{\"case_id\":\"a\",\"status\":\"ok\",\"raw_output\":\"{\\\"label\\\":\\\"no\\\"}\"}\n",
        );
        write(
            &root.path().join("variants/evaluator.py"),
            "import json\n\ndef score(request):\n    actual = json.loads(request['model_output']['raw_output'])['label']\n    expected = request['case']['expected']['label']\n    passed = actual == expected\n    return {'status': 'passed' if passed else 'failed', 'score': 1 if passed else 0, 'message': 'labels compared', 'details': {'actual': actual, 'expected': expected}}\n",
        );
        write(
            &root.path().join("structtrace.yaml"),
            &format!(
                r#"version: 1
project: {{name: external-evaluator-test}}
dataset: {{path: data.jsonl}}
schema: {{path: schema.json}}
variants:
  baseline: {{kind: recorded, path: baseline.jsonl}}
  candidate: {{kind: recorded, path: candidate.jsonl}}
evaluators:
  - id: business
    kind: python
    interpreter: {interpreter}
    callable: variants.evaluator:score
    timeout_ms: 2000
outcomes:
  correct: {{all_of: [business]}}
analysis:
  primary_outcome: correct
  bootstrap: {{samples: 100, confidence: 0.95, seed: 17}}
"#
            ),
        );
        let run = run_recorded(root.path(), Path::new("structtrace.yaml")).unwrap();
        assert_eq!(run.summary.baseline.primary_pass, 1);
        assert_eq!(run.summary.candidate.primary_pass, 0);
        let replay = crate::replay::replay_run(&run.run_dir).unwrap();
        assert!(replay.verified);
        assert_eq!(replay.external_evaluator_receipts_verified, 2);
        assert_eq!(replay.external_evaluator_programs_reexecuted, 0);
        assert!(
            run.run_dir
                .join("external-evaluator-receipts.jsonl")
                .is_file()
        );
    }
}

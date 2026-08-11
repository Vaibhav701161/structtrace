//! Adapter-aware run orchestration with one shared evaluation path.

use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use structtrace_adapters::{
    command::{AdapterRun, CommandLimits, run_command},
    openai::run_openai_compatible,
    python::{BRIDGE_SOURCE, run_python},
};
use structtrace_core::{
    ARTIFACT_FORMAT_VERSION,
    artifact::{RunKind, RunStatus},
    config::{Config, VariantConfig},
    dataset::{Dataset, ExecutionToken, VariantCase},
    evaluation::{compile_schema, validate_references},
    hashing::{hash_bytes, hash_canonical_json, hash_file},
    output::RecordedOutputs,
};
use tempfile::NamedTempFile;
use ulid::Ulid;

use crate::{
    recorded::{CompletedRun, PreparedVariant, finalize_prepared_for_run, outputs_jsonl},
    storage::RunStore,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExecutionCheckpoint {
    artifact_format_version: u32,
    run_id: String,
    configuration_file_hash: String,
    normalized_configuration_hash: String,
    dataset_hash: String,
    schema_hash: String,
    execution_definition_hash: String,
    implementation_fingerprint: String,
    model_facing_schema_hashes: BTreeMap<String, String>,
    completed_outputs: BTreeMap<String, String>,
    original_input_hashes: BTreeMap<String, String>,
    source_labels: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
struct CapturedModelSchema {
    bytes: Vec<u8>,
    value: Value,
}

/// Execute the configured baseline and candidate, then use the shared evaluator pipeline.
pub async fn run_configured(
    project_root: &Path,
    config_path: &Path,
) -> anyhow::Result<CompletedRun> {
    run_configured_inner(project_root, config_path, None).await
}

/// Resume a hash-compatible interrupted run without reinvoking completed variants.
pub async fn resume_configured(
    project_root: &Path,
    config_path: &Path,
    run_id: &str,
) -> anyhow::Result<CompletedRun> {
    validate_run_id(run_id)?;
    run_configured_inner(project_root, config_path, Some(run_id.to_owned())).await
}

async fn run_configured_inner(
    project_root: &Path,
    config_path: &Path,
    resume_run_id: Option<String>,
) -> anyhow::Result<CompletedRun> {
    let project_root = project_root
        .canonicalize()
        .with_context(|| format!("project root {} does not exist", project_root.display()))?;
    let config_path = resolve(&project_root, config_path);
    let config_source_bytes = structtrace_core::hashing::read_bounded(
        &config_path,
        structtrace_core::config::HARD_MAX_CONFIG_BYTES,
        "configuration",
    )?;
    let config = Config::from_bytes(&config_path, &config_source_bytes)?;
    anyhow::ensure!(
        config_source_bytes.len() <= config.limits.max_config_bytes,
        "configuration exceeds limits.max_config_bytes"
    );
    let dataset_path = resolve(&project_root, &config.dataset.path);
    let dataset = Dataset::read_bounded(&dataset_path, &config.dataset.fields, &config.limits)?;
    let reference_issues = validate_references(&dataset.cases, &config.evaluators);
    if !reference_issues.is_empty() {
        anyhow::bail!(
            "dataset reference preflight failed with {} issue(s); no adapter execution or candidate scoring occurred: {}",
            reference_issues.len(),
            serde_json::to_string(&reference_issues.iter().take(20).collect::<Vec<_>>())?
        );
    }
    let schema_path = resolve(&project_root, &config.schema.path);
    let schema_bytes = structtrace_core::hashing::read_bounded(
        &schema_path,
        config.limits.max_schema_bytes,
        "schema",
    )?;
    let schema_value = structtrace_core::strict_json::value_from_slice(&schema_bytes)
        .with_context(|| format!("schema {} is not valid JSON", schema_path.display()))?;
    compile_schema(&schema_value)?;
    let captured_model_schemas =
        capture_model_facing_schemas(&project_root, &config, &schema_bytes, &schema_value)?;
    let model_facing_schema_hashes = captured_model_schemas
        .iter()
        .map(|(name, schema)| (name.clone(), hash_bytes(&schema.bytes)))
        .collect::<BTreeMap<_, _>>();

    let storage_root = resolve(&project_root, &config.storage.root);
    let definition_hash = hash_canonical_json(&serde_json::json!({
        "variants": config.variants,
        "evaluators": config.evaluators,
        "outcomes": config.outcomes,
        "primary_outcome": config.analysis.primary_outcome,
        "bootstrap": config.analysis.bootstrap,
        "gate": config.gate,
        "limits": config.limits,
    }))?;
    let initial_implementation_fingerprint = implementation_fingerprint(&project_root, &config)?;
    let expected = ExecutionCheckpoint {
        artifact_format_version: ARTIFACT_FORMAT_VERSION,
        run_id: resume_run_id.clone().unwrap_or_default(),
        configuration_file_hash: hash_bytes(&config_source_bytes),
        normalized_configuration_hash: hash_canonical_json(&config)?,
        dataset_hash: dataset.source_hash.clone(),
        schema_hash: hash_bytes(&schema_bytes),
        execution_definition_hash: definition_hash,
        implementation_fingerprint: initial_implementation_fingerprint.clone(),
        model_facing_schema_hashes: model_facing_schema_hashes.clone(),
        completed_outputs: BTreeMap::new(),
        original_input_hashes: BTreeMap::new(),
        source_labels: BTreeMap::new(),
    };
    let (run_id, store, mut checkpoint) =
        match resume_run_id {
            Some(run_id) => {
                let run_dir = storage_root.join("runs").join(&run_id);
                let checkpoint_path = run_dir.join("execution-checkpoint.json");
                let bytes = structtrace_core::hashing::read_bounded(
                    &checkpoint_path,
                    config.limits.max_replay_artifact_bytes,
                    "execution checkpoint",
                )
                .with_context(|| {
                    format!(
                        "run `{run_id}` has no resumable execution checkpoint at {}",
                        checkpoint_path.display()
                    )
                })?;
                let checkpoint: ExecutionCheckpoint =
                    structtrace_core::strict_json::from_slice(&bytes)
                        .context("execution checkpoint is invalid JSON")?;
                verify_resume_compatibility(&expected, &checkpoint)?;
                let store = RunStore::open(&run_dir)?;
                let status = store.status(&run_id)?;
                anyhow::ensure!(
                    status != RunStatus::Complete && status != RunStatus::Corrupt,
                    "run `{run_id}` has state {status:?} and cannot be resumed"
                );
                store.set_status(&run_id, RunStatus::Interrupted)?;
                store.record_event("resume_validated", &serde_json::json!({
                "completed_variants": checkpoint.completed_outputs.keys().collect::<Vec<_>>()
            }))?;
                store.set_status(&run_id, RunStatus::Running)?;
                (run_id, store, checkpoint)
            }
            None => {
                let run_id = Ulid::new().to_string();
                let store = RunStore::create(&storage_root, &run_id, RunKind::Production)?;
                store.set_status(&run_id, RunStatus::Validating)?;
                let mut checkpoint = expected;
                checkpoint.run_id.clone_from(&run_id);
                let run_dir = store.run_dir();
                atomic_write(
                    run_dir.join("inputs/configuration.json"),
                    &serde_json::to_vec_pretty(&config)?,
                )?;
                atomic_write(
                    run_dir.join("inputs/configuration.source"),
                    &config_source_bytes,
                )?;
                atomic_write(run_dir.join("inputs/dataset.jsonl"), &dataset.source_bytes)?;
                atomic_write(run_dir.join("inputs/schema.json"), &schema_bytes)?;
                for (variant, schema) in &captured_model_schemas {
                    atomic_write(
                        run_dir
                            .join("inputs/variants")
                            .join(variant)
                            .join("model-facing-schema.json"),
                        &schema.bytes,
                    )?;
                }
                write_checkpoint(run_dir, &checkpoint)?;
                store.record_event(
                    "inputs_validated",
                    &serde_json::json!({"cases": dataset.cases.len()}),
                )?;
                store.set_status(&run_id, RunStatus::Running)?;
                (run_id, store, checkpoint)
            }
        };

    let mut failure_guard = store.failure_guard(&run_id);

    let bridge_path = materialize_python_bridge(&project_root, &config)?;
    let baseline = prepare_or_restore(
        &project_root,
        "baseline",
        config.variants.get("baseline").expect("validated config"),
        &dataset,
        captured_model_schemas
            .get("baseline")
            .map(|schema| &schema.value),
        &bridge_path,
        &config.limits,
        store.run_dir(),
        &mut checkpoint,
    )
    .await?;
    let candidate = prepare_or_restore(
        &project_root,
        "candidate",
        config.variants.get("candidate").expect("validated config"),
        &dataset,
        captured_model_schemas
            .get("candidate")
            .map(|schema| &schema.value),
        &bridge_path,
        &config.limits,
        store.run_dir(),
        &mut checkpoint,
    )
    .await?;
    let final_implementation_fingerprint = implementation_fingerprint(&project_root, &config)?;
    anyhow::ensure!(
        final_implementation_fingerprint == initial_implementation_fingerprint,
        "configured implementation inputs changed during execution; finalization refused"
    );
    let final_model_schemas =
        capture_model_facing_schemas(&project_root, &config, &schema_bytes, &schema_value)?;
    let final_model_schema_hashes = final_model_schemas
        .iter()
        .map(|(name, schema)| (name.clone(), hash_bytes(&schema.bytes)))
        .collect::<BTreeMap<_, _>>();
    anyhow::ensure!(
        final_model_schema_hashes == model_facing_schema_hashes,
        "a model-facing structured-output schema changed during execution; finalization refused"
    );
    failure_guard.disarm();
    drop(failure_guard);
    drop(store);

    let completed = finalize_prepared_for_run(
        &project_root,
        config_source_bytes,
        config,
        dataset,
        schema_bytes,
        baseline,
        candidate,
        Some(run_id),
        Some(initial_implementation_fingerprint),
        RunKind::Production,
    )?;
    let checkpoint_path = completed.run_dir.join("execution-checkpoint.json");
    if checkpoint_path.is_file() {
        std::fs::remove_file(checkpoint_path)?;
    }
    Ok(completed)
}

fn capture_model_facing_schemas(
    project_root: &Path,
    config: &Config,
    external_schema_bytes: &[u8],
    external_schema: &Value,
) -> anyhow::Result<BTreeMap<String, CapturedModelSchema>> {
    let mut captured = BTreeMap::new();
    for (name, variant) in &config.variants {
        let VariantConfig::OpenaiCompatible(adapter) = variant else {
            continue;
        };
        let Some(structured) = &adapter.structured_output else {
            continue;
        };
        let (bytes, value) = if let Some(configured_path) = &structured.schema {
            let path = resolve(project_root, configured_path);
            let metadata = std::fs::symlink_metadata(&path)
                .with_context(|| format!("model-facing schema is missing: {}", path.display()))?;
            anyhow::ensure!(
                metadata.is_file() && !metadata.file_type().is_symlink(),
                "model-facing schema must be a regular non-symlink file: {}",
                path.display()
            );
            let bytes = structtrace_core::hashing::read_bounded(
                &path,
                config.limits.max_schema_bytes,
                "model-facing schema",
            )?;
            let value =
                structtrace_core::strict_json::value_from_slice(&bytes).with_context(|| {
                    format!("model-facing schema {} is invalid JSON", path.display())
                })?;
            (bytes, value)
        } else {
            (external_schema_bytes.to_vec(), external_schema.clone())
        };
        compile_schema(&value)?;
        captured.insert(name.clone(), CapturedModelSchema { bytes, value });
    }
    Ok(captured)
}

fn implementation_fingerprint(project_root: &Path, config: &Config) -> anyhow::Result<String> {
    const MAX_BOUND_FILE_BYTES: u64 = 256 * 1024 * 1024;
    const MAX_TOTAL_BOUND_BYTES: u64 = 512 * 1024 * 1024;
    let mut sources = BTreeMap::<String, String>::new();
    let mut bound_bytes = 0_u64;
    let mut has_live_implementation = false;
    for (variant_id, variant) in &config.variants {
        match variant {
            VariantConfig::Command {
                command,
                implementation,
                ..
            } => {
                has_live_implementation = true;
                if let Some(path) = resolve_executable(project_root, &command.program) {
                    bind_file(
                        format!("variant:{variant_id}:program"),
                        &path,
                        &mut sources,
                        &mut bound_bytes,
                        MAX_BOUND_FILE_BYTES,
                        MAX_TOTAL_BOUND_BYTES,
                    )?;
                }
                bind_declared_implementation(
                    project_root,
                    &format!("variant:{variant_id}"),
                    implementation,
                    &mut sources,
                    &mut bound_bytes,
                )?;
            }
            VariantConfig::Python {
                callable,
                implementation,
                ..
            } => {
                has_live_implementation = true;
                if let Some((module, _)) = callable.split_once(':') {
                    let path = project_root.join(format!("{}.py", module.replace('.', "/")));
                    if path.is_file() {
                        bind_file(
                            format!("variant:{variant_id}:python"),
                            &path,
                            &mut sources,
                            &mut bound_bytes,
                            MAX_BOUND_FILE_BYTES,
                            MAX_TOTAL_BOUND_BYTES,
                        )?;
                    }
                }
                bind_declared_implementation(
                    project_root,
                    &format!("variant:{variant_id}"),
                    implementation,
                    &mut sources,
                    &mut bound_bytes,
                )?;
            }
            VariantConfig::Recorded { .. } | VariantConfig::OpenaiCompatible(_) => {}
        }
    }
    for evaluator in &config.evaluators {
        match &evaluator.kind {
            structtrace_core::config::EvaluatorKind::Command { command, .. } => {
                has_live_implementation = true;
                if let Some(path) = resolve_executable(project_root, &command.program) {
                    bind_file(
                        format!("evaluator:{}:program", evaluator.id),
                        &path,
                        &mut sources,
                        &mut bound_bytes,
                        MAX_BOUND_FILE_BYTES,
                        MAX_TOTAL_BOUND_BYTES,
                    )?;
                }
            }
            structtrace_core::config::EvaluatorKind::Python { callable, .. } => {
                has_live_implementation = true;
                if let Some((module, _)) = callable.split_once(':') {
                    let path = project_root.join(format!("{}.py", module.replace('.', "/")));
                    if path.is_file() {
                        bind_file(
                            format!("evaluator:{}:python", evaluator.id),
                            &path,
                            &mut sources,
                            &mut bound_bytes,
                            MAX_BOUND_FILE_BYTES,
                            MAX_TOTAL_BOUND_BYTES,
                        )?;
                    }
                }
            }
            _ => {}
        }
        bind_declared_implementation(
            project_root,
            &format!("evaluator:{}", evaluator.id),
            &evaluator.implementation,
            &mut sources,
            &mut bound_bytes,
        )?;
    }
    if has_live_implementation {
        for lockfile in [
            "Cargo.lock",
            "uv.lock",
            "poetry.lock",
            "requirements.txt",
            "requirements.lock",
            "Pipfile.lock",
        ] {
            let path = project_root.join(lockfile);
            if path.is_file() {
                bind_file(
                    format!("lockfile:{lockfile}"),
                    &path,
                    &mut sources,
                    &mut bound_bytes,
                    MAX_BOUND_FILE_BYTES,
                    MAX_TOTAL_BOUND_BYTES,
                )?;
            }
        }
    }
    let interpreters = config
        .variants
        .values()
        .filter_map(|variant| match variant {
            VariantConfig::Python { interpreter, .. } => Some(interpreter),
            _ => None,
        })
        .chain(
            config
                .evaluators
                .iter()
                .filter_map(|evaluator| match &evaluator.kind {
                    structtrace_core::config::EvaluatorKind::Python { interpreter, .. } => {
                        Some(interpreter)
                    }
                    _ => None,
                }),
        )
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|interpreter| {
            let resolved_path = resolve_executable(project_root, interpreter)
                .and_then(|path| path.canonicalize().ok())
                .map(|path| path.display().to_string());
            let version = std::process::Command::new(interpreter)
                .arg("--version")
                .output()
                .ok()
                .map(|output| {
                    format!(
                        "{}{}",
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr)
                    )
                });
            (
                interpreter.clone(),
                serde_json::json!({"resolved_path": resolved_path, "version": version}),
            )
        })
        .collect::<BTreeMap<_, _>>();
    hash_canonical_json(&serde_json::json!({
        "sources": sources,
        "interpreters": interpreters,
        "binary": env!("CARGO_PKG_VERSION"),
    }))
    .map_err(Into::into)
}

fn resolve_executable(project_root: &Path, program: &str) -> Option<PathBuf> {
    let configured = resolve(project_root, Path::new(program));
    if configured.is_file() {
        return configured.canonicalize().ok();
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|directory| directory.join(program))
            .find(|path| path.is_file())
            .and_then(|path| path.canonicalize().ok())
    })
}

fn bind_declared_implementation(
    project_root: &Path,
    label: &str,
    implementation: &structtrace_core::config::ImplementationConfig,
    output: &mut BTreeMap<String, String>,
    total_bytes: &mut u64,
) -> anyhow::Result<()> {
    if let Some(digest) = &implementation.digest {
        output.insert(format!("{label}:declared-digest"), digest.clone());
    }
    for (index, source) in implementation.sources.iter().enumerate() {
        let path = resolve(project_root, source);
        bind_file(
            format!("{label}:declared-source:{index}"),
            &path,
            output,
            total_bytes,
            64 * 1024 * 1024,
            256 * 1024 * 1024,
        )?;
    }
    Ok(())
}

fn bind_file(
    label: impl Into<String>,
    path: &Path,
    output: &mut BTreeMap<String, String>,
    total_bytes: &mut u64,
    max_file_bytes: u64,
    max_total_bytes: u64,
) -> anyhow::Result<()> {
    let metadata = std::fs::symlink_metadata(path).with_context(|| {
        format!(
            "configured implementation input is missing: {}",
            path.display()
        )
    })?;
    anyhow::ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "configured implementation input must be a regular non-symlink file: {}",
        path.display()
    );
    anyhow::ensure!(
        metadata.len() <= max_file_bytes,
        "configured implementation input exceeds {max_file_bytes} bytes: {}",
        path.display()
    );
    *total_bytes = total_bytes
        .checked_add(metadata.len())
        .context("implementation fingerprint byte count overflow")?;
    anyhow::ensure!(
        *total_bytes <= max_total_bytes,
        "configured implementation inputs exceed {max_total_bytes} total bytes"
    );
    output.insert(label.into(), hash_file(path)?);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn prepare_or_restore(
    project_root: &Path,
    name: &str,
    variant: &VariantConfig,
    dataset: &Dataset,
    model_facing_schema: Option<&Value>,
    bridge_path: &Path,
    limits: &structtrace_core::config::LimitsConfig,
    run_dir: &Path,
    checkpoint: &mut ExecutionCheckpoint,
) -> anyhow::Result<PreparedVariant> {
    let output_path = run_dir.join("inputs").join(format!("{name}.jsonl"));
    if let Some(expected_hash) = checkpoint.completed_outputs.get(name) {
        let actual_hash = hash_file(&output_path)
            .with_context(|| format!("completed {name} checkpoint output is missing"))?;
        anyhow::ensure!(
            &actual_hash == expected_hash,
            "completed {name} output hash changed; resume refused"
        );
        let outputs = RecordedOutputs::read_bounded(&output_path, dataset, limits)?;
        return Ok(PreparedVariant {
            source_label: checkpoint
                .source_labels
                .get(name)
                .cloned()
                .unwrap_or_else(|| format!("checkpoint:{name}")),
            source_bytes: outputs.source_bytes,
            input_hash: checkpoint
                .original_input_hashes
                .get(name)
                .cloned()
                .context("checkpoint is missing the original input hash")?,
            rows: outputs.rows,
            stderr: read_optional(&run_dir.join("logs").join(format!("{name}.stderr.log")))?,
            protocol_errors: read_optional_json(
                &run_dir
                    .join("logs")
                    .join(format!("{name}.protocol-errors.json")),
            )?,
        });
    }
    let prepared = prepare_variant(
        project_root,
        name,
        variant,
        dataset,
        model_facing_schema,
        bridge_path,
        limits,
        run_dir
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .context("run directory has no valid run nonce")?,
    )
    .await?;
    let retained_config_bytes = structtrace_core::hashing::read_bounded(
        &run_dir.join("inputs/configuration.json"),
        limits.max_config_bytes,
        "retained configuration",
    )?;
    let retained_config: Config =
        structtrace_core::strict_json::from_slice(&retained_config_bytes)?;
    // Keep the immutable capture through analysis. The finalizer applies retention only after
    // strict parsing, validation, and external evaluator execution are frozen.
    atomic_write(output_path, &prepared.source_bytes)?;
    let retained_log_bytes = std::fs::read_dir(run_dir.join("logs"))?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.metadata().ok())
        .map(|metadata| metadata.len() as usize)
        .sum::<usize>();
    let mut process_log_budget = retained_config
        .storage
        .process_logs
        .max_total_bytes
        .saturating_sub(retained_log_bytes);
    if let Some(log) =
        crate::process_logs::retain(&retained_config, &prepared.stderr, &mut process_log_budget)
    {
        atomic_write(
            run_dir.join("logs").join(format!("{name}.stderr.log")),
            &log,
        )?;
    }
    if !prepared.protocol_errors.is_empty() {
        let bytes = serde_json::to_vec_pretty(&prepared.protocol_errors)?;
        if let Some(log) =
            crate::process_logs::retain(&retained_config, &bytes, &mut process_log_budget)
        {
            atomic_write(
                run_dir
                    .join("logs")
                    .join(format!("{name}.protocol-errors.json")),
                &log,
            )?;
        }
    }
    checkpoint
        .completed_outputs
        .insert(name.to_owned(), hash_bytes(&prepared.source_bytes));
    checkpoint
        .original_input_hashes
        .insert(name.to_owned(), prepared.input_hash.clone());
    checkpoint
        .source_labels
        .insert(name.to_owned(), prepared.source_label.clone());
    write_checkpoint(run_dir, checkpoint)?;
    Ok(prepared)
}

#[allow(clippy::too_many_arguments)]
async fn prepare_variant(
    project_root: &Path,
    name: &str,
    variant: &VariantConfig,
    dataset: &Dataset,
    model_facing_schema: Option<&Value>,
    bridge_path: &Path,
    limits: &structtrace_core::config::LimitsConfig,
    run_nonce: &str,
) -> anyhow::Result<PreparedVariant> {
    let variant_cases = variant_cases_for_run(dataset, run_nonce)?;
    match variant {
        VariantConfig::Recorded { path } => {
            let path = resolve(project_root, path);
            let outputs = RecordedOutputs::read_bounded(&path, dataset, limits)
                .with_context(|| format!("could not read {} output {}", name, path.display()))?;
            Ok(PreparedVariant {
                source_label: path.display().to_string(),
                source_bytes: outputs.source_bytes,
                input_hash: outputs.source_hash,
                rows: outputs.rows,
                stderr: Vec::new(),
                protocol_errors: Vec::new(),
            })
        }
        VariantConfig::Command {
            command,
            process_mode,
            timeout_ms,
            ..
        } => {
            let run = run_command(
                command,
                *process_mode,
                *timeout_ms,
                &variant_cases,
                project_root,
                &CommandLimits {
                    max_output_bytes: limits.max_output_bytes_per_case,
                    max_stderr_bytes: limits.max_stderr_bytes_per_process,
                },
            )
            .await;
            from_adapter(
                format!("command:{name}:{}", command.program),
                remap_adapter_outputs(run, dataset, &variant_cases)?,
            )
        }
        VariantConfig::Python {
            interpreter,
            callable,
            timeout_ms,
            ..
        } => {
            let run = run_python(
                interpreter,
                callable,
                *timeout_ms,
                &variant_cases,
                project_root,
                bridge_path,
                &CommandLimits {
                    max_output_bytes: limits.max_output_bytes_per_case,
                    max_stderr_bytes: limits.max_stderr_bytes_per_process,
                },
            )
            .await;
            from_adapter(
                format!("python:{name}:{callable}"),
                remap_adapter_outputs(run, dataset, &variant_cases)?,
            )
        }
        VariantConfig::OpenaiCompatible(adapter) => {
            let output_schema = model_facing_schema.cloned();
            if let Some(schema) = &output_schema {
                compile_schema(schema)?;
            }
            let run = run_openai_compatible(
                adapter,
                &variant_cases,
                output_schema.as_ref(),
                limits.max_output_bytes_per_case,
            )
            .await;
            from_adapter(
                format!("openai_compatible:{name}:{}", adapter.model),
                remap_adapter_outputs(run, dataset, &variant_cases)?,
            )
        }
    }
}

fn variant_cases_for_run(dataset: &Dataset, run_nonce: &str) -> anyhow::Result<Vec<VariantCase>> {
    dataset
        .cases
        .iter()
        .enumerate()
        .map(|(ordinal, case)| {
            Ok(VariantCase::for_execution(
                case,
                ExecutionToken::new(run_nonce, ordinal),
            ))
        })
        .collect()
}

fn remap_adapter_outputs(
    mut run: AdapterRun,
    dataset: &Dataset,
    variant_cases: &[VariantCase],
) -> anyhow::Result<AdapterRun> {
    anyhow::ensure!(
        run.rows.len() == dataset.cases.len(),
        "adapter returned {} rows for {} dataset cases",
        run.rows.len(),
        dataset.cases.len()
    );
    for (ordinal, ((row, case), variant_case)) in run
        .rows
        .iter_mut()
        .zip(&dataset.cases)
        .zip(variant_cases)
        .enumerate()
    {
        let expected_token = &variant_case.id;
        anyhow::ensure!(
            &row.case_id == expected_token,
            "adapter row {} returned an unexpected opaque execution token",
            ordinal + 1
        );
        row.case_id.clone_from(&case.id);
    }
    Ok(run)
}

fn from_adapter(source_label: String, run: AdapterRun) -> anyhow::Result<PreparedVariant> {
    let source_bytes = outputs_jsonl(&run.rows)?;
    Ok(PreparedVariant {
        source_label,
        input_hash: hash_bytes(&source_bytes),
        source_bytes,
        rows: run.rows,
        stderr: run.stderr,
        protocol_errors: run.protocol_errors,
    })
}

fn materialize_python_bridge(project_root: &Path, config: &Config) -> anyhow::Result<PathBuf> {
    let bridge_path = resolve(project_root, &config.storage.root)
        .join("runtime")
        .join("python-bridge-v3.py");
    let parent = bridge_path.parent().context("bridge path has no parent")?;
    std::fs::create_dir_all(parent)?;
    if let Ok(metadata) = std::fs::symlink_metadata(&bridge_path) {
        anyhow::ensure!(
            !metadata.file_type().is_symlink(),
            "refusing symlinked Python bridge {}",
            bridge_path.display()
        );
    }
    if structtrace_core::hashing::read_bounded(
        &bridge_path,
        BRIDGE_SOURCE.len() + 1,
        "Python bridge",
    )
    .ok()
    .as_deref()
        != Some(BRIDGE_SOURCE.as_bytes())
    {
        std::fs::write(&bridge_path, BRIDGE_SOURCE)?;
    }
    Ok(bridge_path)
}

fn resolve(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_owned()
    } else {
        root.join(path)
    }
}

fn verify_resume_compatibility(
    expected: &ExecutionCheckpoint,
    actual: &ExecutionCheckpoint,
) -> anyhow::Result<()> {
    let expected_model_schemas = hash_canonical_json(&expected.model_facing_schema_hashes)?;
    let actual_model_schemas = hash_canonical_json(&actual.model_facing_schema_hashes)?;
    let checks = [
        (
            "artifact format version",
            expected.artifact_format_version.to_string(),
            actual.artifact_format_version.to_string(),
        ),
        (
            "configuration file hash",
            expected.configuration_file_hash.clone(),
            actual.configuration_file_hash.clone(),
        ),
        (
            "normalized configuration hash",
            expected.normalized_configuration_hash.clone(),
            actual.normalized_configuration_hash.clone(),
        ),
        (
            "dataset hash",
            expected.dataset_hash.clone(),
            actual.dataset_hash.clone(),
        ),
        (
            "schema hash",
            expected.schema_hash.clone(),
            actual.schema_hash.clone(),
        ),
        (
            "execution definition hash",
            expected.execution_definition_hash.clone(),
            actual.execution_definition_hash.clone(),
        ),
        (
            "implementation fingerprint",
            expected.implementation_fingerprint.clone(),
            actual.implementation_fingerprint.clone(),
        ),
        (
            "model-facing schema hashes",
            expected_model_schemas,
            actual_model_schemas,
        ),
    ];
    let changed = checks
        .into_iter()
        .filter_map(|(name, expected, actual)| (expected != actual).then_some(name))
        .collect::<Vec<_>>();
    anyhow::ensure!(
        changed.is_empty(),
        "resume refused because these bound inputs changed: {}",
        changed.join(", ")
    );
    Ok(())
}

fn write_checkpoint(run_dir: &Path, checkpoint: &ExecutionCheckpoint) -> anyhow::Result<()> {
    let mut bytes = serde_json::to_vec_pretty(checkpoint)?;
    bytes.push(b'\n');
    atomic_write(run_dir.join("execution-checkpoint.json"), &bytes)
}

fn atomic_write(path: PathBuf, bytes: &[u8]) -> anyhow::Result<()> {
    let parent = path.parent().context("atomic output path has no parent")?;
    std::fs::create_dir_all(parent)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file_mut().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

fn read_optional(path: &Path) -> anyhow::Result<Vec<u8>> {
    if path.is_file() {
        Ok(structtrace_core::hashing::read_bounded(
            path,
            64 * 1024 * 1024,
            "execution checkpoint",
        )?)
    } else {
        Ok(Vec::new())
    }
}

fn read_optional_json(path: &Path) -> anyhow::Result<Vec<String>> {
    if path.is_file() {
        Ok(structtrace_core::hashing::read_json_bounded(
            path,
            64 * 1024 * 1024,
            "execution checkpoint",
        )?)
    } else {
        Ok(Vec::new())
    }
}

fn validate_run_id(run_id: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        !run_id.is_empty()
            && run_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'),
        "invalid run ID"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tempfile::tempdir;

    use structtrace_core::{
        config::{CommandSpec, ProcessMode},
        dataset::{Case, Dataset},
        hashing::hash_bytes,
    };

    use super::*;

    fn checkpoint() -> ExecutionCheckpoint {
        ExecutionCheckpoint {
            artifact_format_version: ARTIFACT_FORMAT_VERSION,
            run_id: "01TEST".to_owned(),
            configuration_file_hash: "config".to_owned(),
            normalized_configuration_hash: "normalized".to_owned(),
            dataset_hash: "dataset".to_owned(),
            schema_hash: "schema".to_owned(),
            execution_definition_hash: "definition".to_owned(),
            implementation_fingerprint: "implementation".to_owned(),
            model_facing_schema_hashes: BTreeMap::new(),
            completed_outputs: BTreeMap::new(),
            original_input_hashes: BTreeMap::new(),
            source_labels: BTreeMap::new(),
        }
    }

    #[test]
    fn resume_refuses_any_bound_input_change() {
        let expected = checkpoint();
        let mut actual = expected.clone();
        actual.dataset_hash = "changed".to_owned();
        let error = verify_resume_compatibility(&expected, &actual).unwrap_err();
        assert!(error.to_string().contains("dataset hash"));
    }

    #[test]
    fn resume_refuses_changed_implementation_fingerprint() {
        let expected = checkpoint();
        let mut actual = expected.clone();
        actual.implementation_fingerprint = "changed-code".to_owned();
        let error = verify_resume_compatibility(&expected, &actual).unwrap_err();
        assert!(error.to_string().contains("implementation fingerprint"));
    }

    #[test]
    fn changing_model_facing_schema_blocks_resume() {
        let expected = checkpoint();
        let mut actual = expected.clone();
        actual
            .model_facing_schema_hashes
            .insert("candidate".to_owned(), "changed".to_owned());
        let error = verify_resume_compatibility(&expected, &actual).unwrap_err();
        assert!(error.to_string().contains("model-facing schema hashes"));
    }

    #[test]
    fn recorded_run_does_not_scan_unrelated_python_tree() {
        let root = tempdir().unwrap();
        let config: Config = serde_json::from_value(json!({
            "version": 3,
            "project": {"name": "recorded-only"},
            "dataset": {"path": "data.jsonl"},
            "schema": {"path": "schema.json"},
            "variants": {
                "baseline": {"kind": "recorded", "path": "baseline.jsonl"},
                "candidate": {"kind": "recorded", "path": "candidate.jsonl"}
            },
            "evaluators": [{"id": "exact", "kind": "exact_json"}],
            "outcomes": {"correct": {"all_of": ["exact"]}},
            "analysis": {"primary_outcome": "correct"}
        }))
        .unwrap();
        let before = implementation_fingerprint(root.path(), &config).unwrap();
        std::fs::write(
            root.path().join("unrelated.py"),
            "raise RuntimeError('unused')",
        )
        .unwrap();
        let after = implementation_fingerprint(root.path(), &config).unwrap();
        assert_eq!(before, after);
    }

    fn openai_schema_config() -> Config {
        serde_json::from_value(json!({
            "version": 3,
            "project": {"name": "schema-snapshot"},
            "dataset": {"path": "data.jsonl"},
            "schema": {"path": "external.json"},
            "variants": {
                "baseline": {"kind": "recorded", "path": "baseline.jsonl"},
                "candidate": {
                    "kind": "openai_compatible",
                    "base_url": "http://127.0.0.1:8000/v1",
                    "model": "local-model",
                    "request": {"user_template": "{{ input }}"},
                    "structured_output": {"mode": "json_schema", "schema": "model.json"}
                }
            },
            "evaluators": [{"id": "exact", "kind": "exact_json"}],
            "outcomes": {"correct": {"all_of": ["exact"]}},
            "analysis": {"primary_outcome": "correct"}
        }))
        .unwrap()
    }

    #[test]
    fn model_facing_schema_is_captured_exactly_and_size_bounded() {
        let root = tempdir().unwrap();
        let external = br#"{"type":"object"}"#;
        let model = br#"{"type":"object","required":["answer"]}"#;
        std::fs::write(root.path().join("model.json"), model).unwrap();
        let config = openai_schema_config();
        let captured = capture_model_facing_schemas(
            root.path(),
            &config,
            external,
            &serde_json::from_slice(external).unwrap(),
        )
        .unwrap();
        assert_eq!(captured["candidate"].bytes, model);

        let mut bounded = config;
        bounded.limits.max_schema_bytes = 8;
        assert!(
            capture_model_facing_schemas(
                root.path(),
                &bounded,
                external,
                &serde_json::from_slice(external).unwrap(),
            )
            .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn model_facing_schema_symlink_is_rejected() {
        use std::os::unix::fs::symlink;
        let root = tempdir().unwrap();
        std::fs::write(root.path().join("real.json"), "{\"type\":\"object\"}").unwrap();
        symlink(
            root.path().join("real.json"),
            root.path().join("model.json"),
        )
        .unwrap();
        let external = br#"{"type":"object"}"#;
        assert!(
            capture_model_facing_schemas(
                root.path(),
                &openai_schema_config(),
                external,
                &serde_json::from_slice(external).unwrap(),
            )
            .is_err()
        );
    }

    #[test]
    fn opaque_adapter_tokens_are_run_scoped_unique_and_map_back() {
        let case = Case {
            id: "gold-positive-001".to_owned(),
            input: json!({"text": "hello"}),
            expected: Some(json!({"label": "positive"})),
            model_visible_metadata: None,
            metadata: None,
            source_line: 1,
        };
        let mut repeated = case.clone();
        repeated.id = "gold-positive-002".to_owned();
        let dataset = Dataset {
            cases: vec![case.clone(), repeated],
            source_hash: String::new(),
            source_bytes: Vec::new(),
        };
        let variant_cases = variant_cases_for_run(&dataset, "run-a").unwrap();
        let token = variant_cases[0].id.clone();
        assert_ne!(token, case.id);
        assert!(!token.contains("positive"));
        assert_ne!(variant_cases[0].id, variant_cases[1].id);
        assert_ne!(
            variant_cases[0].id,
            variant_cases_for_run(&dataset, "run-b").unwrap()[0].id
        );
        let run = AdapterRun {
            rows: variant_cases
                .iter()
                .map(|variant_case| structtrace_core::output::VariantOutput {
                    case_id: variant_case.id.clone(),
                    status: structtrace_core::output::OutputStatus::Ok,
                    raw_output: Some("{}".to_owned()),
                    parsed_output: None,
                    error: None,
                    latency_ms: None,
                    usage: None,
                    cost: None,
                    metadata: Value::Null,
                    retries: Vec::new(),
                })
                .collect(),
            stderr: Vec::new(),
            protocol_errors: Vec::new(),
        };
        let remapped = remap_adapter_outputs(run, &dataset, &variant_cases).unwrap();
        assert_eq!(remapped.rows[0].case_id, case.id);
        assert_eq!(remapped.rows[1].case_id, "gold-positive-002");
    }

    #[tokio::test]
    async fn allocated_run_is_marked_failed_when_preparation_returns_an_error() {
        let root = tempdir().unwrap();
        std::fs::write(
            root.path().join("data.jsonl"),
            "{\"id\":\"one\",\"input\":{},\"expected\":{}}\n",
        )
        .unwrap();
        std::fs::write(root.path().join("schema.json"), "{\"type\":\"object\"}").unwrap();
        std::fs::write(
            root.path().join("structtrace.yaml"),
            r#"version: 3
project: {name: lifecycle-test}
dataset: {path: data.jsonl}
schema: {path: schema.json}
variants:
  baseline: {kind: recorded, path: missing-baseline.jsonl}
  candidate: {kind: recorded, path: missing-candidate.jsonl}
evaluators:
  - {id: exact, kind: exact_json}
outcomes:
  correct: {all_of: [exact]}
analysis: {primary_outcome: correct}
"#,
        )
        .unwrap();
        assert!(
            run_configured(root.path(), Path::new("structtrace.yaml"))
                .await
                .is_err()
        );
        let run_dir = std::fs::read_dir(root.path().join(".structtrace/runs"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let run_id = run_dir.file_name().unwrap().to_str().unwrap();
        let store = RunStore::open(&run_dir).unwrap();
        assert_eq!(store.status(run_id).unwrap(), RunStatus::Failed);
    }

    #[tokio::test]
    async fn source_file_change_during_run_does_not_change_retained_input() {
        let python = ["python3", "python"]
            .into_iter()
            .find(|program| {
                std::process::Command::new(program)
                    .arg("--version")
                    .output()
                    .is_ok_and(|output| output.status.success())
            })
            .expect("Python is required for configured adapter tests");
        let root = tempdir().unwrap();
        let original = "{\"id\":\"one\",\"input\":{},\"expected\":{\"label\":\"yes\"}}\n";
        std::fs::write(root.path().join("data.jsonl"), original).unwrap();
        std::fs::write(root.path().join("schema.json"), "{\"type\":\"object\"}").unwrap();
        std::fs::write(
            root.path().join("variant.py"),
            "import json, pathlib, sys\nfor line in sys.stdin:\n request=json.loads(line)\n pathlib.Path('data.jsonl').write_text('{\"id\":\"tampered\",\"input\":{}}\\n')\n print(json.dumps({'protocol':'structtrace.variant','protocol_version': 3,'case_id':request['case_id'],'status':'ok','output':{'label':'yes'}}), flush=True)\n",
        )
        .unwrap();
        std::fs::write(
            root.path().join("structtrace.yaml"),
            format!(
                r#"version: 3
project: {{name: immutable-source}}
dataset: {{path: data.jsonl}}
schema: {{path: schema.json}}
variants:
  baseline:
    kind: command
    command: {{program: "{python}", args: ["variant.py"]}}
  candidate:
    kind: command
    command: {{program: "{python}", args: ["variant.py"]}}
evaluators:
  - {{id: exact, kind: exact_json}}
outcomes:
  correct: {{all_of: [exact]}}
analysis: {{primary_outcome: correct}}
"#
            ),
        )
        .unwrap();
        let run = run_configured(root.path(), Path::new("structtrace.yaml"))
            .await
            .unwrap();
        assert_ne!(
            std::fs::read_to_string(root.path().join("data.jsonl")).unwrap(),
            original
        );
        assert_eq!(
            std::fs::read_to_string(run.run_dir.join("inputs/dataset.jsonl")).unwrap(),
            original
        );
        assert!(crate::replay::replay_run(&run.run_dir).unwrap().verified);
    }

    #[tokio::test]
    async fn completed_variant_is_restored_without_reinvocation() {
        let root = tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("inputs")).unwrap();
        let bytes = b"{\"case_id\":\"one\",\"status\":\"ok\",\"raw_output\":\"{\\\"label\\\":\\\"yes\\\"}\",\"metadata\":{},\"retries\":[]}\n";
        std::fs::write(root.path().join("inputs/baseline.jsonl"), bytes).unwrap();
        let case = Case {
            id: "one".to_owned(),
            input: json!({}),
            expected: Some(json!({"label": "yes"})),
            model_visible_metadata: None,
            metadata: None,
            source_line: 1,
        };
        let dataset = Dataset {
            cases: vec![case],
            source_hash: "dataset".to_owned(),
            source_bytes: b"dataset".to_vec(),
        };
        let mut checkpoint = checkpoint();
        checkpoint
            .completed_outputs
            .insert("baseline".to_owned(), hash_bytes(bytes));
        checkpoint
            .original_input_hashes
            .insert("baseline".to_owned(), "original".to_owned());
        checkpoint
            .source_labels
            .insert("baseline".to_owned(), "command:baseline:test".to_owned());
        let impossible = VariantConfig::Command {
            command: CommandSpec {
                program: "this-program-must-not-exist".to_owned(),
                args: vec![],
            },
            process_mode: ProcessMode::Persistent,
            timeout_ms: 10,
            implementation: Default::default(),
        };
        let restored = prepare_or_restore(
            root.path(),
            "baseline",
            &impossible,
            &dataset,
            None,
            Path::new("unused.py"),
            &structtrace_core::config::LimitsConfig::default(),
            root.path(),
            &mut checkpoint,
        )
        .await
        .unwrap();
        assert_eq!(restored.rows.len(), 1);
        assert_eq!(restored.rows[0].case_id, "one");
        assert_eq!(restored.input_hash, "original");
    }
}

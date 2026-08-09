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
    artifact::RunStatus,
    config::{Config, VariantConfig},
    dataset::{Dataset, VariantCase},
    evaluation::compile_schema,
    hashing::{hash_bytes, hash_canonical_json, hash_file},
    output::RecordedOutputs,
};
use tempfile::NamedTempFile;
use ulid::Ulid;

use crate::{
    recorded::{
        CompletedRun, PreparedVariant, apply_storage_retention, finalize_prepared_for_run,
        outputs_jsonl,
    },
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
    completed_outputs: BTreeMap<String, String>,
    original_input_hashes: BTreeMap<String, String>,
    source_labels: BTreeMap<String, String>,
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
    let schema_path = resolve(&project_root, &config.schema.path);
    let schema_bytes = structtrace_core::hashing::read_bounded(
        &schema_path,
        config.limits.max_schema_bytes,
        "schema",
    )?;
    let schema_value: Value = serde_json::from_slice(&schema_bytes)
        .with_context(|| format!("schema {} is not valid JSON", schema_path.display()))?;
    compile_schema(&schema_value)?;

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
    let implementation_fingerprint = implementation_fingerprint(&project_root, &config)?;
    let expected = ExecutionCheckpoint {
        artifact_format_version: ARTIFACT_FORMAT_VERSION,
        run_id: resume_run_id.clone().unwrap_or_default(),
        configuration_file_hash: hash_bytes(&config_source_bytes),
        normalized_configuration_hash: hash_canonical_json(&config)?,
        dataset_hash: dataset.source_hash.clone(),
        schema_hash: hash_bytes(&schema_bytes),
        execution_definition_hash: definition_hash,
        implementation_fingerprint: implementation_fingerprint.clone(),
        completed_outputs: BTreeMap::new(),
        original_input_hashes: BTreeMap::new(),
        source_labels: BTreeMap::new(),
    };
    let (run_id, store, mut checkpoint) =
        match resume_run_id {
            Some(run_id) => {
                let run_dir = storage_root.join("runs").join(&run_id);
                let checkpoint_path = run_dir.join("execution-checkpoint.json");
                let bytes = std::fs::read(&checkpoint_path).with_context(|| {
                    format!(
                        "run `{run_id}` has no resumable execution checkpoint at {}",
                        checkpoint_path.display()
                    )
                })?;
                let checkpoint: ExecutionCheckpoint = serde_json::from_slice(&bytes)
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
                let store = RunStore::create(&storage_root, &run_id)?;
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
        &schema_value,
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
        &schema_value,
        &bridge_path,
        &config.limits,
        store.run_dir(),
        &mut checkpoint,
    )
    .await?;
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
        Some(implementation_fingerprint),
    )?;
    let checkpoint_path = completed.run_dir.join("execution-checkpoint.json");
    if checkpoint_path.is_file() {
        std::fs::remove_file(checkpoint_path)?;
    }
    Ok(completed)
}

fn implementation_fingerprint(project_root: &Path, config: &Config) -> anyhow::Result<String> {
    let mut sources = BTreeMap::<String, String>::new();
    collect_project_python_sources(project_root, project_root, &mut sources)?;
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
            sources.insert(lockfile.to_owned(), hash_file(&path)?);
        }
    }
    for (variant_id, variant) in &config.variants {
        match variant {
            VariantConfig::Command { command, .. } => {
                let path = resolve(project_root, Path::new(&command.program));
                if path.is_file() {
                    sources.insert(format!("variant:{variant_id}:program"), hash_file(&path)?);
                }
            }
            VariantConfig::Python { callable, .. } => {
                if let Some((module, _)) = callable.split_once(':') {
                    let path = project_root.join(format!("{}.py", module.replace('.', "/")));
                    if path.is_file() {
                        sources.insert(format!("variant:{variant_id}:python"), hash_file(&path)?);
                    }
                }
            }
            VariantConfig::Recorded { .. } | VariantConfig::OpenaiCompatible(_) => {}
        }
    }
    for evaluator in &config.evaluators {
        match &evaluator.kind {
            structtrace_core::config::EvaluatorKind::Command { command, .. } => {
                let path = resolve(project_root, Path::new(&command.program));
                if path.is_file() {
                    sources.insert(
                        format!("evaluator:{}:program", evaluator.id),
                        hash_file(&path)?,
                    );
                }
            }
            structtrace_core::config::EvaluatorKind::Python { callable, .. } => {
                if let Some((module, _)) = callable.split_once(':') {
                    let path = project_root.join(format!("{}.py", module.replace('.', "/")));
                    if path.is_file() {
                        sources.insert(
                            format!("evaluator:{}:python", evaluator.id),
                            hash_file(&path)?,
                        );
                    }
                }
            }
            _ => {}
        }
    }
    let git_commit = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(project_root)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned());
    let git_dirty_hash = std::process::Command::new("git")
        .args(["diff", "--binary", "HEAD"])
        .current_dir(project_root)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| hash_bytes(&output.stdout));
    let git_status_hash = std::process::Command::new("git")
        .args(["status", "--porcelain=v1", "--untracked-files=all"])
        .current_dir(project_root)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| hash_bytes(&output.stdout));
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
            (interpreter.clone(), version)
        })
        .collect::<BTreeMap<_, _>>();
    hash_canonical_json(&serde_json::json!({
        "sources": sources,
        "git_commit": git_commit,
        "git_dirty_hash": git_dirty_hash,
        "git_status_hash": git_status_hash,
        "interpreters": interpreters,
        "binary": env!("CARGO_PKG_VERSION"),
    }))
    .map_err(Into::into)
}

fn collect_project_python_sources(
    root: &Path,
    directory: &Path,
    output: &mut BTreeMap<String, String>,
) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            anyhow::ensure!(
                path.extension().and_then(|value| value.to_str()) != Some("py"),
                "Python implementation source must not be a symlink: {}",
                path.display()
            );
            continue;
        }
        if file_type.is_dir() {
            if matches!(
                name.to_str(),
                Some(
                    ".git"
                        | ".structtrace"
                        | ".venv"
                        | ".tox"
                        | "target"
                        | "node_modules"
                        | "dist"
                        | "build"
                        | "__pycache__"
                )
            ) {
                continue;
            }
            collect_project_python_sources(root, &path, output)?;
        } else if path.extension().and_then(|value| value.to_str()) == Some("py") {
            let metadata = entry.metadata()?;
            anyhow::ensure!(
                metadata.len() <= 16 * 1024 * 1024,
                "Python source is too large to fingerprint: {}",
                path.display()
            );
            let relative = path
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            output.insert(format!("python-tree:{relative}"), hash_file(&path)?);
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn prepare_or_restore(
    project_root: &Path,
    name: &str,
    variant: &VariantConfig,
    dataset: &Dataset,
    external_schema: &Value,
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
        let outputs = RecordedOutputs::read(&output_path, dataset)?;
        return Ok(PreparedVariant {
            source_label: checkpoint
                .source_labels
                .get(name)
                .cloned()
                .unwrap_or_else(|| format!("checkpoint:{name}")),
            source_bytes: std::fs::read(&output_path)?,
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
    let mut prepared = prepare_variant(
        project_root,
        name,
        variant,
        dataset,
        external_schema,
        bridge_path,
        limits,
    )
    .await?;
    let retained_config: Config =
        serde_json::from_slice(&std::fs::read(run_dir.join("inputs/configuration.json"))?)?;
    apply_storage_retention(
        &mut prepared,
        retained_config.storage.retain_raw_outputs,
        retained_config.storage.retain_provider_responses,
        retained_config.report.include_prompts,
    )?;
    atomic_write(output_path, &prepared.source_bytes)?;
    if !prepared.stderr.is_empty() {
        atomic_write(
            run_dir.join("logs").join(format!("{name}.stderr.log")),
            &prepared.stderr,
        )?;
    }
    if !prepared.protocol_errors.is_empty() {
        atomic_write(
            run_dir
                .join("logs")
                .join(format!("{name}.protocol-errors.json")),
            &serde_json::to_vec_pretty(&prepared.protocol_errors)?,
        )?;
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

async fn prepare_variant(
    project_root: &Path,
    name: &str,
    variant: &VariantConfig,
    dataset: &Dataset,
    external_schema: &Value,
    bridge_path: &Path,
    limits: &structtrace_core::config::LimitsConfig,
) -> anyhow::Result<PreparedVariant> {
    let variant_cases = dataset
        .cases
        .iter()
        .map(VariantCase::from)
        .collect::<Vec<_>>();
    match variant {
        VariantConfig::Recorded { path } => {
            let path = resolve(project_root, path);
            let source_bytes = std::fs::read(&path)
                .with_context(|| format!("could not read {} output {}", name, path.display()))?;
            let outputs = RecordedOutputs::read(&path, dataset)?;
            Ok(PreparedVariant {
                source_label: path.display().to_string(),
                source_bytes,
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
            from_adapter(format!("command:{name}:{}", command.program), run)
        }
        VariantConfig::Python {
            interpreter,
            callable,
            timeout_ms,
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
            from_adapter(format!("python:{name}:{callable}"), run)
        }
        VariantConfig::OpenaiCompatible(adapter) => {
            let output_schema = match adapter
                .structured_output
                .as_ref()
                .and_then(|structured| structured.schema.as_ref())
            {
                Some(path) => {
                    let path = resolve(project_root, path);
                    let bytes = std::fs::read(&path).with_context(|| {
                        format!("could not read structured-output schema {}", path.display())
                    })?;
                    Some(serde_json::from_slice::<Value>(&bytes).with_context(|| {
                        format!(
                            "structured-output schema {} is invalid JSON",
                            path.display()
                        )
                    })?)
                }
                None if adapter.structured_output.is_some() => Some(external_schema.clone()),
                None => None,
            };
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
            from_adapter(format!("openai_compatible:{name}:{}", adapter.model), run)
        }
    }
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
        .join("python-bridge-v1.py");
    let parent = bridge_path.parent().context("bridge path has no parent")?;
    std::fs::create_dir_all(parent)?;
    if std::fs::read(&bridge_path).ok().as_deref() != Some(BRIDGE_SOURCE.as_bytes()) {
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
        Ok(std::fs::read(path)?)
    } else {
        Ok(Vec::new())
    }
}

fn read_optional_json(path: &Path) -> anyhow::Result<Vec<String>> {
    if path.is_file() {
        Ok(serde_json::from_slice(&std::fs::read(path)?)?)
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
            r#"version: 1
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
            "import json, pathlib, sys\nfor line in sys.stdin:\n request=json.loads(line)\n pathlib.Path('data.jsonl').write_text('{\"id\":\"tampered\",\"input\":{}}\\n')\n print(json.dumps({'protocol':'structtrace.variant','protocol_version':1,'case_id':request['case_id'],'status':'ok','output':{'label':'yes'}}), flush=True)\n",
        )
        .unwrap();
        std::fs::write(
            root.path().join("structtrace.yaml"),
            format!(
                r#"version: 1
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
        };
        let restored = prepare_or_restore(
            root.path(),
            "baseline",
            &impossible,
            &dataset,
            &json!({"type": "object"}),
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

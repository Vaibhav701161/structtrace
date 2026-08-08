//! Versioned portable run artifacts.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    ARTIFACT_FORMAT_VERSION,
    config::{BootstrapConfig, GateConfig},
    dataset::Case,
    evaluation::CaseEvaluation,
    gate::GateDecision,
    output::VariantOutput,
    statistics::{BootstrapInterval, PairedMetrics},
};

/// One portable case-level paired record used by reports and replay.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PairedCaseRecord {
    /// Immutable case envelope.
    pub case: Case,
    /// Original baseline adapter envelope.
    pub baseline_output: VariantOutput,
    /// Original candidate adapter envelope.
    pub candidate_output: VariantOutput,
    /// Recomputed baseline scores.
    pub baseline_evaluation: CaseEvaluation,
    /// Recomputed candidate scores.
    pub candidate_evaluation: CaseEvaluation,
    /// Primary paired category.
    pub transition: String,
}

/// Explicit lifecycle state retained in the manifest and database.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// Directory and database allocated.
    Created,
    /// Inputs are being checked.
    Validating,
    /// Variants are executing or outputs are being imported.
    Running,
    /// Execution stopped before completion.
    Interrupted,
    /// Scores and summaries are being computed.
    Analyzing,
    /// All artifacts finalized successfully.
    Complete,
    /// Run failed and is not resumable as-is.
    Failed,
    /// Stored integrity checks failed.
    Corrupt,
}

/// Reproducibility and artifact-integrity manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunManifest {
    /// StructTrace package version.
    pub structtrace_version: String,
    /// Portable artifact schema version.
    pub artifact_format_version: u32,
    /// ULID run identity.
    pub run_id: String,
    /// Project name.
    pub project_name: String,
    /// Exact source configuration hash.
    pub configuration_file_hash: String,
    /// Canonical resolved configuration hash with no secret values.
    pub normalized_configuration_hash: String,
    /// Dataset source path as configured.
    pub dataset_path: String,
    /// Exact dataset bytes hash.
    pub dataset_hash: String,
    /// Schema source path as configured.
    pub schema_path: String,
    /// Exact schema bytes hash.
    pub schema_hash: String,
    /// Redacted variant definitions.
    pub variants: Value,
    /// Evaluators and outcomes.
    pub evaluation_definition: Value,
    /// Gate definition.
    pub gate: GateConfig,
    /// Bootstrap settings.
    pub bootstrap: BootstrapConfig,
    /// Compilation target architecture and OS.
    pub binary_target: String,
    /// Environment variable names and presence only.
    pub environment: BTreeMap<String, bool>,
    /// Hashes of finalized artifacts.
    pub artifacts: BTreeMap<String, String>,
    /// Exact hashes of imported baseline and candidate artifacts.
    pub input_artifacts: BTreeMap<String, String>,
    /// Run creation time as Unix milliseconds.
    pub started_at_unix_ms: u128,
    /// Run completion time as Unix milliseconds.
    pub completed_at_unix_ms: Option<u128>,
    /// Current lifecycle state.
    pub status: RunStatus,
}

impl RunManifest {
    /// Construct the invariant artifact header.
    pub fn new(run_id: String, project_name: String) -> Self {
        Self {
            structtrace_version: env!("CARGO_PKG_VERSION").to_owned(),
            artifact_format_version: ARTIFACT_FORMAT_VERSION,
            run_id,
            project_name,
            configuration_file_hash: String::new(),
            normalized_configuration_hash: String::new(),
            dataset_path: String::new(),
            dataset_hash: String::new(),
            schema_path: String::new(),
            schema_hash: String::new(),
            variants: Value::Null,
            evaluation_definition: Value::Null,
            gate: GateConfig::default(),
            bootstrap: BootstrapConfig::default(),
            binary_target: format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS),
            environment: BTreeMap::new(),
            artifacts: BTreeMap::new(),
            input_artifacts: BTreeMap::new(),
            started_at_unix_ms: unix_millis(),
            completed_at_unix_ms: None,
            status: RunStatus::Created,
        }
    }
}

fn unix_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

/// Aggregated structural results for one variant.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct VariantSummary {
    /// Complete denominator.
    pub total: usize,
    /// Strict JSON successes.
    pub parse_valid: usize,
    /// External-schema successes.
    pub schema_valid: usize,
    /// Primary semantic or executable successes.
    pub primary_pass: usize,
    /// Structurally valid but primary-outcome failures.
    pub valid_but_wrong: usize,
    /// Adapter errors, including missing outputs.
    pub errors: usize,
    /// Explicit timeout errors.
    pub timeouts: usize,
    /// Operational measurements reported by adapters.
    pub operational: OperationalSummary,
}

/// Latency, retry, usage, and user-priced cost measurements for one variant.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct OperationalSummary {
    /// Rows with latency measurements.
    pub latency_observations: usize,
    /// Mean end-to-end latency in milliseconds.
    pub mean_latency_ms: Option<f64>,
    /// Median end-to-end latency in milliseconds.
    pub median_latency_ms: Option<f64>,
    /// Nearest-rank p95 latency in milliseconds.
    pub p95_latency_ms: Option<f64>,
    /// Total explicitly retained retry attempts.
    pub retry_attempts: usize,
    /// Rows with token-usage measurements.
    pub usage_observations: usize,
    /// Sum of retained input tokens.
    pub input_tokens: u64,
    /// Sum of retained output tokens.
    pub output_tokens: u64,
    /// Rows with a user-priced cost.
    pub cost_observations: usize,
    /// Exact decimal total cost when all observed costs share one currency.
    pub total_cost: Option<String>,
    /// Exact decimal average cost when all observed costs share one currency.
    pub average_cost: Option<String>,
    /// User-declared currency, when unambiguous.
    pub currency: Option<String>,
    /// True when cost rows used more than one currency and were not aggregated.
    pub mixed_currencies: bool,
}

/// Portable summary generated from complete-denominator case artifacts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunSummary {
    /// Artifact format version.
    pub artifact_format_version: u32,
    /// Run ID.
    pub run_id: String,
    /// Primary outcome name.
    pub primary_outcome: String,
    /// Baseline aggregate.
    pub baseline: VariantSummary,
    /// Candidate aggregate.
    pub candidate: VariantSummary,
    /// Paired primary transition matrix and effect.
    pub paired: PairedMetrics,
    /// Seeded paired bootstrap interval.
    pub bootstrap: BootstrapInterval,
    /// Release-gate decision.
    pub gate: GateDecision,
    /// Per-evaluator pass counts by variant.
    pub evaluator_passes: BTreeMap<String, EvaluatorComparison>,
    /// Field-level regression and improvement counts.
    pub field_hotspots: Vec<FieldHotspot>,
}

/// Per-evaluator aggregate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvaluatorComparison {
    /// Complete denominator.
    pub total: usize,
    /// Baseline passes.
    pub baseline_pass: usize,
    /// Candidate passes.
    pub candidate_pass: usize,
}

/// JSON Pointer-level paired changes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FieldHotspot {
    /// Output JSON Pointer.
    pub pointer: String,
    /// Baseline pass and candidate fail.
    pub regressions: usize,
    /// Baseline fail and candidate pass.
    pub improvements: usize,
    /// Total candidate failures.
    pub candidate_failures: usize,
}

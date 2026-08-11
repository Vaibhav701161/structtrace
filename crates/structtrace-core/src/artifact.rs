//! Versioned portable run artifacts.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    ARTIFACT_FORMAT_VERSION,
    config::{BootstrapConfig, GateConfig, SchemaProvenance},
    dataset::Case,
    evaluation::CaseEvaluation,
    evaluation::EvaluatorResult,
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
    /// Paired category based on complete deployment success.
    #[serde(alias = "transition")]
    pub deployment_transition: PairedTransition,
    /// Paired category based on semantic truth when both outcomes fully resolved.
    #[serde(default)]
    pub semantic_transition: Option<PairedTransition>,
}

/// Typed paired transition retained in case evidence and verified by replay.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PairedTransition {
    /// Both variants passed the selected criterion.
    BothPass,
    /// Baseline passed and candidate failed.
    BaselineOnlyPass,
    /// Candidate passed and baseline failed.
    CandidateOnlyPass,
    /// Neither variant passed.
    BothFail,
}

impl PairedTransition {
    /// Build a transition from paired binary facts.
    #[must_use]
    pub const fn from_bools(baseline: bool, candidate: bool) -> Self {
        match (baseline, candidate) {
            (true, true) => Self::BothPass,
            (true, false) => Self::BaselineOnlyPass,
            (false, true) => Self::CandidateOnlyPass,
            (false, false) => Self::BothFail,
        }
    }

    /// Stable snake-case label used by reports and filters.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BothPass => "both_pass",
            Self::BaselineOnlyPass => "baseline_only_pass",
            Self::CandidateOnlyPass => "candidate_only_pass",
            Self::BothFail => "both_fail",
        }
    }
}

/// Hash-bound receipt for one deliberately non-reexecuted external evaluator.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExternalEvaluatorReceipt {
    /// Evaluator identity.
    pub evaluator_id: String,
    /// Matched case identity.
    pub case_id: String,
    /// Baseline or candidate identity.
    pub variant_id: String,
    /// Canonical hash of the exact evaluator request object.
    pub request_hash: String,
    /// Canonical hash of the parsed evaluator response fact.
    pub response_hash: String,
    /// Canonical hash of the configured executable or callable definition.
    pub definition_hash: String,
    /// Parsed response fact reused during replay.
    pub result: EvaluatorResult,
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

/// Purpose of a run, used to keep examples and verification fixtures out of production history.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunKind {
    /// A user-requested comparison eligible for the default `latest` selector.
    #[default]
    Production,
    /// A bundled product demonstration.
    Demo,
    /// A normalized research verification fixture with no pooled release claim.
    ResearchFixture,
    /// An internal automated test run.
    Test,
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
    /// Purpose of this run; only production runs participate in default `latest` resolution.
    #[serde(default)]
    pub run_kind: RunKind,
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
    /// Explicit authority of the schema used for structural validity.
    #[serde(default)]
    pub schema_provenance: SchemaProvenance,
    /// Redacted variant definitions.
    pub variants: Value,
    /// Evaluators and outcomes.
    pub evaluation_definition: Value,
    /// Gate definition.
    pub gate: GateConfig,
    /// Bootstrap settings.
    pub bootstrap: BootstrapConfig,
    /// Fixed variant execution order used by this artifact format.
    #[serde(default = "default_execution_schedule")]
    pub execution_schedule: String,
    /// Bound local source, interpreter, lockfile, Git, and dirty-tree fingerprint for live runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub implementation_fingerprint: Option<String>,
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
            run_kind: RunKind::Production,
            project_name,
            configuration_file_hash: String::new(),
            normalized_configuration_hash: String::new(),
            dataset_path: String::new(),
            dataset_hash: String::new(),
            schema_path: String::new(),
            schema_hash: String::new(),
            schema_provenance: SchemaProvenance::CallerSupplied,
            variants: Value::Null,
            evaluation_definition: Value::Null,
            gate: GateConfig::default(),
            bootstrap: BootstrapConfig::default(),
            execution_schedule: default_execution_schedule(),
            implementation_fingerprint: None,
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

fn default_execution_schedule() -> String {
    "blocked_baseline_then_candidate".to_owned()
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
    /// Adapter, strict parsing, and caller-facing schema successes.
    #[serde(default)]
    pub structured_success: usize,
    /// Explicit primary semantic successes, independent of structure.
    #[serde(default)]
    pub semantic_success: usize,
    /// Complete deployable successes: structural, semantic, and fully evaluated.
    #[serde(default)]
    pub deployment_success: usize,
    /// Primary outcomes that ran and explicitly failed.
    #[serde(default)]
    pub primary_failed: usize,
    /// Primary outcomes that could not be evaluated reliably.
    #[serde(default)]
    pub primary_error: usize,
    /// Primary outcomes explicitly marked not applicable.
    #[serde(default)]
    pub primary_not_applicable: usize,
    /// Rows without a named primary outcome result.
    #[serde(default)]
    pub primary_unscored: usize,
    /// Rows whose primary outcome has no errored, not-applicable, or missing component.
    #[serde(default)]
    pub primary_fully_evaluated: usize,
    /// Total required primary evaluator components across rows.
    #[serde(default)]
    pub primary_required_components: usize,
    /// Required primary evaluator components that errored.
    #[serde(default)]
    pub primary_component_errors: usize,
    /// Required primary evaluator components that were not applicable.
    #[serde(default)]
    pub primary_component_not_applicable: usize,
    /// Required primary evaluator components that were absent.
    #[serde(default)]
    pub primary_component_unscored: usize,
    /// Structurally valid outputs with an explicit false primary outcome.
    pub valid_but_wrong: usize,
    /// Valid-but-wrong rows for which every primary component was resolved.
    #[serde(default)]
    pub fully_evaluated_valid_but_wrong: usize,
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

/// Operational measurements observed on both members of the same pair.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct MatchedOperationalSummary {
    /// Complete paired denominator.
    pub total_pairs: usize,
    /// Pairs with latency on both variants.
    pub latency_pairs: usize,
    /// Baseline p95 over latency-matched pairs.
    pub baseline_p95_latency_ms: Option<f64>,
    /// Candidate p95 over latency-matched pairs.
    pub candidate_p95_latency_ms: Option<f64>,
    /// Pairs with comparable cost on both variants.
    pub cost_pairs: usize,
    /// Baseline average cost over cost-matched pairs.
    pub baseline_average_cost: Option<String>,
    /// Candidate average cost over cost-matched pairs.
    pub candidate_average_cost: Option<String>,
    /// Shared user-declared currency for matched costs.
    pub currency: Option<String>,
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
    /// Baseline aggregate over independent, non-conflicting evidence units.
    pub baseline: VariantSummary,
    /// Candidate aggregate over independent, non-conflicting evidence units.
    pub candidate: VariantSummary,
    /// Baseline aggregate over every captured row, with no independence claim.
    pub descriptive_baseline: VariantSummary,
    /// Candidate aggregate over every captured row, with no independence claim.
    pub descriptive_candidate: VariantSummary,
    /// Evidence units with an explicit binary primary outcome for both variants.
    #[serde(default)]
    pub primary_jointly_scored: usize,
    /// Dataset independence audit used by inferential statistics and gates.
    pub evidence: EvidenceSummary,
    /// Backward-compatible alias for the deployment-success effect over independent evidence units.
    pub independent_paired: PairedMetrics,
    /// Complete-denominator paired deployment effect.
    pub deployment_paired: PairedMetrics,
    /// Deployment-success bootstrap interval over independent evidence units.
    pub independent_bootstrap: Option<BootstrapInterval>,
    /// Paired semantic effect restricted to explicitly scored pass/fail pairs.
    pub jointly_scored_semantic: SemanticEffectSummary,
    /// Pair-matched operational measurements used by operational gates.
    #[serde(default)]
    pub matched_operational: MatchedOperationalSummary,
    /// Operational measurements over every captured row, for description only.
    pub descriptive_matched_operational: MatchedOperationalSummary,
    /// Backward-compatible alias for the complete-denominator deployment transition matrix.
    pub paired: PairedMetrics,
    /// Seeded deployment-success paired bootstrap interval.
    pub bootstrap: Option<BootstrapInterval>,
    /// Release-gate decision.
    pub gate: GateDecision,
    /// Per-evaluator pass counts by variant.
    pub evaluator_passes: BTreeMap<String, EvaluatorComparison>,
    /// Field-level regression and improvement counts.
    #[serde(default)]
    pub field_hotspots: Vec<FieldHotspot>,
    /// Field facts emitted by evaluators reachable from the selected primary outcome.
    #[serde(default)]
    pub primary_field_hotspots: Vec<FieldHotspot>,
    /// Field facts from every configured evaluator, explicitly diagnostic only.
    #[serde(default)]
    pub all_evaluator_field_diagnostics: Vec<FieldHotspot>,
}

/// Exact-duplicate audit for the matched dataset.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct EvidenceSummary {
    /// All matched rows, including exact duplicates and repeated trials.
    pub total_rows: usize,
    /// Evidence units represented by exactly one row.
    pub singleton_evidence_units: usize,
    /// Evidence groups whose complete scored and operational observations are identical.
    pub exact_duplicate_groups: usize,
    /// Evidence groups containing differing scored or operational observations.
    pub repeated_trial_groups: usize,
    /// Stimuli associated with incompatible expected references.
    pub label_conflict_groups: usize,
    /// Rows beyond the first member of exact-duplicate groups.
    pub exact_duplicate_rows: usize,
    /// Largest number of rows sharing one evidence-unit identity.
    pub largest_group: usize,
    /// Fraction of captured rows that are redundant exact duplicates.
    pub exact_duplicate_row_rate: f64,
    /// Denominator used by independent paired inference and gates.
    pub effective_inference_units: usize,
    /// Human-readable configured evidence-unit identity.
    pub inference_policy: String,
    /// Deterministic per-group classifications for audit and report diagnostics.
    pub groups: Vec<EvidenceGroupDiagnostic>,
}

/// Conservative v1 classification for one configured evidence group.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceGroupKind {
    /// One row represents the evidence unit.
    Singleton,
    /// Multiple rows have identical stimulus, reference, scored, and operational evidence.
    ExactDuplicate,
    /// Multiple rows disagree in scored or operational evidence and are not independently modeled.
    RepeatedTrial,
    /// One stimulus maps to incompatible expected references.
    LabelConflict,
}

/// Safe, case-ID-free evidence-group audit fact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceGroupDiagnostic {
    /// Canonical hash of the configured evidence-unit identity.
    pub evidence_unit_hash: String,
    /// Canonical stimulus hashes observed in the group.
    pub stimulus_hashes: Vec<String>,
    /// Canonical reference hashes observed in the group.
    pub reference_hashes: Vec<String>,
    /// Canonical retention-independent scored-observation hashes.
    pub scored_observation_hashes: Vec<String>,
    /// Canonical operational-observation hashes.
    pub operational_observation_hashes: Vec<String>,
    /// Conservative group classification.
    pub kind: EvidenceGroupKind,
    /// Number of captured rows in the group.
    pub rows: usize,
}

/// Semantic-only paired analysis, separate from complete-denominator deployment success.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SemanticEffectSummary {
    /// Pairs with explicit True/False primary outcomes on both variants.
    pub jointly_scored_cases: usize,
    /// Operational/error pairs excluded from this semantic-only estimate.
    pub excluded_pairs: usize,
    /// Exclusion counts keyed by stable reason.
    pub exclusion_reasons: BTreeMap<String, usize>,
    /// Paired transition matrix over jointly scored cases.
    pub paired: PairedMetrics,
    /// Paired bootstrap interval when at least one pair is jointly scored.
    pub bootstrap: Option<BootstrapInterval>,
}

/// Per-evaluator aggregate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvaluatorComparison {
    /// Baseline evaluator states.
    pub baseline: EvaluatorStateCounts,
    /// Candidate evaluator states.
    pub candidate: EvaluatorStateCounts,
}

/// Complete-denominator state counts for one evaluator and variant.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvaluatorStateCounts {
    /// Complete case denominator.
    pub total: usize,
    /// Explicit passes.
    pub passed: usize,
    /// Explicit failures.
    pub failed: usize,
    /// Evaluator errors.
    pub error: usize,
    /// Explicitly not-applicable results.
    pub not_applicable: usize,
    /// Cases with no evaluator result.
    pub unscored: usize,
}

/// JSON Pointer-level paired changes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FieldHotspot {
    /// Evaluator whose facts produced this diagnostic.
    #[serde(default)]
    pub evaluator_id: String,
    /// Output JSON Pointer.
    pub pointer: String,
    /// Baseline pass and candidate fail.
    pub regressions: usize,
    /// Baseline fail and candidate pass.
    pub improvements: usize,
    /// Total candidate failures.
    pub candidate_failures: usize,
    /// Baseline field-result states, including missing results.
    #[serde(default)]
    pub baseline: EvaluatorStateCounts,
    /// Candidate field-result states, including missing results.
    #[serde(default)]
    pub candidate: EvaluatorStateCounts,
}

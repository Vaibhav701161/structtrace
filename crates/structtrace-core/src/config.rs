//! Versioned StructTrace configuration.

use std::{collections::BTreeMap, path::PathBuf, str::FromStr};

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::{CoreError, Result, hashing::read_bounded};

/// Complete `structtrace.yaml` configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Configuration schema version. Only version 1 is accepted.
    pub version: u32,
    /// Project identity.
    pub project: ProjectConfig,
    /// Local artifact and privacy settings.
    #[serde(default)]
    pub storage: StorageConfig,
    /// Bounded runtime and report-embedding limits.
    #[serde(default)]
    pub limits: LimitsConfig,
    /// Matched-case dataset.
    pub dataset: DatasetConfig,
    /// Caller-facing output schema.
    pub schema: SchemaConfig,
    /// Required baseline and candidate implementations.
    pub variants: BTreeMap<String, VariantConfig>,
    /// Deterministic evaluator definitions.
    #[serde(default)]
    pub evaluators: Vec<EvaluatorConfig>,
    /// Named semantic or executable outcomes.
    pub outcomes: BTreeMap<String, OutcomeConfig>,
    /// Paired analysis settings.
    pub analysis: AnalysisConfig,
    /// Deployment gate rules.
    #[serde(default)]
    pub gate: GateConfig,
    /// Report behavior.
    #[serde(default)]
    pub report: ReportConfig,
}

/// Hard ceilings prevent a configuration typo from disabling bounded-memory safeguards.
pub const HARD_MAX_OUTPUT_BYTES_PER_CASE: usize = 64 * 1024 * 1024;
/// Hard ceiling for retained standard error from one process.
pub const HARD_MAX_STDERR_BYTES_PER_PROCESS: usize = 16 * 1024 * 1024;
/// Hard ceiling for one raw-output value embedded in a report.
pub const HARD_MAX_REPORT_RAW_BYTES_PER_CASE: usize = 4 * 1024 * 1024;
/// Hard ceiling for all generated report assets combined.
pub const HARD_MAX_REPORT_TOTAL_BYTES: usize = 1024 * 1024 * 1024;
/// Hard ceiling for the optional self-contained report export.
pub const HARD_MAX_SINGLE_FILE_REPORT_BYTES: usize = 100 * 1024 * 1024;
/// Hard ceiling for one configured adapter or evaluator timeout.
pub const HARD_MAX_TIMEOUT_MS: u64 = 24 * 60 * 60 * 1000;
/// Hard ceiling for simultaneous provider requests.
pub const HARD_MAX_CONCURRENCY: usize = 256;
/// Hard ceiling for explicit provider retries.
pub const HARD_MAX_RETRIES: u32 = 20;
/// Hard ceiling for provider-requested generated tokens.
pub const HARD_MAX_OUTPUT_TOKENS: u32 = 1_000_000;
/// Hard ceiling for source configuration bytes.
pub const HARD_MAX_CONFIG_BYTES: usize = 16 * 1024 * 1024;
/// Hard ceiling for dataset bytes.
pub const HARD_MAX_DATASET_BYTES: usize = 1024 * 1024 * 1024;
/// Hard ceiling for one recorded-output artifact.
pub const HARD_MAX_RECORDED_OUTPUT_BYTES: usize = 2 * 1024 * 1024 * 1024;
/// Hard ceiling for an external JSON Schema.
pub const HARD_MAX_SCHEMA_BYTES: usize = 64 * 1024 * 1024;
/// Hard ceiling for matched case count.
pub const HARD_MAX_CASES: usize = 10_000_000;
/// Hard ceiling for one JSONL record.
pub const HARD_MAX_JSONL_LINE_BYTES: usize = 64 * 1024 * 1024;
/// Hard ceiling for one derived artifact consumed during replay.
pub const HARD_MAX_REPLAY_ARTIFACT_BYTES: usize = 2 * 1024 * 1024 * 1024;
/// Hard ceiling for bootstrap replicates, independent of configuration schema validation.
pub const HARD_MAX_BOOTSTRAP_SAMPLES: usize = 1_000_000;
/// Hard ceiling for the product of bootstrap replicates and evidence units.
pub const HARD_MAX_BOOTSTRAP_WORK_UNITS: usize = 100_000_000;
/// Hard ceiling for explicitly bound implementation source files.
pub const HARD_MAX_IMPLEMENTATION_SOURCES: usize = 256;

/// Configurable resource limits with conservative defaults and enforced hard ceilings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LimitsConfig {
    /// Maximum bytes accepted for the source configuration.
    pub max_config_bytes: usize,
    /// Maximum bytes accepted for the dataset source.
    pub max_dataset_bytes: usize,
    /// Maximum bytes accepted for either recorded-output source.
    pub max_recorded_output_bytes: usize,
    /// Maximum bytes accepted for the external JSON Schema.
    pub max_schema_bytes: usize,
    /// Maximum number of cases accepted from a dataset.
    pub max_cases: usize,
    /// Maximum bytes accepted for one JSONL line.
    pub max_jsonl_line_bytes: usize,
    /// Maximum bytes accepted for one derived replay artifact.
    pub max_replay_artifact_bytes: usize,
    /// Maximum model or adapter output bytes retained for one case.
    pub max_output_bytes_per_case: usize,
    /// Maximum standard-error bytes retained from one process.
    pub max_stderr_bytes_per_process: usize,
    /// Maximum raw-output bytes embedded per variant in the HTML report.
    pub max_report_raw_bytes_per_case: usize,
    /// Maximum bytes across the generated report directory.
    pub max_report_total_bytes: usize,
    /// Maximum bytes allowed for an optional self-contained HTML export.
    pub max_single_file_report_bytes: usize,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            max_config_bytes: 1024 * 1024,
            max_dataset_bytes: 256 * 1024 * 1024,
            max_recorded_output_bytes: 512 * 1024 * 1024,
            max_schema_bytes: 16 * 1024 * 1024,
            max_cases: 1_000_000,
            max_jsonl_line_bytes: 16 * 1024 * 1024,
            max_replay_artifact_bytes: 512 * 1024 * 1024,
            max_output_bytes_per_case: 4 * 1024 * 1024,
            max_stderr_bytes_per_process: 1024 * 1024,
            max_report_raw_bytes_per_case: 256 * 1024,
            max_report_total_bytes: 256 * 1024 * 1024,
            max_single_file_report_bytes: 10 * 1024 * 1024,
        }
    }
}

/// Human-readable project identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    /// Stable project name.
    pub name: String,
    /// Optional explanation displayed in reports.
    #[serde(default)]
    pub description: Option<String>,
}

/// Local artifact retention and redaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StorageConfig {
    /// Root of local StructTrace state.
    pub root: PathBuf,
    /// Whether original model output is retained.
    pub retain_raw_outputs: bool,
    /// Whether complete provider envelopes are retained.
    pub retain_provider_responses: bool,
    /// Pointers redacted before report export.
    pub redaction: RedactionConfig,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            root: PathBuf::from(".structtrace"),
            retain_raw_outputs: true,
            retain_provider_responses: false,
            redaction: RedactionConfig::default(),
        }
    }
}

/// Report redaction rules.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RedactionConfig {
    /// JSON Pointers replaced with a fixed redaction marker.
    pub json_pointers: Vec<String>,
    /// Text policy: exact structured echoes or aggressive substring removal.
    #[serde(default)]
    pub text_mode: TextRedactionMode,
    /// Additional literal secret patterns removed from report text.
    #[serde(default)]
    pub custom_patterns: Vec<String>,
}

/// Free-form text redaction strength.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextRedactionMode {
    /// Redact exact typed values and sufficiently distinctive substrings.
    #[default]
    ExactStructured,
    /// Also replace short numeric and Boolean values inside arbitrary text.
    AggressiveTextual,
}

/// JSONL dataset and field mapping.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatasetConfig {
    /// Dataset path, relative to the project root when not absolute.
    pub path: PathBuf,
    /// Currently only `jsonl` is supported.
    #[serde(default = "default_jsonl")]
    pub format: String,
    /// JSON Pointer mapping for case envelope fields.
    #[serde(default)]
    pub fields: DatasetFields,
    /// Explicit definition of the independent statistical evidence unit.
    #[serde(default)]
    pub evidence_unit: EvidenceUnitConfig,
}

/// How rows are grouped into independent evidence units.
///
/// With neither field configured, StructTrace fingerprints only input,
/// expected output, and explicitly model-visible metadata. Arbitrary
/// evaluation metadata is deliberately excluded because trace IDs and
/// timestamps do not create independent semantic evidence.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EvidenceUnitConfig {
    /// One normalized-case pointer whose value identifies the evidence unit.
    pub pointer: Option<String>,
    /// Normalized-case pointers included in a canonical fingerprint.
    pub include: Option<Vec<String>>,
}

fn default_jsonl() -> String {
    "jsonl".to_owned()
}

/// Field pointers within each JSONL case.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DatasetFields {
    /// Non-empty unique case identifier.
    pub id: String,
    /// Model or application input.
    pub input: String,
    /// Optional expected value.
    pub expected: String,
    /// Optional metadata exposed to baseline and candidate implementations.
    pub model_visible_metadata: String,
    /// Optional evaluation-only metadata.
    pub metadata: String,
}

impl Default for DatasetFields {
    fn default() -> Self {
        Self {
            id: "/id".to_owned(),
            input: "/input".to_owned(),
            expected: "/expected".to_owned(),
            model_visible_metadata: "/model_visible_metadata".to_owned(),
            metadata: "/metadata".to_owned(),
        }
    }
}

/// External JSON Schema configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaConfig {
    /// Schema path.
    pub path: PathBuf,
}

/// Baseline or candidate execution source.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VariantConfig {
    /// Pre-recorded JSONL results.
    Recorded {
        /// Output envelope path.
        path: PathBuf,
    },
    /// Versioned JSONL subprocess.
    Command {
        /// Executable and argument array.
        command: CommandSpec,
        /// Persistent process or one process per case.
        #[serde(default)]
        process_mode: ProcessMode,
        /// Per-case timeout.
        #[serde(default = "default_timeout_ms")]
        timeout_ms: u64,
        /// Explicit immutable digest and/or source files bound into resume identity.
        #[serde(default)]
        implementation: ImplementationConfig,
    },
    /// Python callable invoked through the bundled bridge.
    Python {
        /// Python executable.
        #[serde(default = "default_python")]
        interpreter: String,
        /// Import path formatted as `module:callable`.
        callable: String,
        /// Per-case timeout.
        #[serde(default = "default_timeout_ms")]
        timeout_ms: u64,
        /// Explicit immutable digest and/or source files bound into resume identity.
        #[serde(default)]
        implementation: ImplementationConfig,
    },
    /// OpenAI-compatible chat-completions endpoint.
    OpenaiCompatible(Box<OpenAiCompatibleConfig>),
}

/// User-declared implementation identity for command and Python applications.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ImplementationConfig {
    /// Immutable implementation digest supplied by the application owner.
    pub digest: Option<String>,
    /// Source or data files whose exact bytes define the implementation.
    pub sources: Vec<PathBuf>,
}

impl ImplementationConfig {
    fn is_empty(&self) -> bool {
        self.digest.is_none() && self.sources.is_empty()
    }
}

/// Executable and argument array. No shell is involved.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandSpec {
    /// Executable name or path.
    pub program: String,
    /// Literal arguments.
    #[serde(default)]
    pub args: Vec<String>,
}

/// Subprocess lifecycle.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessMode {
    /// One process serves multiple cases.
    #[default]
    Persistent,
    /// A clean process is created for each case.
    PerCase,
}

fn default_timeout_ms() -> u64 {
    60_000
}

fn default_python() -> String {
    if cfg!(windows) {
        "python".to_owned()
    } else {
        "python3".to_owned()
    }
}

/// Focused OpenAI-compatible adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenAiCompatibleConfig {
    /// Endpoint root, usually ending in `/v1`.
    pub base_url: String,
    /// Optional environment variable containing the credential. Omit for an
    /// explicitly unauthenticated local endpoint.
    #[serde(default)]
    pub api_key_env: Option<String>,
    /// Provider model identifier.
    pub model: String,
    /// Prompt and decoding configuration.
    pub request: OpenAiRequestConfig,
    /// Optional structured-output request.
    #[serde(default)]
    pub structured_output: Option<StructuredOutputConfig>,
    /// Request timeout.
    #[serde(default = "default_provider_timeout_ms")]
    pub timeout_ms: u64,
    /// Maximum simultaneous requests.
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
    /// Explicit retries. Zero means no retry.
    #[serde(default)]
    pub retries: u32,
    /// Optional user-declared prices.
    #[serde(default)]
    pub pricing: Option<PricingConfig>,
}

fn default_provider_timeout_ms() -> u64 {
    120_000
}

fn default_concurrency() -> usize {
    4
}

/// OpenAI-compatible prompt settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenAiRequestConfig {
    /// Optional system message.
    #[serde(default)]
    pub system: Option<String>,
    /// MiniJinja template rendered against the case envelope.
    pub user_template: String,
    /// Sampling temperature.
    #[serde(default)]
    pub temperature: f64,
    /// Maximum generated tokens.
    #[serde(default = "default_max_output_tokens")]
    pub max_output_tokens: u32,
}

fn default_max_output_tokens() -> u32 {
    500
}

/// Provider-native output formatting request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructuredOutputConfig {
    /// `json_schema` or `json_object`.
    pub mode: String,
    /// Schema path when `mode` is `json_schema`.
    #[serde(default)]
    pub schema: Option<PathBuf>,
}

/// User-supplied prices, never inferred by StructTrace.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PricingConfig {
    /// Cost per million input tokens.
    pub input_per_million: String,
    /// Cost per million output tokens.
    pub output_per_million: String,
    /// ISO-style currency label.
    pub currency: String,
}

/// One deterministic evaluator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluatorConfig {
    /// Stable identifier used by outcomes and reports.
    pub id: String,
    /// Immutable implementation version or digest for external evaluators.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub implementation_version: Option<String>,
    /// Explicit source files or digest that define an external evaluator implementation.
    #[serde(default, skip_serializing_if = "ImplementationConfig::is_empty")]
    pub implementation: ImplementationConfig,
    /// Evaluator behavior.
    #[serde(flatten)]
    pub kind: EvaluatorKind,
}

/// Built-in and subprocess evaluator types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EvaluatorKind {
    /// Complete JSON equality against the expected value.
    ExactJson,
    /// Exact output and expected JSON Pointer values.
    JsonPointerExact {
        /// Pointer in model output.
        pointer: String,
        /// Pointer in expected value.
        expected_pointer: String,
    },
    /// Exact comparison across several pointer pairs.
    JsonPointersExact {
        /// Pointer pairs.
        pointers: Vec<PointerPair>,
    },
    /// Categorical accuracy at a pointer.
    EnumAccuracy {
        /// Pointer in model output.
        pointer: String,
        /// Pointer in expected value.
        expected_pointer: String,
    },
    /// Unicode-normalized, whitespace-collapsed string comparison.
    NormalizedString {
        /// Pointer in model output.
        pointer: String,
        /// Pointer in expected value.
        expected_pointer: String,
        /// Compare case-insensitively after normalization.
        #[serde(default = "default_true")]
        case_insensitive: bool,
    },
    /// Calendar-aware date comparison after canonicalization.
    CanonicalDate {
        /// Pointer in model output.
        pointer: String,
        /// Pointer in expected value.
        expected_pointer: String,
        /// Accepted input formats: `iso`, `dmy_slash`, or `mdy_slash`.
        #[serde(default = "default_date_formats")]
        formats: Vec<String>,
    },
    /// Decimal tolerance or exact-integer comparison.
    NumericTolerance {
        /// Pointer in model output.
        pointer: String,
        /// Pointer in expected value.
        expected_pointer: String,
        /// Absolute decimal tolerance.
        #[serde(default)]
        absolute: Option<String>,
        /// Relative decimal tolerance.
        #[serde(default)]
        relative: Option<String>,
        /// Require lexical integer values and exact arbitrary-precision equality.
        #[serde(default)]
        exact_integer: bool,
    },
    /// Selected output fields must exist and be non-null.
    RequiredFields {
        /// JSON Pointers checked.
        pointers: Vec<String>,
    },
    /// Tool-name comparison.
    ToolSelection {
        /// Pointer in model output.
        #[serde(default = "default_tool_name_pointer")]
        pointer: String,
        /// Pointer in expected value.
        #[serde(default = "default_tool_name_pointer")]
        expected_pointer: String,
    },
    /// Exact selected tool-argument comparison.
    ToolArguments {
        /// Pointer pairs below the argument object.
        pointers: Vec<PointerPair>,
    },
    /// Order-independent array comparison keyed by selected item fields.
    KeyedArray {
        /// Array pointer in model output.
        pointer: String,
        /// Array pointer in expected value.
        expected_pointer: String,
        /// Relative JSON Pointers that form each item's identity.
        keys: Vec<String>,
        /// Optional field-specific comparators applied after identity matching.
        #[serde(default)]
        fields: Vec<KeyedArrayField>,
    },
    /// Invoice arithmetic consistency independent of golden-answer matching.
    FinancialInvariants {
        /// Line-item array pointer.
        #[serde(default = "default_line_items_pointer")]
        line_items_pointer: String,
        /// Subtotal pointer.
        #[serde(default = "default_subtotal_pointer")]
        subtotal_pointer: String,
        /// Tax pointer.
        #[serde(default = "default_tax_pointer")]
        tax_pointer: String,
        /// Total pointer.
        #[serde(default = "default_total_pointer")]
        total_pointer: String,
        /// Absolute decimal tolerance.
        #[serde(default = "default_decimal_tolerance")]
        absolute: String,
    },
    /// Versioned command evaluator.
    Command {
        /// Executable and arguments.
        command: CommandSpec,
        /// Persistent worker or one process per request.
        #[serde(default)]
        process_mode: ProcessMode,
        /// Per-case timeout.
        #[serde(default = "default_timeout_ms")]
        timeout_ms: u64,
    },
    /// Python evaluator through the bundled bridge.
    Python {
        /// Python executable.
        #[serde(default = "default_python")]
        interpreter: String,
        /// Import path formatted as `module:callable`.
        callable: String,
        /// Persistent worker or one interpreter per request.
        #[serde(default)]
        process_mode: ProcessMode,
        /// Per-case timeout.
        #[serde(default = "default_timeout_ms")]
        timeout_ms: u64,
    },
}

/// Comparator for one field inside a matched keyed-array item.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyedArrayField {
    /// Relative JSON Pointer within each item.
    pub pointer: String,
    /// `exact`, `normalized_string`, `exact_integer`, `decimal_tolerance`, or `canonical_date`.
    pub evaluator: String,
    /// Absolute tolerance for `decimal_tolerance`.
    #[serde(default)]
    pub absolute: Option<String>,
    /// Case folding for `normalized_string`.
    #[serde(default = "default_true")]
    pub case_insensitive: bool,
    /// Accepted formats for `canonical_date`.
    #[serde(default = "default_date_formats")]
    pub formats: Vec<String>,
}

fn default_tool_name_pointer() -> String {
    "/name".to_owned()
}

fn default_true() -> bool {
    true
}

fn default_date_formats() -> Vec<String> {
    vec!["iso".to_owned()]
}

fn default_line_items_pointer() -> String {
    "/line_items".to_owned()
}

fn default_subtotal_pointer() -> String {
    "/subtotal".to_owned()
}

fn default_tax_pointer() -> String {
    "/tax".to_owned()
}

fn default_total_pointer() -> String {
    "/total".to_owned()
}

fn default_decimal_tolerance() -> String {
    "0.01".to_owned()
}

/// Output and expected pointer pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointerPair {
    /// Pointer in model output.
    pub pointer: String,
    /// Pointer in expected value.
    pub expected_pointer: String,
}

/// A small, auditable composition of evaluator facts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomeConfig {
    /// Every listed evaluator must pass.
    #[serde(default)]
    pub all_of: Vec<String>,
    /// At least one listed evaluator must pass.
    #[serde(default)]
    pub any_of: Vec<String>,
}

/// Primary paired analysis and interval settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisConfig {
    /// Named primary outcome.
    pub primary_outcome: String,
    /// Seeded paired bootstrap.
    #[serde(default)]
    pub bootstrap: BootstrapConfig,
}

/// Paired bootstrap settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BootstrapConfig {
    /// Resample count.
    pub samples: usize,
    /// Interval coverage between zero and one.
    pub confidence: f64,
    /// Deterministic seed.
    pub seed: u64,
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            samples: 10_000,
            confidence: 0.95,
            seed: 17,
        }
    }
}

/// Independent release-gate rules.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GateConfig {
    /// Minimum paired case count required for a release decision.
    pub min_cases: Option<usize>,
    /// Minimum distinct semantic evidence units required for a release decision.
    pub min_unique_cases: Option<usize>,
    /// Maximum fraction of rows belonging to an exact semantic duplicate group.
    pub max_duplicate_case_rate: Option<f64>,
    /// Minimum fraction explicitly scored pass or fail on both variants.
    pub min_primary_scored_rate: Option<f64>,
    /// Maximum evaluator-error fraction on either variant.
    pub max_primary_evaluator_error_rate: Option<f64>,
    /// Maximum not-applicable fraction on either variant.
    pub max_primary_not_applicable_rate: Option<f64>,
    /// Maximum unscored fraction on either variant.
    pub max_primary_unscored_rate: Option<f64>,
    /// Maximum allowed primary-outcome decline in percentage points.
    pub max_primary_regression_pp: Option<f64>,
    /// Maximum allowed increase in valid-but-wrong rate.
    pub max_valid_but_wrong_increase_pp: Option<f64>,
    /// Required candidate schema-validity fraction.
    pub min_candidate_schema_validity: Option<f64>,
    /// Maximum candidate error fraction.
    pub max_error_rate: Option<f64>,
    /// Maximum candidate timeout fraction.
    pub max_timeout_rate: Option<f64>,
    /// Optional latency gate.
    pub latency: Option<LatencyGateConfig>,
    /// Optional cost gate.
    pub cost: Option<CostGateConfig>,
}

impl GateConfig {
    /// Whether at least one release criterion is configured.
    pub fn is_configured(&self) -> bool {
        self.min_cases.is_some()
            || self.min_unique_cases.is_some()
            || self.max_duplicate_case_rate.is_some()
            || self.min_primary_scored_rate.is_some()
            || self.max_primary_evaluator_error_rate.is_some()
            || self.max_primary_not_applicable_rate.is_some()
            || self.max_primary_unscored_rate.is_some()
            || self.max_primary_regression_pp.is_some()
            || self.max_valid_but_wrong_increase_pp.is_some()
            || self.min_candidate_schema_validity.is_some()
            || self.max_error_rate.is_some()
            || self.max_timeout_rate.is_some()
            || self.latency.is_some()
            || self.cost.is_some()
    }
}

/// Operational latency gate.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LatencyGateConfig {
    /// Maximum candidate p95 increase relative to baseline.
    pub max_p95_increase_percent: f64,
    /// Minimum fraction of paired cases with latency on both variants.
    #[serde(default = "default_operational_gate_coverage")]
    pub min_coverage: f64,
}

/// Operational cost gate.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CostGateConfig {
    /// Maximum candidate average cost increase relative to baseline.
    pub max_average_increase_percent: f64,
    /// Minimum fraction of paired cases with comparable cost on both variants.
    #[serde(default = "default_operational_gate_coverage")]
    pub min_coverage: f64,
}

fn default_operational_gate_coverage() -> f64 {
    1.0
}

/// HTML report preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ReportConfig {
    /// Report title.
    pub title: Option<String>,
    /// Include retained raw output in case detail.
    pub include_raw_outputs: bool,
    /// Include prompts where adapters retain them.
    pub include_prompts: bool,
    /// Initial case filter.
    pub default_case_filter: String,
}

impl Default for ReportConfig {
    fn default() -> Self {
        Self {
            title: None,
            include_raw_outputs: true,
            include_prompts: false,
            default_case_filter: "discordant".to_owned(),
        }
    }
}

impl Config {
    /// Load YAML or JSON and validate cross-field invariants.
    pub fn load(path: &std::path::Path) -> Result<Self> {
        let bytes = read_bounded(path, HARD_MAX_CONFIG_BYTES, "configuration")?;
        Self::from_bytes(path, &bytes)
    }

    /// Parse an immutable configuration snapshot captured by the caller.
    pub fn from_bytes(path: &std::path::Path, bytes: &[u8]) -> Result<Self> {
        let config = if path.extension().and_then(|value| value.to_str()) == Some("json") {
            serde_json::from_slice(bytes)
                .map_err(|error| CoreError::Configuration(format!("{}: {error}", path.display())))?
        } else {
            serde_yaml_ng::from_slice(bytes)
                .map_err(|error| CoreError::Configuration(format!("{}: {error}", path.display())))?
        };
        Self::validate(config)
    }

    /// Validate a parsed configuration before any adapter is invoked.
    pub fn validate(config: Self) -> Result<Self> {
        if config.version != 1 {
            return Err(CoreError::Configuration(format!(
                "unsupported version {}; expected 1",
                config.version
            )));
        }
        if config.project.name.trim().is_empty() {
            return Err(CoreError::Configuration(
                "project.name must not be empty".to_owned(),
            ));
        }
        for required in ["baseline", "candidate"] {
            if !config.variants.contains_key(required) {
                return Err(CoreError::Configuration(format!(
                    "variants.{required} is required"
                )));
            }
        }
        if config.variants.len() != 2 {
            let extras = config
                .variants
                .keys()
                .filter(|name| name.as_str() != "baseline" && name.as_str() != "candidate")
                .cloned()
                .collect::<Vec<_>>();
            return Err(CoreError::Configuration(format!(
                "version 1 supports exactly baseline and candidate variants; unsupported variants: {}",
                extras.join(", ")
            )));
        }
        if config.dataset.format != "jsonl" {
            return Err(CoreError::Configuration(format!(
                "unsupported dataset format `{}`; expected `jsonl`",
                config.dataset.format
            )));
        }
        if !config
            .outcomes
            .contains_key(&config.analysis.primary_outcome)
        {
            return Err(CoreError::Configuration(format!(
                "analysis.primary_outcome `{}` is not defined in outcomes",
                config.analysis.primary_outcome
            )));
        }
        let evaluator_ids = config
            .evaluators
            .iter()
            .map(|item| item.id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        if evaluator_ids.len() != config.evaluators.len() {
            return Err(CoreError::Configuration(
                "evaluator IDs must be unique".to_owned(),
            ));
        }
        for (name, outcome) in &config.outcomes {
            if name.trim().is_empty() {
                return Err(CoreError::Configuration(
                    "outcome names must not be empty".to_owned(),
                ));
            }
            let mode_count =
                usize::from(!outcome.all_of.is_empty()) + usize::from(!outcome.any_of.is_empty());
            if mode_count != 1 {
                return Err(CoreError::Configuration(format!(
                    "outcome `{name}` must define exactly one non-empty `all_of` or `any_of`"
                )));
            }
            for id in outcome.all_of.iter().chain(&outcome.any_of) {
                if !evaluator_ids.contains(id.as_str()) {
                    return Err(CoreError::Configuration(format!(
                        "outcome `{name}` references unknown evaluator `{id}`"
                    )));
                }
            }
        }
        if config.analysis.bootstrap.samples == 0
            || config.analysis.bootstrap.samples > HARD_MAX_BOOTSTRAP_SAMPLES
            || !(0.0..1.0).contains(&config.analysis.bootstrap.confidence)
        {
            return Err(CoreError::Configuration(format!(
                "bootstrap samples must be between 1 and {HARD_MAX_BOOTSTRAP_SAMPLES}; confidence must be between 0 and 1"
            )));
        }
        for (name, value, maximum) in [
            (
                "limits.max_config_bytes",
                config.limits.max_config_bytes,
                HARD_MAX_CONFIG_BYTES,
            ),
            (
                "limits.max_dataset_bytes",
                config.limits.max_dataset_bytes,
                HARD_MAX_DATASET_BYTES,
            ),
            (
                "limits.max_recorded_output_bytes",
                config.limits.max_recorded_output_bytes,
                HARD_MAX_RECORDED_OUTPUT_BYTES,
            ),
            (
                "limits.max_schema_bytes",
                config.limits.max_schema_bytes,
                HARD_MAX_SCHEMA_BYTES,
            ),
            ("limits.max_cases", config.limits.max_cases, HARD_MAX_CASES),
            (
                "limits.max_jsonl_line_bytes",
                config.limits.max_jsonl_line_bytes,
                HARD_MAX_JSONL_LINE_BYTES,
            ),
            (
                "limits.max_replay_artifact_bytes",
                config.limits.max_replay_artifact_bytes,
                HARD_MAX_REPLAY_ARTIFACT_BYTES,
            ),
            (
                "limits.max_output_bytes_per_case",
                config.limits.max_output_bytes_per_case,
                HARD_MAX_OUTPUT_BYTES_PER_CASE,
            ),
            (
                "limits.max_stderr_bytes_per_process",
                config.limits.max_stderr_bytes_per_process,
                HARD_MAX_STDERR_BYTES_PER_PROCESS,
            ),
            (
                "limits.max_report_raw_bytes_per_case",
                config.limits.max_report_raw_bytes_per_case,
                HARD_MAX_REPORT_RAW_BYTES_PER_CASE,
            ),
            (
                "limits.max_report_total_bytes",
                config.limits.max_report_total_bytes,
                HARD_MAX_REPORT_TOTAL_BYTES,
            ),
            (
                "limits.max_single_file_report_bytes",
                config.limits.max_single_file_report_bytes,
                HARD_MAX_SINGLE_FILE_REPORT_BYTES,
            ),
        ] {
            if value == 0 || value > maximum {
                return Err(CoreError::Configuration(format!(
                    "{name} must be between 1 and {maximum} bytes"
                )));
            }
        }
        validate_nonempty_path("storage.root", &config.storage.root)?;
        if config
            .storage
            .redaction
            .custom_patterns
            .iter()
            .any(|pattern| pattern.is_empty())
        {
            return Err(CoreError::Configuration(
                "storage.redaction.custom_patterns must not contain empty strings".to_owned(),
            ));
        }
        validate_nonempty_path("dataset.path", &config.dataset.path)?;
        validate_nonempty_path("schema.path", &config.schema.path)?;
        for (name, pointer) in [
            ("dataset.fields.id", &config.dataset.fields.id),
            ("dataset.fields.input", &config.dataset.fields.input),
            ("dataset.fields.expected", &config.dataset.fields.expected),
            (
                "dataset.fields.model_visible_metadata",
                &config.dataset.fields.model_visible_metadata,
            ),
            ("dataset.fields.metadata", &config.dataset.fields.metadata),
        ] {
            validate_json_pointer(name, pointer)?;
        }
        validate_dataset_field_isolation(&config.dataset.fields)?;
        validate_evidence_unit(&config.dataset.evidence_unit)?;
        for (name, variant) in &config.variants {
            validate_variant(&format!("variants.{name}"), variant)?;
        }
        for evaluator in &config.evaluators {
            if evaluator.id.trim().is_empty() {
                return Err(CoreError::Configuration(
                    "evaluator IDs must not be empty".to_owned(),
                ));
            }
            validate_evaluator(&evaluator.id, &evaluator.kind)?;
            validate_implementation(
                &format!("evaluators.{}", evaluator.id),
                &evaluator.implementation,
            )?;
            if matches!(
                evaluator.kind,
                EvaluatorKind::Command { .. } | EvaluatorKind::Python { .. }
            ) && evaluator
                .implementation_version
                .as_deref()
                .is_none_or(|version| version.trim().is_empty())
            {
                return Err(CoreError::Configuration(format!(
                    "external evaluator `{}` requires a nonempty implementation_version",
                    evaluator.id
                )));
            }
        }
        validate_gate(&config.gate)?;
        const REPORT_FILTERS: &[&str] = &[
            "all",
            "discordant",
            "baseline_only_pass",
            "candidate_only_pass",
            "both_fail",
            "valid_but_wrong",
            "parse_failure",
            "schema_failure",
            "adapter_error",
            "evaluator_error",
            "not_applicable",
            "unscored",
        ];
        if !REPORT_FILTERS.contains(&config.report.default_case_filter.as_str()) {
            return Err(CoreError::Configuration(format!(
                "unsupported report.default_case_filter `{}`",
                config.report.default_case_filter
            )));
        }
        Ok(config)
    }
}

fn validate_variant(name: &str, variant: &VariantConfig) -> Result<()> {
    match variant {
        VariantConfig::Recorded { path } => validate_nonempty_path(&format!("{name}.path"), path),
        VariantConfig::Command {
            command,
            timeout_ms,
            implementation,
            ..
        } => {
            validate_command(&format!("{name}.command"), command)?;
            validate_timeout(&format!("{name}.timeout_ms"), *timeout_ms)?;
            validate_implementation(name, implementation)
        }
        VariantConfig::Python {
            interpreter,
            callable,
            timeout_ms,
            implementation,
        } => {
            validate_python(&format!("{name}.python"), interpreter, callable)?;
            validate_timeout(&format!("{name}.timeout_ms"), *timeout_ms)?;
            validate_implementation(name, implementation)
        }
        VariantConfig::OpenaiCompatible(adapter) => {
            for (field, value) in [
                ("base_url", adapter.base_url.as_str()),
                ("model", adapter.model.as_str()),
                (
                    "request.user_template",
                    adapter.request.user_template.as_str(),
                ),
            ] {
                if value.trim().is_empty() {
                    return Err(CoreError::Configuration(format!(
                        "{name}.{field} must not be empty"
                    )));
                }
            }
            let base_url = url::Url::parse(&adapter.base_url).map_err(|error| {
                CoreError::Configuration(format!("{name}.base_url is invalid: {error}"))
            })?;
            if !matches!(base_url.scheme(), "http" | "https")
                || base_url.host_str().is_none()
                || !base_url.username().is_empty()
                || base_url.password().is_some()
                || base_url.query().is_some()
                || base_url.fragment().is_some()
            {
                return Err(CoreError::Configuration(format!(
                    "{name}.base_url must be an HTTP(S) root with a host and without credentials, query, or fragment"
                )));
            }
            if base_url
                .path()
                .trim_end_matches('/')
                .ends_with("/chat/completions")
            {
                return Err(CoreError::Configuration(format!(
                    "{name}.base_url must be the API root (for example /v1), not the full /chat/completions endpoint"
                )));
            }
            if adapter
                .api_key_env
                .as_deref()
                .is_some_and(|value| value.trim().is_empty())
            {
                return Err(CoreError::Configuration(format!(
                    "{name}.api_key_env must be omitted or non-empty"
                )));
            }
            validate_timeout(&format!("{name}.timeout_ms"), adapter.timeout_ms)?;
            if adapter.concurrency == 0 || adapter.concurrency > HARD_MAX_CONCURRENCY {
                return Err(CoreError::Configuration(format!(
                    "{name}.concurrency must be between 1 and {HARD_MAX_CONCURRENCY}"
                )));
            }
            if adapter.retries > HARD_MAX_RETRIES {
                return Err(CoreError::Configuration(format!(
                    "{name}.retries must not exceed {HARD_MAX_RETRIES}"
                )));
            }
            if adapter.request.max_output_tokens == 0
                || adapter.request.max_output_tokens > HARD_MAX_OUTPUT_TOKENS
            {
                return Err(CoreError::Configuration(format!(
                    "{name}.request.max_output_tokens must be between 1 and {HARD_MAX_OUTPUT_TOKENS}"
                )));
            }
            if !adapter.request.temperature.is_finite()
                || !(0.0..=2.0).contains(&adapter.request.temperature)
            {
                return Err(CoreError::Configuration(format!(
                    "{name}.request.temperature must be a finite number between 0 and 2"
                )));
            }
            if let Some(structured) = &adapter.structured_output {
                if !matches!(structured.mode.as_str(), "json_schema" | "json_object") {
                    return Err(CoreError::Configuration(format!(
                        "{name}.structured_output.mode must be `json_schema` or `json_object`"
                    )));
                }
                if let Some(path) = &structured.schema {
                    validate_nonempty_path(&format!("{name}.structured_output.schema"), path)?;
                }
            }
            if let Some(pricing) = &adapter.pricing {
                for (field, value) in [
                    ("input_per_million", pricing.input_per_million.as_str()),
                    ("output_per_million", pricing.output_per_million.as_str()),
                ] {
                    let decimal = Decimal::from_str(value).map_err(|_| {
                        CoreError::Configuration(format!(
                            "{name}.pricing.{field} must be a valid decimal"
                        ))
                    })?;
                    if decimal.is_sign_negative() {
                        return Err(CoreError::Configuration(format!(
                            "{name}.pricing.{field} must be non-negative"
                        )));
                    }
                }
                if pricing.currency.trim().is_empty() {
                    return Err(CoreError::Configuration(format!(
                        "{name}.pricing.currency must not be empty"
                    )));
                }
            }
            Ok(())
        }
    }
}

fn validate_implementation(name: &str, implementation: &ImplementationConfig) -> Result<()> {
    if implementation.sources.len() > HARD_MAX_IMPLEMENTATION_SOURCES {
        return Err(CoreError::Configuration(format!(
            "{name}.implementation.sources contains more than {HARD_MAX_IMPLEMENTATION_SOURCES} files"
        )));
    }
    if implementation
        .digest
        .as_deref()
        .is_some_and(|digest| digest.trim().is_empty())
    {
        return Err(CoreError::Configuration(format!(
            "{name}.implementation.digest must be omitted or non-empty"
        )));
    }
    for source in &implementation.sources {
        validate_nonempty_path(&format!("{name}.implementation.sources"), source)?;
    }
    Ok(())
}

fn validate_evaluator(id: &str, evaluator: &EvaluatorKind) -> Result<()> {
    let name = format!("evaluator `{id}`");
    match evaluator {
        EvaluatorKind::ExactJson => Ok(()),
        EvaluatorKind::JsonPointerExact {
            pointer,
            expected_pointer,
        }
        | EvaluatorKind::EnumAccuracy {
            pointer,
            expected_pointer,
        }
        | EvaluatorKind::NormalizedString {
            pointer,
            expected_pointer,
            ..
        }
        | EvaluatorKind::ToolSelection {
            pointer,
            expected_pointer,
        } => {
            validate_json_pointer(&format!("{name}.pointer"), pointer)?;
            validate_json_pointer(&format!("{name}.expected_pointer"), expected_pointer)
        }
        EvaluatorKind::CanonicalDate {
            pointer,
            expected_pointer,
            formats,
        } => {
            validate_json_pointer(&format!("{name}.pointer"), pointer)?;
            validate_json_pointer(&format!("{name}.expected_pointer"), expected_pointer)?;
            if formats.is_empty()
                || formats
                    .iter()
                    .any(|format| !matches!(format.as_str(), "iso" | "dmy_slash" | "mdy_slash"))
            {
                return Err(CoreError::Configuration(format!(
                    "{name}.formats must contain only iso, dmy_slash, or mdy_slash"
                )));
            }
            if formats.iter().any(|format| format == "dmy_slash")
                && formats.iter().any(|format| format == "mdy_slash")
            {
                return Err(CoreError::Configuration(format!(
                    "{name}.formats cannot combine dmy_slash and mdy_slash without an explicit ambiguity policy"
                )));
            }
            Ok(())
        }
        EvaluatorKind::JsonPointersExact { pointers }
        | EvaluatorKind::ToolArguments { pointers } => validate_pointer_pairs(&name, pointers),
        EvaluatorKind::KeyedArray {
            pointer,
            expected_pointer,
            keys,
            fields,
        } => {
            validate_json_pointer(&format!("{name}.pointer"), pointer)?;
            validate_json_pointer(&format!("{name}.expected_pointer"), expected_pointer)?;
            if keys.is_empty() {
                return Err(CoreError::Configuration(format!(
                    "{name}.keys must not be empty"
                )));
            }
            if fields.is_empty() {
                return Err(CoreError::Configuration(format!(
                    "{name}.fields must not be empty; configure field semantics explicitly"
                )));
            }
            for key in keys {
                validate_json_pointer(&format!("{name}.keys"), key)?;
            }
            for field in fields {
                validate_json_pointer(&format!("{name}.fields.pointer"), &field.pointer)?;
                if !matches!(
                    field.evaluator.as_str(),
                    "exact"
                        | "normalized_string"
                        | "exact_integer"
                        | "decimal_tolerance"
                        | "canonical_date"
                ) {
                    return Err(CoreError::Configuration(format!(
                        "{name}.fields evaluator `{}` is unsupported",
                        field.evaluator
                    )));
                }
                if field.evaluator == "decimal_tolerance" {
                    let value = field.absolute.as_deref().ok_or_else(|| {
                        CoreError::Configuration(format!(
                            "{name}.fields decimal_tolerance requires absolute"
                        ))
                    })?;
                    let tolerance = Decimal::from_str(value).map_err(|_| {
                        CoreError::Configuration(format!(
                            "{name}.fields absolute must be a valid decimal"
                        ))
                    })?;
                    if tolerance.is_sign_negative() {
                        return Err(CoreError::Configuration(format!(
                            "{name}.fields absolute must be non-negative"
                        )));
                    }
                }
                if field.evaluator == "canonical_date"
                    && (field.formats.is_empty()
                        || field.formats.iter().any(|format| {
                            !matches!(format.as_str(), "iso" | "dmy_slash" | "mdy_slash")
                        })
                        || (field.formats.iter().any(|format| format == "dmy_slash")
                            && field.formats.iter().any(|format| format == "mdy_slash")))
                {
                    return Err(CoreError::Configuration(format!(
                        "{name}.fields canonical_date formats must be non-ambiguous and supported"
                    )));
                }
            }
            Ok(())
        }
        EvaluatorKind::FinancialInvariants {
            line_items_pointer,
            subtotal_pointer,
            tax_pointer,
            total_pointer,
            absolute,
        } => {
            for (field, pointer) in [
                ("line_items_pointer", line_items_pointer),
                ("subtotal_pointer", subtotal_pointer),
                ("tax_pointer", tax_pointer),
                ("total_pointer", total_pointer),
            ] {
                validate_json_pointer(&format!("{name}.{field}"), pointer)?;
            }
            let tolerance = Decimal::from_str(absolute).map_err(|_| {
                CoreError::Configuration(format!("{name}.absolute must be a valid decimal"))
            })?;
            if tolerance.is_sign_negative() {
                return Err(CoreError::Configuration(format!(
                    "{name}.absolute must be non-negative"
                )));
            }
            Ok(())
        }
        EvaluatorKind::NumericTolerance {
            pointer,
            expected_pointer,
            absolute,
            relative,
            ..
        } => {
            validate_json_pointer(&format!("{name}.pointer"), pointer)?;
            validate_json_pointer(&format!("{name}.expected_pointer"), expected_pointer)?;
            for (field, value) in [("absolute", absolute), ("relative", relative)] {
                if let Some(value) = value {
                    let decimal = Decimal::from_str(value).map_err(|_| {
                        CoreError::Configuration(format!("{name}.{field} must be a valid decimal"))
                    })?;
                    if decimal.is_sign_negative() {
                        return Err(CoreError::Configuration(format!(
                            "{name}.{field} must be non-negative"
                        )));
                    }
                }
            }
            Ok(())
        }
        EvaluatorKind::RequiredFields { pointers } => {
            if pointers.is_empty() {
                return Err(CoreError::Configuration(format!(
                    "{name}.pointers must not be empty"
                )));
            }
            for pointer in pointers {
                validate_json_pointer(&format!("{name}.pointers"), pointer)?;
            }
            Ok(())
        }
        EvaluatorKind::Command {
            command,
            process_mode,
            timeout_ms,
        } => {
            if matches!(process_mode, ProcessMode::PerCase) {
                return Err(CoreError::Configuration(format!(
                    "{name}.process_mode=per_case is experimental and refused by the stable runtime; use persistent"
                )));
            }
            validate_command(&format!("{name}.command"), command)?;
            validate_timeout(&format!("{name}.timeout_ms"), *timeout_ms)
        }
        EvaluatorKind::Python {
            interpreter,
            callable,
            process_mode,
            timeout_ms,
        } => {
            if matches!(process_mode, ProcessMode::PerCase) {
                return Err(CoreError::Configuration(format!(
                    "{name}.process_mode=per_case is experimental and refused by the stable runtime; use persistent"
                )));
            }
            validate_python(&name, interpreter, callable)?;
            validate_timeout(&format!("{name}.timeout_ms"), *timeout_ms)
        }
    }
}

fn validate_pointer_pairs(name: &str, pointers: &[PointerPair]) -> Result<()> {
    if pointers.is_empty() {
        return Err(CoreError::Configuration(format!(
            "{name}.pointers must not be empty"
        )));
    }
    for pair in pointers {
        validate_json_pointer(&format!("{name}.pointer"), &pair.pointer)?;
        validate_json_pointer(&format!("{name}.expected_pointer"), &pair.expected_pointer)?;
    }
    Ok(())
}

fn validate_python(name: &str, interpreter: &str, callable: &str) -> Result<()> {
    if interpreter.trim().is_empty() {
        return Err(CoreError::Configuration(format!(
            "{name}.interpreter must not be empty"
        )));
    }
    let mut parts = callable.split(':');
    if parts.next().is_none_or(str::is_empty)
        || parts.next().is_none_or(str::is_empty)
        || parts.next().is_some()
    {
        return Err(CoreError::Configuration(format!(
            "{name}.callable must use `module:callable`"
        )));
    }
    Ok(())
}

fn validate_command(name: &str, command: &CommandSpec) -> Result<()> {
    if command.program.trim().is_empty() {
        return Err(CoreError::Configuration(format!(
            "{name}.program must not be empty"
        )));
    }
    Ok(())
}

fn validate_timeout(name: &str, timeout_ms: u64) -> Result<()> {
    if timeout_ms == 0 || timeout_ms > HARD_MAX_TIMEOUT_MS {
        return Err(CoreError::Configuration(format!(
            "{name} must be between 1 and {HARD_MAX_TIMEOUT_MS}"
        )));
    }
    Ok(())
}

fn validate_nonempty_path(name: &str, path: &std::path::Path) -> Result<()> {
    if path.as_os_str().is_empty() {
        return Err(CoreError::Configuration(format!(
            "{name} must not be empty"
        )));
    }
    Ok(())
}

fn validate_json_pointer(name: &str, pointer: &str) -> Result<()> {
    if !pointer.is_empty() && !pointer.starts_with('/') {
        return Err(CoreError::Configuration(format!(
            "{name} must be an RFC 6901 JSON Pointer"
        )));
    }
    let mut characters = pointer.chars();
    while let Some(character) = characters.next() {
        if character == '~' && !matches!(characters.next(), Some('0' | '1')) {
            return Err(CoreError::Configuration(format!(
                "{name} contains an invalid JSON Pointer escape"
            )));
        }
    }
    Ok(())
}

fn validate_gate(gate: &GateConfig) -> Result<()> {
    if gate.min_cases == Some(0) || gate.min_unique_cases == Some(0) {
        return Err(CoreError::Configuration(
            "gate.min_cases and gate.min_unique_cases must be at least 1".to_owned(),
        ));
    }
    for (name, value) in [
        ("max_primary_regression_pp", gate.max_primary_regression_pp),
        (
            "max_valid_but_wrong_increase_pp",
            gate.max_valid_but_wrong_increase_pp,
        ),
    ] {
        if value.is_some_and(|value| !value.is_finite() || value < 0.0) {
            return Err(CoreError::Configuration(format!(
                "gate.{name} must be a finite non-negative number"
            )));
        }
    }
    for (name, value) in [
        ("min_primary_scored_rate", gate.min_primary_scored_rate),
        (
            "max_primary_evaluator_error_rate",
            gate.max_primary_evaluator_error_rate,
        ),
        (
            "max_primary_not_applicable_rate",
            gate.max_primary_not_applicable_rate,
        ),
        ("max_primary_unscored_rate", gate.max_primary_unscored_rate),
        ("max_duplicate_case_rate", gate.max_duplicate_case_rate),
        (
            "min_candidate_schema_validity",
            gate.min_candidate_schema_validity,
        ),
        ("max_error_rate", gate.max_error_rate),
        ("max_timeout_rate", gate.max_timeout_rate),
    ] {
        if value.is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value)) {
            return Err(CoreError::Configuration(format!(
                "gate.{name} must be between 0 and 1"
            )));
        }
    }
    for (name, value) in [
        (
            "latency.max_p95_increase_percent",
            gate.latency
                .as_ref()
                .map(|item| item.max_p95_increase_percent),
        ),
        (
            "cost.max_average_increase_percent",
            gate.cost
                .as_ref()
                .map(|item| item.max_average_increase_percent),
        ),
        (
            "latency.min_coverage",
            gate.latency.as_ref().map(|item| item.min_coverage),
        ),
        (
            "cost.min_coverage",
            gate.cost.as_ref().map(|item| item.min_coverage),
        ),
    ] {
        let is_coverage = name.ends_with("min_coverage");
        if value.is_some_and(|value| {
            !value.is_finite()
                || if is_coverage {
                    !(0.0..=1.0).contains(&value)
                } else {
                    value < 0.0
                }
        }) {
            return Err(CoreError::Configuration(format!(
                "gate.{name} must be {}",
                if is_coverage {
                    "between 0 and 1"
                } else {
                    "a finite non-negative number"
                }
            )));
        }
    }
    Ok(())
}

fn validate_dataset_field_isolation(fields: &DatasetFields) -> Result<()> {
    let protected = [
        ("id", fields.id.as_str()),
        ("input", fields.input.as_str()),
        ("expected", fields.expected.as_str()),
        (
            "model_visible_metadata",
            fields.model_visible_metadata.as_str(),
        ),
        ("metadata", fields.metadata.as_str()),
    ];
    for left in 0..protected.len() {
        for right in (left + 1)..protected.len() {
            let (left_name, left_pointer) = protected[left];
            let (right_name, right_pointer) = protected[right];
            if pointers_overlap(left_pointer, right_pointer) {
                return Err(CoreError::Configuration(format!(
                    "dataset.fields.{left_name} `{left_pointer}` overlaps dataset.fields.{right_name} `{right_pointer}`; model-visible and evaluation-only fields must be disjoint"
                )));
            }
        }
    }
    Ok(())
}

fn validate_evidence_unit(config: &EvidenceUnitConfig) -> Result<()> {
    if config.pointer.is_some() && config.include.is_some() {
        return Err(CoreError::Configuration(
            "dataset.evidence_unit must define either pointer or include, not both".to_owned(),
        ));
    }
    if let Some(pointer) = &config.pointer {
        validate_json_pointer("dataset.evidence_unit.pointer", pointer)?;
    }
    if let Some(include) = &config.include {
        if include.is_empty() {
            return Err(CoreError::Configuration(
                "dataset.evidence_unit.include must not be empty".to_owned(),
            ));
        }
        for pointer in include {
            validate_json_pointer("dataset.evidence_unit.include", pointer)?;
        }
    }
    Ok(())
}

fn pointers_overlap(left: &str, right: &str) -> bool {
    left == right
        || left.is_empty()
        || right.is_empty()
        || left
            .strip_prefix(right)
            .is_some_and(|suffix| suffix.starts_with('/'))
        || right
            .strip_prefix(left)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal() -> Config {
        Config {
            version: 1,
            project: ProjectConfig {
                name: "tickets".to_owned(),
                description: None,
            },
            storage: StorageConfig::default(),
            limits: LimitsConfig::default(),
            dataset: DatasetConfig {
                path: "data.jsonl".into(),
                format: "jsonl".to_owned(),
                fields: DatasetFields::default(),
                evidence_unit: EvidenceUnitConfig::default(),
            },
            schema: SchemaConfig {
                path: "schema.json".into(),
            },
            variants: BTreeMap::from([
                (
                    "baseline".to_owned(),
                    VariantConfig::Recorded {
                        path: "baseline.jsonl".into(),
                    },
                ),
                (
                    "candidate".to_owned(),
                    VariantConfig::Recorded {
                        path: "candidate.jsonl".into(),
                    },
                ),
            ]),
            evaluators: vec![EvaluatorConfig {
                id: "exact".to_owned(),
                implementation_version: None,
                implementation: ImplementationConfig::default(),
                kind: EvaluatorKind::ExactJson,
            }],
            outcomes: BTreeMap::from([(
                "semantic_correct".to_owned(),
                OutcomeConfig {
                    all_of: vec!["exact".to_owned()],
                    any_of: vec![],
                },
            )]),
            analysis: AnalysisConfig {
                primary_outcome: "semantic_correct".to_owned(),
                bootstrap: BootstrapConfig::default(),
            },
            gate: GateConfig::default(),
            report: ReportConfig::default(),
        }
    }

    #[test]
    fn accepts_minimal_configuration() {
        Config::validate(minimal()).unwrap();
    }

    #[test]
    fn shipped_configurations_match_schema_and_runtime_validation() {
        let schema: serde_json::Value =
            serde_json::from_str(include_str!("../../../schemas/structtrace.schema.json")).unwrap();
        let validator = jsonschema::options().build(&schema).unwrap();
        for (name, source) in [
            (
                "recorded",
                include_str!("../../../examples/recorded-output-comparison/structtrace.yaml"),
            ),
            (
                "python",
                include_str!("../../../examples/python-callable/structtrace.yaml"),
            ),
            (
                "command",
                include_str!("../../../examples/command/structtrace.yaml"),
            ),
            (
                "document-extraction",
                include_str!("../../../examples/document-extraction/structtrace.yaml"),
            ),
            (
                "tool-calling",
                include_str!("../../../examples/tool-calling/structtrace.yaml"),
            ),
            (
                "openai-compatible",
                include_str!("../../../examples/openai-compatible/structtrace.yaml"),
            ),
            (
                "support-demo",
                include_str!("../../../demo/support-ticket/structtrace.yaml"),
            ),
            (
                "research-demo",
                include_str!("../../../demo/accepted-research/structtrace.yaml"),
            ),
        ] {
            Config::from_bytes(std::path::Path::new("structtrace.yaml"), source.as_bytes())
                .unwrap_or_else(|error| panic!("{name} failed runtime validation: {error}"));
            let yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(source).unwrap();
            let value = serde_json::to_value(yaml).unwrap();
            let errors = validator
                .iter_errors(&value)
                .map(|error| error.to_string())
                .collect::<Vec<_>>();
            assert!(errors.is_empty(), "{name} failed JSON Schema: {errors:?}");
        }
    }

    #[test]
    fn expected_pointer_cannot_equal_input_pointer() {
        let mut config = minimal();
        config.dataset.fields.input = "/expected".to_owned();
        assert!(
            Config::validate(config)
                .unwrap_err()
                .to_string()
                .contains("overlaps")
        );
    }

    #[test]
    fn id_pointer_cannot_overlap_any_model_or_evaluation_field() {
        for pointer in [
            "/input",
            "/expected",
            "/model_visible_metadata",
            "/metadata",
            "",
        ] {
            let mut config = minimal();
            config.dataset.fields.id = pointer.to_owned();
            assert!(
                Config::validate(config)
                    .unwrap_err()
                    .to_string()
                    .contains("overlaps"),
                "pointer {pointer:?} should be rejected"
            );
        }
    }

    #[test]
    fn evidence_unit_configuration_is_explicit_and_unambiguous() {
        let mut both = minimal();
        both.dataset.evidence_unit.pointer = Some("/metadata/document_id".to_owned());
        both.dataset.evidence_unit.include = Some(vec!["/input".to_owned()]);
        assert!(Config::validate(both).is_err());

        let mut empty = minimal();
        empty.dataset.evidence_unit.include = Some(Vec::new());
        assert!(Config::validate(empty).is_err());
    }

    #[test]
    fn huge_bootstrap_configuration_is_rejected() {
        let mut config = minimal();
        config.analysis.bootstrap.samples = HARD_MAX_BOOTSTRAP_SAMPLES + 1;
        assert!(Config::validate(config).is_err());
    }

    #[test]
    fn unsafe_openai_urls_and_temperatures_are_rejected() {
        for base_url in [
            "file:///etc/passwd",
            "https://user@example.com/v1",
            "https://user:password@example.com/v1",
            "https://example.com/v1?token=secret",
            "https://example.com/v1#secret",
            "https://example.com/v1/chat/completions",
        ] {
            let mut config = minimal();
            config.variants.insert(
                "candidate".to_owned(),
                VariantConfig::OpenaiCompatible(Box::new(OpenAiCompatibleConfig {
                    base_url: base_url.to_owned(),
                    api_key_env: Some("TEST_API_KEY".to_owned()),
                    model: "model".to_owned(),
                    request: OpenAiRequestConfig {
                        system: None,
                        user_template: "{{ input }}".to_owned(),
                        temperature: 0.0,
                        max_output_tokens: 100,
                    },
                    structured_output: None,
                    timeout_ms: 1_000,
                    concurrency: 1,
                    retries: 0,
                    pricing: None,
                })),
            );
            assert!(Config::validate(config).is_err(), "accepted {base_url}");
        }
        let mut config = minimal();
        config.variants.insert(
            "candidate".to_owned(),
            VariantConfig::OpenaiCompatible(Box::new(OpenAiCompatibleConfig {
                base_url: "http://127.0.0.1:8000/v1".to_owned(),
                api_key_env: None,
                model: "model".to_owned(),
                request: OpenAiRequestConfig {
                    system: None,
                    user_template: "{{ input }}".to_owned(),
                    temperature: 2.1,
                    max_output_tokens: 100,
                },
                structured_output: None,
                timeout_ms: 1_000,
                concurrency: 1,
                retries: 0,
                pricing: None,
            })),
        );
        assert!(Config::validate(config).is_err());
    }

    #[test]
    fn ambiguous_date_formats_and_empty_keyed_fields_are_rejected() {
        let mut date = minimal();
        date.evaluators[0].kind = EvaluatorKind::CanonicalDate {
            pointer: "/date".to_owned(),
            expected_pointer: "/date".to_owned(),
            formats: vec!["dmy_slash".to_owned(), "mdy_slash".to_owned()],
        };
        assert!(Config::validate(date).is_err());

        let mut keyed = minimal();
        keyed.evaluators[0].kind = EvaluatorKind::KeyedArray {
            pointer: "/items".to_owned(),
            expected_pointer: "/items".to_owned(),
            keys: vec!["/id".to_owned()],
            fields: Vec::new(),
        };
        assert!(Config::validate(keyed).is_err());
    }

    #[test]
    fn expected_pointer_cannot_equal_model_visible_pointer() {
        let mut config = minimal();
        config.dataset.fields.model_visible_metadata = "/expected".to_owned();
        assert!(
            Config::validate(config)
                .unwrap_err()
                .to_string()
                .contains("overlaps")
        );
    }

    #[test]
    fn root_input_cannot_contain_expected() {
        let mut config = minimal();
        config.dataset.fields.input = String::new();
        assert!(
            Config::validate(config)
                .unwrap_err()
                .to_string()
                .contains("overlaps")
        );
    }

    #[test]
    fn parent_child_pointer_overlap_is_rejected() {
        let mut config = minimal();
        config.dataset.fields.input = "/payload".to_owned();
        config.dataset.fields.expected = "/payload/gold".to_owned();
        assert!(
            Config::validate(config)
                .unwrap_err()
                .to_string()
                .contains("overlaps")
        );
    }

    #[test]
    fn evaluation_metadata_cannot_be_model_visible() {
        let mut config = minimal();
        config.dataset.fields.model_visible_metadata = "/metadata/public".to_owned();
        config.dataset.fields.metadata = "/metadata".to_owned();
        assert!(
            Config::validate(config)
                .unwrap_err()
                .to_string()
                .contains("overlaps")
        );
    }

    #[test]
    fn per_case_external_evaluator_is_refused_from_stable_runtime() {
        let mut config = minimal();
        config.evaluators = vec![EvaluatorConfig {
            id: "external".to_owned(),
            implementation_version: Some("v1".to_owned()),
            implementation: ImplementationConfig::default(),
            kind: EvaluatorKind::Command {
                command: CommandSpec {
                    program: "worker".to_owned(),
                    args: vec![],
                },
                process_mode: ProcessMode::PerCase,
                timeout_ms: 100,
            },
        }];
        config.outcomes.get_mut("semantic_correct").unwrap().all_of = vec!["external".to_owned()];
        let error = Config::validate(config).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("per_case is experimental and refused")
        );
    }

    #[test]
    fn accepts_every_report_filter_emitted_by_case_classification() {
        for filter in ["evaluator_error", "not_applicable", "unscored"] {
            let mut config = minimal();
            config.report.default_case_filter = filter.to_owned();
            Config::validate(config).unwrap();
        }
    }

    #[test]
    fn rejects_missing_candidate() {
        let mut config = minimal();
        config.variants.remove("candidate");
        assert!(Config::validate(config).is_err());
    }

    #[test]
    fn extra_variant_is_rejected() {
        let mut config = minimal();
        config.variants.insert(
            "shadow".to_owned(),
            VariantConfig::Recorded {
                path: "shadow.jsonl".into(),
            },
        );
        let error = Config::validate(config).unwrap_err();
        assert!(error.to_string().contains("unsupported variants: shadow"));
    }

    #[test]
    fn rejects_limits_that_disable_or_exceed_hard_safety_ceiling() {
        let mut zero = minimal();
        zero.limits.max_output_bytes_per_case = 0;
        assert!(Config::validate(zero).is_err());

        let mut excessive = minimal();
        excessive.limits.max_stderr_bytes_per_process = HARD_MAX_STDERR_BYTES_PER_PROCESS + 1;
        assert!(Config::validate(excessive).is_err());
    }

    #[test]
    fn rejects_operational_values_that_bypass_typed_schema_constraints() {
        let mut zero_timeout = minimal();
        zero_timeout.variants.insert(
            "candidate".to_owned(),
            VariantConfig::Command {
                command: CommandSpec {
                    program: "worker".to_owned(),
                    args: vec![],
                },
                process_mode: ProcessMode::Persistent,
                timeout_ms: 0,
                implementation: ImplementationConfig::default(),
            },
        );
        assert!(Config::validate(zero_timeout).is_err());

        let mut malformed_pointer = minimal();
        malformed_pointer.dataset.fields.input = "input".to_owned();
        assert!(Config::validate(malformed_pointer).is_err());

        let mut negative_gate = minimal();
        negative_gate.gate.max_primary_regression_pp = Some(-1.0);
        assert!(Config::validate(negative_gate).is_err());
    }

    #[test]
    fn refuses_schema_validity_as_inferred_semantics() {
        let mut config = minimal();
        config.outcomes.insert(
            "bad".to_owned(),
            OutcomeConfig {
                all_of: vec!["json_schema".to_owned()],
                any_of: vec![],
            },
        );
        config.analysis.primary_outcome = "bad".to_owned();
        assert!(Config::validate(config).is_err());
    }
}

//! Versioned StructTrace configuration.

use std::{collections::BTreeMap, path::PathBuf, str::FromStr};

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::{CoreError, Result, error::read_error};

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
/// Hard ceiling for one configured adapter or evaluator timeout.
pub const HARD_MAX_TIMEOUT_MS: u64 = 24 * 60 * 60 * 1000;
/// Hard ceiling for simultaneous provider requests.
pub const HARD_MAX_CONCURRENCY: usize = 256;
/// Hard ceiling for explicit provider retries.
pub const HARD_MAX_RETRIES: u32 = 20;
/// Hard ceiling for provider-requested generated tokens.
pub const HARD_MAX_OUTPUT_TOKENS: u32 = 1_000_000;

/// Configurable resource limits with conservative defaults and enforced hard ceilings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LimitsConfig {
    /// Maximum model or adapter output bytes retained for one case.
    pub max_output_bytes_per_case: usize,
    /// Maximum standard-error bytes retained from one process.
    pub max_stderr_bytes_per_process: usize,
    /// Maximum raw-output bytes embedded per variant in the HTML report.
    pub max_report_raw_bytes_per_case: usize,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            max_output_bytes_per_case: 4 * 1024 * 1024,
            max_stderr_bytes_per_process: 1024 * 1024,
            max_report_raw_bytes_per_case: 256 * 1024,
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
    },
    /// OpenAI-compatible chat-completions endpoint.
    OpenaiCompatible(Box<OpenAiCompatibleConfig>),
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
    /// Versioned command evaluator.
    Command {
        /// Executable and arguments.
        command: CommandSpec,
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
        /// Per-case timeout.
        #[serde(default = "default_timeout_ms")]
        timeout_ms: u64,
    },
}

fn default_tool_name_pointer() -> String {
    "/name".to_owned()
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
        let bytes = std::fs::read(path).map_err(read_error(path))?;
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
            || !(0.0..1.0).contains(&config.analysis.bootstrap.confidence)
        {
            return Err(CoreError::Configuration(
                "bootstrap samples must be positive and confidence must be between 0 and 1"
                    .to_owned(),
            ));
        }
        for (name, value, maximum) in [
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
        ] {
            if value == 0 || value > maximum {
                return Err(CoreError::Configuration(format!(
                    "{name} must be between 1 and {maximum} bytes"
                )));
            }
        }
        validate_nonempty_path("storage.root", &config.storage.root)?;
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
            ..
        } => {
            validate_command(&format!("{name}.command"), command)?;
            validate_timeout(&format!("{name}.timeout_ms"), *timeout_ms)
        }
        VariantConfig::Python {
            interpreter,
            callable,
            timeout_ms,
        } => {
            validate_python(&format!("{name}.python"), interpreter, callable)?;
            validate_timeout(&format!("{name}.timeout_ms"), *timeout_ms)
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
            if !adapter.request.temperature.is_finite() || adapter.request.temperature < 0.0 {
                return Err(CoreError::Configuration(format!(
                    "{name}.request.temperature must be a finite non-negative number"
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
        | EvaluatorKind::ToolSelection {
            pointer,
            expected_pointer,
        } => {
            validate_json_pointer(&format!("{name}.pointer"), pointer)?;
            validate_json_pointer(&format!("{name}.expected_pointer"), expected_pointer)
        }
        EvaluatorKind::JsonPointersExact { pointers }
        | EvaluatorKind::ToolArguments { pointers } => validate_pointer_pairs(&name, pointers),
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
            timeout_ms,
        } => {
            validate_command(&format!("{name}.command"), command)?;
            validate_timeout(&format!("{name}.timeout_ms"), *timeout_ms)
        }
        EvaluatorKind::Python {
            interpreter,
            callable,
            timeout_ms,
        } => {
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

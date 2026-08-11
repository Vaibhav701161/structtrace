//! Independent practical release-gate rules.

use serde::{Deserialize, Serialize};

use crate::{
    config::{GateConfig, GateMode},
    statistics::PairedMetrics,
};

/// Inputs required by the gate engine.
#[derive(Debug, Clone)]
pub struct GateInputs<'a> {
    /// Complete matched case count.
    pub total_cases: usize,
    /// Distinct canonical semantic evidence units.
    pub unique_cases: usize,
    /// Fraction of rows beyond the first member of each semantic group.
    pub duplicate_case_rate: f64,
    /// Repeated evidence groups unsupported by the independent v1 analysis.
    pub repeated_trial_groups: usize,
    /// Stimuli with incompatible expected references.
    pub label_conflict_groups: usize,
    /// Minimum explicit pass-or-fail scoring rate across both variants.
    pub primary_fully_evaluated_rate: f64,
    /// Maximum primary required-component error rate across both variants.
    pub primary_component_error_rate: f64,
    /// Maximum primary required-component not-applicable rate across both variants.
    pub primary_component_not_applicable_rate: f64,
    /// Maximum primary required-component unscored rate across both variants.
    pub primary_component_unscored_rate: f64,
    /// Primary paired metrics.
    pub primary: &'a PairedMetrics,
    /// Complete-denominator deployment-success paired metrics.
    pub deployment: &'a PairedMetrics,
    /// Baseline valid-but-wrong fraction.
    pub baseline_valid_but_wrong_rate: f64,
    /// Candidate valid-but-wrong fraction.
    pub candidate_valid_but_wrong_rate: f64,
    /// Candidate primary semantic success fraction.
    pub candidate_primary_success_rate: f64,
    /// Candidate complete-denominator deployment-success fraction.
    pub candidate_deployment_success_rate: f64,
    /// Candidate strict-parse validity fraction.
    pub candidate_parse_validity: f64,
    /// Lower bound of the paired candidate-minus-baseline interval.
    pub primary_lower_confidence_bound_pp: Option<f64>,
    /// Lower bound of candidate-minus-baseline deployment-success effect.
    pub deployment_lower_confidence_bound_pp: Option<f64>,
    /// Candidate schema-validity fraction.
    pub candidate_schema_validity: f64,
    /// Candidate adapter-error fraction.
    pub candidate_error_rate: f64,
    /// Candidate timeout fraction.
    pub candidate_timeout_rate: f64,
    /// Optional baseline p95 latency.
    pub baseline_p95_latency_ms: Option<f64>,
    /// Optional candidate p95 latency.
    pub candidate_p95_latency_ms: Option<f64>,
    /// Fraction of all pairs included in the matched latency comparison.
    pub latency_coverage: f64,
    /// Optional baseline average cost.
    pub baseline_average_cost: Option<f64>,
    /// Optional candidate average cost.
    pub candidate_average_cost: Option<f64>,
    /// Fraction of all pairs included in the matched cost comparison.
    pub cost_coverage: f64,
}

/// State of one release-gate rule.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GateRuleStatus {
    /// The configured rule was evaluated and passed.
    Passed,
    /// The configured quality rule was evaluated and failed.
    Failed,
    /// The rule was not configured.
    NotConfigured,
    /// Required evidence was absent or below its declared minimum.
    InsufficientEvidence,
    /// The rule could not be evaluated because of an internal error.
    Error,
}

/// One transparent gate-rule result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GateRuleResult {
    /// Stable rule name.
    pub rule: String,
    /// Explicit rule state.
    pub status: GateRuleStatus,
    /// Observed value in the rule's documented unit.
    pub observed: Option<f64>,
    /// Configured threshold.
    pub threshold: Option<f64>,
    /// Exact explanation.
    pub message: String,
}

/// State of the complete release-gate decision.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GateStatus {
    /// Every configured quality and evidence rule passed.
    Passed,
    /// Evidence was sufficient and at least one quality rule failed.
    Failed,
    /// No release criteria were configured.
    NotConfigured,
    /// A release decision was requested without enough scored evidence.
    InsufficientEvidence,
    /// A gate rule encountered an internal evaluation error.
    Error,
}

impl GateStatus {
    /// Stable uppercase label for human output.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Passed => "PASSED",
            Self::Failed => "FAILED",
            Self::NotConfigured => "NOT CONFIGURED",
            Self::InsufficientEvidence => "INSUFFICIENT EVIDENCE",
            Self::Error => "ERROR",
        }
    }

    /// Whether all configured rules passed, independent of gate authority.
    pub const fn is_passed(self) -> bool {
        matches!(self, Self::Passed)
    }
}

/// Complete release-gate decision.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GateDecision {
    /// Authority of the gate that produced this decision.
    pub gate_mode: GateMode,
    /// Explicit deployment-decision state.
    pub status: GateStatus,
    /// True only when every configured rule passed in `release` mode.
    pub deployment_authorized: bool,
    /// Configured quality rules that failed.
    pub quality_failures: Vec<String>,
    /// Evidence requirements that were insufficient.
    pub evidence_failures: Vec<String>,
    /// Rules that could not be evaluated safely.
    pub runtime_errors: Vec<String>,
    /// Independent rule results.
    pub rules: Vec<GateRuleResult>,
}

/// Evaluate all configured rules without allowing one metric to hide another.
pub fn evaluate_gate(config: &GateConfig, inputs: &GateInputs<'_>) -> GateDecision {
    if !config.is_configured() {
        return GateDecision {
            gate_mode: config.mode,
            status: GateStatus::NotConfigured,
            deployment_authorized: false,
            quality_failures: Vec::new(),
            evidence_failures: Vec::new(),
            runtime_errors: Vec::new(),
            rules: Vec::new(),
        };
    }
    let mut rules = vec![
        repeated_trials_rule(inputs.repeated_trial_groups),
        label_conflicts_rule(inputs.label_conflict_groups),
        required_minimum_rule(
            "min_cases",
            config.min_cases.map(|value| value as f64),
            inputs.total_cases as f64,
            "paired case count",
        ),
        required_minimum_rule(
            "min_unique_cases",
            config.min_unique_cases.map(|value| value as f64),
            inputs.unique_cases as f64,
            "unique semantic case count",
        ),
        required_maximum_rule(
            "max_duplicate_case_rate",
            config.max_duplicate_case_rate,
            inputs.duplicate_case_rate,
            "exact semantic duplicate rate",
        ),
        required_minimum_rule(
            "min_primary_fully_evaluated_rate",
            config.min_primary_fully_evaluated_rate,
            inputs.primary_fully_evaluated_rate,
            "primary fully evaluated rate",
        ),
        required_maximum_rule(
            "max_primary_component_error_rate",
            config.max_primary_component_error_rate,
            inputs.primary_component_error_rate,
            "primary required-component error rate",
        ),
        required_maximum_rule(
            "max_primary_component_not_applicable_rate",
            config.max_primary_component_not_applicable_rate,
            inputs.primary_component_not_applicable_rate,
            "primary required-component not-applicable rate",
        ),
        required_maximum_rule(
            "max_primary_component_unscored_rate",
            config.max_primary_component_unscored_rate,
            inputs.primary_component_unscored_rate,
            "primary required-component unscored rate",
        ),
        optional_maximum_rule(
            "max_primary_regression_pp",
            config.max_primary_regression_pp,
            inputs
                .primary
                .difference_pp
                .map(|effect| (-effect).max(0.0)),
            "primary outcome regression",
        ),
        optional_maximum_rule(
            "max_deployment_regression_pp",
            config.max_deployment_regression_pp,
            inputs
                .deployment
                .difference_pp
                .map(|effect| (-effect).max(0.0)),
            "deployment-success regression",
        ),
        maximum_rule(
            "max_valid_but_wrong_increase_pp",
            config.max_valid_but_wrong_increase_pp,
            100.0
                * (inputs.candidate_valid_but_wrong_rate - inputs.baseline_valid_but_wrong_rate)
                    .max(0.0),
            "valid-but-wrong increase",
        ),
        minimum_rule(
            "min_candidate_primary_success_rate",
            config.min_candidate_primary_success_rate,
            inputs.candidate_primary_success_rate,
            "candidate primary success rate",
        ),
        minimum_rule(
            "min_candidate_deployment_success_rate",
            config.min_candidate_deployment_success_rate,
            inputs.candidate_deployment_success_rate,
            "candidate deployment-success rate",
        ),
        maximum_rule(
            "max_candidate_valid_but_wrong_rate",
            config.max_candidate_valid_but_wrong_rate,
            inputs.candidate_valid_but_wrong_rate,
            "candidate valid-but-wrong rate",
        ),
        minimum_rule(
            "min_candidate_parse_validity",
            config.min_candidate_parse_validity,
            inputs.candidate_parse_validity,
            "candidate strict-parse validity",
        ),
        optional_maximum_rule(
            "max_upper_confidence_bound_regression_pp",
            config.max_upper_confidence_bound_regression_pp,
            inputs
                .primary_lower_confidence_bound_pp
                .map(|lower| (-lower).max(0.0)),
            "upper confidence bound on primary regression",
        ),
        optional_maximum_rule(
            "max_upper_confidence_bound_deployment_regression_pp",
            config.max_upper_confidence_bound_deployment_regression_pp,
            inputs
                .deployment_lower_confidence_bound_pp
                .map(|lower| (-lower).max(0.0)),
            "upper confidence bound on deployment-success regression",
        ),
        minimum_rule(
            "min_discordant_pairs",
            config.min_discordant_pairs.map(|value| value as f64),
            (inputs.primary.baseline_only_pass + inputs.primary.candidate_only_pass) as f64,
            "primary discordant pair count",
        ),
        minimum_rule(
            "min_candidate_schema_validity",
            config.min_candidate_schema_validity,
            inputs.candidate_schema_validity,
            "candidate schema validity",
        ),
        maximum_rule(
            "max_error_rate",
            config.max_error_rate,
            inputs.candidate_error_rate,
            "candidate error rate",
        ),
        maximum_rule(
            "max_timeout_rate",
            config.max_timeout_rate,
            inputs.candidate_timeout_rate,
            "candidate timeout rate",
        ),
    ];
    let latency_threshold = config
        .latency
        .as_ref()
        .map(|item| item.max_p95_increase_percent);
    let latency_min_coverage = config.latency.as_ref().map(|item| item.min_coverage);
    rules.push(minimum_rule(
        "min_matched_latency_coverage",
        latency_min_coverage,
        inputs.latency_coverage,
        "matched latency coverage",
    ));
    rules.push(relative_increase_rule(
        "max_p95_latency_increase_percent",
        latency_threshold,
        inputs.baseline_p95_latency_ms,
        inputs.candidate_p95_latency_ms,
        "p95 latency increase",
    ));
    let cost_threshold = config
        .cost
        .as_ref()
        .map(|item| item.max_average_increase_percent);
    let cost_min_coverage = config.cost.as_ref().map(|item| item.min_coverage);
    rules.push(minimum_rule(
        "min_matched_cost_coverage",
        cost_min_coverage,
        inputs.cost_coverage,
        "matched cost coverage",
    ));
    rules.push(relative_increase_rule(
        "max_average_cost_increase_percent",
        cost_threshold,
        inputs.baseline_average_cost,
        inputs.candidate_average_cost,
        "average cost increase",
    ));
    let status = if rules
        .iter()
        .any(|rule| rule.status == GateRuleStatus::Error)
    {
        GateStatus::Error
    } else if rules
        .iter()
        .any(|rule| rule.status == GateRuleStatus::InsufficientEvidence)
    {
        GateStatus::InsufficientEvidence
    } else if rules
        .iter()
        .any(|rule| rule.status == GateRuleStatus::Failed)
    {
        GateStatus::Failed
    } else {
        GateStatus::Passed
    };
    let quality_failures = rules
        .iter()
        .filter(|rule| rule.status == GateRuleStatus::Failed)
        .map(|rule| rule.rule.clone())
        .collect();
    let evidence_failures = rules
        .iter()
        .filter(|rule| rule.status == GateRuleStatus::InsufficientEvidence)
        .map(|rule| rule.rule.clone())
        .collect();
    let runtime_errors = rules
        .iter()
        .filter(|rule| rule.status == GateRuleStatus::Error)
        .map(|rule| rule.rule.clone())
        .collect();
    GateDecision {
        gate_mode: config.mode,
        status,
        deployment_authorized: status == GateStatus::Passed && config.mode == GateMode::Release,
        quality_failures,
        evidence_failures,
        runtime_errors,
        rules,
    }
}

fn repeated_trials_rule(groups: usize) -> GateRuleResult {
    GateRuleResult {
        rule: "repeated_trial_model".to_owned(),
        status: if groups == 0 {
            GateRuleStatus::Passed
        } else {
            GateRuleStatus::InsufficientEvidence
        },
        observed: Some(groups as f64),
        threshold: Some(0.0),
        message: if groups == 0 {
            "No evidence unit contains unsupported repeated trials.".to_owned()
        } else {
            format!(
                "{groups} evidence unit(s) contain repeated trials; no row was selected arbitrarily and v1 independent inference is unavailable for those groups."
            )
        },
    }
}

fn label_conflicts_rule(groups: usize) -> GateRuleResult {
    GateRuleResult {
        rule: "dataset_label_conflicts".to_owned(),
        status: if groups == 0 {
            GateRuleStatus::Passed
        } else {
            GateRuleStatus::Error
        },
        observed: Some(groups as f64),
        threshold: Some(0.0),
        message: if groups == 0 {
            "No model-visible stimulus maps to incompatible expected references.".to_owned()
        } else {
            format!("{groups} stimulus group(s) have incompatible expected references.")
        },
    }
}

fn maximum_rule(name: &str, threshold: Option<f64>, observed: f64, label: &str) -> GateRuleResult {
    threshold.map_or_else(
        || not_evaluated(name, format!("No {label} threshold was configured.")),
        |limit| {
            let passed = observed <= limit;
            GateRuleResult {
                rule: name.to_owned(),
                status: if passed {
                    GateRuleStatus::Passed
                } else {
                    GateRuleStatus::Failed
                },
                observed: Some(observed),
                threshold: Some(limit),
                message: format!(
                    "Observed {label} was {observed:.3}; maximum allowed is {limit:.3}."
                ),
            }
        },
    )
}

fn minimum_rule(name: &str, threshold: Option<f64>, observed: f64, label: &str) -> GateRuleResult {
    threshold.map_or_else(
        || not_evaluated(name, format!("No {label} threshold was configured.")),
        |limit| {
            let passed = observed >= limit;
            GateRuleResult {
                rule: name.to_owned(),
                status: if passed {
                    GateRuleStatus::Passed
                } else {
                    GateRuleStatus::Failed
                },
                observed: Some(observed),
                threshold: Some(limit),
                message: format!(
                    "Observed {label} was {observed:.6}; minimum required is {limit:.6}."
                ),
            }
        },
    )
}

fn optional_maximum_rule(
    name: &str,
    threshold: Option<f64>,
    observed: Option<f64>,
    label: &str,
) -> GateRuleResult {
    let Some(limit) = threshold else {
        return not_evaluated(name, format!("No {label} threshold was configured."));
    };
    observed.map_or_else(
        || {
            unavailable_failure(
                name,
                limit,
                format!("{label} was configured but no paired effect estimate was available."),
            )
        },
        |value| maximum_rule(name, Some(limit), value, label),
    )
}

fn relative_increase_rule(
    name: &str,
    threshold: Option<f64>,
    baseline: Option<f64>,
    candidate: Option<f64>,
    label: &str,
) -> GateRuleResult {
    let Some(limit) = threshold else {
        return not_evaluated(name, format!("No {label} threshold was configured."));
    };
    let (Some(baseline), Some(candidate)) = (baseline, candidate) else {
        return unavailable_failure(
            name,
            limit,
            format!("{label} was configured but could not be computed from retained data."),
        );
    };
    if baseline == 0.0 {
        return unavailable_failure(
            name,
            limit,
            format!("{label} was configured but is undefined because the baseline is zero."),
        );
    }
    let observed = 100.0 * (candidate - baseline) / baseline;
    maximum_rule(name, Some(limit), observed, label)
}

fn unavailable_failure(name: &str, threshold: f64, message: String) -> GateRuleResult {
    GateRuleResult {
        rule: name.to_owned(),
        status: GateRuleStatus::InsufficientEvidence,
        observed: None,
        threshold: Some(threshold),
        message,
    }
}

fn not_evaluated(name: &str, message: String) -> GateRuleResult {
    GateRuleResult {
        rule: name.to_owned(),
        status: GateRuleStatus::NotConfigured,
        observed: None,
        threshold: None,
        message,
    }
}

fn required_minimum_rule(
    name: &str,
    threshold: Option<f64>,
    observed: f64,
    label: &str,
) -> GateRuleResult {
    match threshold {
        Some(limit) => {
            let mut rule = minimum_rule(name, Some(limit), observed, label);
            if rule.status == GateRuleStatus::Failed {
                rule.status = GateRuleStatus::InsufficientEvidence;
            }
            rule
        }
        None => missing_evidence_requirement(name, label),
    }
}

fn required_maximum_rule(
    name: &str,
    threshold: Option<f64>,
    observed: f64,
    label: &str,
) -> GateRuleResult {
    match threshold {
        Some(limit) => {
            let mut rule = maximum_rule(name, Some(limit), observed, label);
            if rule.status == GateRuleStatus::Failed {
                rule.status = GateRuleStatus::InsufficientEvidence;
            }
            rule
        }
        None => missing_evidence_requirement(name, label),
    }
}

fn missing_evidence_requirement(name: &str, label: &str) -> GateRuleResult {
    GateRuleResult {
        rule: name.to_owned(),
        status: GateRuleStatus::InsufficientEvidence,
        observed: None,
        threshold: None,
        message: format!(
            "A {label} requirement must be configured before StructTrace can make a release decision."
        ),
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        config::{GateConfig, GateMode},
        statistics::paired_metrics,
    };

    use super::*;

    fn release_config() -> GateConfig {
        GateConfig {
            mode: GateMode::Release,
            min_cases: Some(2),
            min_unique_cases: Some(2),
            max_duplicate_case_rate: Some(0.0),
            min_primary_fully_evaluated_rate: Some(1.0),
            max_primary_component_error_rate: Some(0.0),
            max_primary_component_not_applicable_rate: Some(0.0),
            max_primary_component_unscored_rate: Some(0.0),
            max_primary_regression_pp: Some(1.0),
            min_candidate_primary_success_rate: Some(0.5),
            ..GateConfig::default()
        }
    }

    fn inputs<'a>(primary: &'a PairedMetrics) -> GateInputs<'a> {
        GateInputs {
            total_cases: primary.total,
            unique_cases: primary.total,
            duplicate_case_rate: 0.0,
            repeated_trial_groups: 0,
            label_conflict_groups: 0,
            primary_fully_evaluated_rate: 1.0,
            primary_component_error_rate: 0.0,
            primary_component_not_applicable_rate: 0.0,
            primary_component_unscored_rate: 0.0,
            primary,
            deployment: primary,
            baseline_valid_but_wrong_rate: 0.0,
            candidate_valid_but_wrong_rate: 0.0,
            candidate_primary_success_rate: 1.0,
            candidate_deployment_success_rate: 1.0,
            candidate_parse_validity: 1.0,
            primary_lower_confidence_bound_pp: Some(0.0),
            deployment_lower_confidence_bound_pp: Some(0.0),
            candidate_schema_validity: 1.0,
            candidate_error_rate: 0.0,
            candidate_timeout_rate: 0.0,
            baseline_p95_latency_ms: None,
            candidate_p95_latency_ms: None,
            latency_coverage: 0.0,
            baseline_average_cost: None,
            candidate_average_cost: None,
            cost_coverage: 0.0,
        }
    }

    #[test]
    fn empty_gate_is_not_pass() {
        let primary = paired_metrics(&[(true, true)]);
        let decision = evaluate_gate(&GateConfig::default(), &inputs(&primary));
        assert_eq!(decision.status, GateStatus::NotConfigured);
        assert!(decision.rules.is_empty());
    }

    #[test]
    fn semantic_failure_is_not_hidden_by_perfect_schema_validity() {
        let primary = paired_metrics(&[(true, false), (true, true)]);
        let decision = evaluate_gate(&release_config(), &inputs(&primary));
        assert_eq!(decision.status, GateStatus::Failed);
        assert_eq!(
            decision
                .rules
                .iter()
                .find(|rule| rule.rule == "max_primary_regression_pp")
                .unwrap()
                .status,
            GateRuleStatus::Failed
        );
    }

    #[test]
    fn all_evaluator_errors_cannot_pass_gate() {
        let primary = paired_metrics(&[(true, true), (true, true)]);
        let mut evidence = inputs(&primary);
        evidence.primary_fully_evaluated_rate = 0.0;
        evidence.primary_component_error_rate = 1.0;
        let decision = evaluate_gate(&release_config(), &evidence);
        assert_eq!(decision.status, GateStatus::InsufficientEvidence);
    }

    #[test]
    fn low_scoring_coverage_is_insufficient_evidence() {
        let primary = paired_metrics(&[(true, true), (false, false)]);
        let mut config = release_config();
        config.min_primary_fully_evaluated_rate = Some(0.99);
        let mut evidence = inputs(&primary);
        evidence.primary_fully_evaluated_rate = 0.5;
        evidence.primary_component_unscored_rate = 0.5;
        let decision = evaluate_gate(&config, &evidence);
        assert_eq!(decision.status, GateStatus::InsufficientEvidence);
    }

    #[test]
    fn not_applicable_cases_do_not_disappear() {
        let primary = paired_metrics(&[(true, true), (false, false)]);
        let mut evidence = inputs(&primary);
        evidence.primary_fully_evaluated_rate = 0.5;
        evidence.primary_component_not_applicable_rate = 0.5;
        let decision = evaluate_gate(&release_config(), &evidence);
        assert_eq!(decision.status, GateStatus::InsufficientEvidence);
    }

    #[test]
    fn minimum_case_count_is_enforced() {
        let primary = paired_metrics(&[(true, true), (true, true)]);
        let mut config = release_config();
        config.min_cases = Some(100);
        let decision = evaluate_gate(&config, &inputs(&primary));
        assert_eq!(decision.status, GateStatus::InsufficientEvidence);
    }

    #[test]
    fn insufficient_evidence_prevents_a_quality_verdict() {
        let primary = paired_metrics(&[(true, false), (true, true)]);
        let mut config = release_config();
        config.min_cases = Some(100);
        let decision = evaluate_gate(&config, &inputs(&primary));
        assert_eq!(decision.status, GateStatus::InsufficientEvidence);
        assert!(decision.rules.iter().any(|rule| {
            rule.rule == "min_cases" && rule.status == GateRuleStatus::InsufficientEvidence
        }));
        assert!(decision.rules.iter().any(|rule| {
            rule.rule == "max_primary_regression_pp" && rule.status == GateRuleStatus::Failed
        }));
    }

    #[test]
    fn advisory_mode_never_authorizes_and_evidence_only_gate_cannot_authorize() {
        let primary = paired_metrics(&[(true, true), (false, false)]);
        let mut config = release_config();
        config.mode = GateMode::Advisory;
        config.max_primary_regression_pp = None;
        config.min_candidate_primary_success_rate = None;
        let decision = evaluate_gate(&config, &inputs(&primary));
        assert!(!decision.deployment_authorized);
    }

    #[test]
    fn regression_mode_does_not_claim_absolute_safety() {
        let primary = paired_metrics(&[(true, true), (true, true)]);
        let mut config = release_config();
        config.mode = GateMode::Regression;
        config.min_candidate_primary_success_rate = None;
        let decision = evaluate_gate(&config, &inputs(&primary));
        assert_eq!(decision.status, GateStatus::Passed);
        assert!(!decision.deployment_authorized);
    }

    #[test]
    fn zero_percent_baseline_and_candidate_cannot_pass_release_gate() {
        let primary = paired_metrics(&[(false, false), (false, false)]);
        let mut observed = inputs(&primary);
        observed.candidate_primary_success_rate = 0.0;
        let decision = evaluate_gate(&release_config(), &observed);
        assert_eq!(decision.status, GateStatus::Failed);
        assert!(!decision.deployment_authorized);
        assert!(
            decision
                .quality_failures
                .contains(&"min_candidate_primary_success_rate".to_owned())
        );
    }

    #[test]
    fn wide_harm_interval_can_block_release_when_configured() {
        let primary = paired_metrics(&[(true, true), (false, false)]);
        let mut config = release_config();
        config.max_upper_confidence_bound_regression_pp = Some(2.0);
        let mut observed = inputs(&primary);
        observed.primary_lower_confidence_bound_pp = Some(-12.0);
        let decision = evaluate_gate(&config, &observed);
        assert_eq!(decision.status, GateStatus::Failed);
        assert!(!decision.deployment_authorized);
    }
}

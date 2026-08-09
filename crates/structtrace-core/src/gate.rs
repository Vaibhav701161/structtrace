//! Independent practical release-gate rules.

use serde::{Deserialize, Serialize};

use crate::{config::GateConfig, statistics::PairedMetrics};

/// Inputs required by the gate engine.
#[derive(Debug, Clone)]
pub struct GateInputs<'a> {
    /// Complete matched case count.
    pub total_cases: usize,
    /// Distinct canonical semantic evidence units.
    pub unique_cases: usize,
    /// Fraction of rows beyond the first member of each semantic group.
    pub duplicate_case_rate: f64,
    /// Repeated evidence groups with contradictory retained observations.
    pub conflicting_repeated_groups: usize,
    /// Minimum explicit pass-or-fail scoring rate across both variants.
    pub primary_scored_rate: f64,
    /// Maximum primary evaluator-error rate across both variants.
    pub primary_evaluator_error_rate: f64,
    /// Maximum primary not-applicable rate across both variants.
    pub primary_not_applicable_rate: f64,
    /// Maximum primary unscored rate across both variants.
    pub primary_unscored_rate: f64,
    /// Primary paired metrics.
    pub primary: &'a PairedMetrics,
    /// Baseline valid-but-wrong fraction.
    pub baseline_valid_but_wrong_rate: f64,
    /// Candidate valid-but-wrong fraction.
    pub candidate_valid_but_wrong_rate: f64,
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

    /// Whether this state authorizes deployment.
    pub const fn is_passed(self) -> bool {
        matches!(self, Self::Passed)
    }
}

/// Complete release-gate decision.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GateDecision {
    /// Explicit deployment-decision state.
    pub status: GateStatus,
    /// Independent rule results.
    pub rules: Vec<GateRuleResult>,
}

/// Evaluate all configured rules without allowing one metric to hide another.
pub fn evaluate_gate(config: &GateConfig, inputs: &GateInputs<'_>) -> GateDecision {
    let gate_configured = config.min_cases.is_some()
        || config.min_unique_cases.is_some()
        || config.max_duplicate_case_rate.is_some()
        || config.min_primary_scored_rate.is_some()
        || config.max_primary_evaluator_error_rate.is_some()
        || config.max_primary_not_applicable_rate.is_some()
        || config.max_primary_unscored_rate.is_some()
        || config.max_primary_regression_pp.is_some()
        || config.max_valid_but_wrong_increase_pp.is_some()
        || config.min_candidate_schema_validity.is_some()
        || config.max_error_rate.is_some()
        || config.max_timeout_rate.is_some()
        || config.latency.is_some()
        || config.cost.is_some();
    if !gate_configured {
        return GateDecision {
            status: GateStatus::NotConfigured,
            rules: Vec::new(),
        };
    }
    let mut rules = vec![
        conflict_free_rule(inputs.conflicting_repeated_groups),
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
            "min_primary_scored_rate",
            config.min_primary_scored_rate,
            inputs.primary_scored_rate,
            "primary scored rate",
        ),
        required_maximum_rule(
            "max_primary_evaluator_error_rate",
            config.max_primary_evaluator_error_rate,
            inputs.primary_evaluator_error_rate,
            "primary evaluator error rate",
        ),
        required_maximum_rule(
            "max_primary_not_applicable_rate",
            config.max_primary_not_applicable_rate,
            inputs.primary_not_applicable_rate,
            "primary not-applicable rate",
        ),
        required_maximum_rule(
            "max_primary_unscored_rate",
            config.max_primary_unscored_rate,
            inputs.primary_unscored_rate,
            "primary unscored rate",
        ),
        maximum_rule(
            "max_primary_regression_pp",
            config.max_primary_regression_pp,
            (-inputs.primary.difference_pp).max(0.0),
            "primary outcome regression",
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
    GateDecision { status, rules }
}

fn conflict_free_rule(conflicts: usize) -> GateRuleResult {
    GateRuleResult {
        rule: "conflicting_repeated_evidence".to_owned(),
        status: if conflicts == 0 {
            GateRuleStatus::Passed
        } else {
            GateRuleStatus::InsufficientEvidence
        },
        observed: Some(conflicts as f64),
        threshold: Some(0.0),
        message: if conflicts == 0 {
            "No repeated evidence unit had conflicting retained outcomes.".to_owned()
        } else {
            format!(
                "{conflicts} repeated evidence unit(s) contain conflicting observations; no row was selected arbitrarily."
            )
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
    use crate::{config::GateConfig, statistics::paired_metrics};

    use super::*;

    fn release_config() -> GateConfig {
        GateConfig {
            min_cases: Some(2),
            min_unique_cases: Some(2),
            max_duplicate_case_rate: Some(0.0),
            min_primary_scored_rate: Some(1.0),
            max_primary_evaluator_error_rate: Some(0.0),
            max_primary_not_applicable_rate: Some(0.0),
            max_primary_unscored_rate: Some(0.0),
            max_primary_regression_pp: Some(1.0),
            ..GateConfig::default()
        }
    }

    fn inputs<'a>(primary: &'a PairedMetrics) -> GateInputs<'a> {
        GateInputs {
            total_cases: primary.total,
            unique_cases: primary.total,
            duplicate_case_rate: 0.0,
            conflicting_repeated_groups: 0,
            primary_scored_rate: 1.0,
            primary_evaluator_error_rate: 0.0,
            primary_not_applicable_rate: 0.0,
            primary_unscored_rate: 0.0,
            primary,
            baseline_valid_but_wrong_rate: 0.0,
            candidate_valid_but_wrong_rate: 0.0,
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
        evidence.primary_scored_rate = 0.0;
        evidence.primary_evaluator_error_rate = 1.0;
        let decision = evaluate_gate(&release_config(), &evidence);
        assert_eq!(decision.status, GateStatus::InsufficientEvidence);
    }

    #[test]
    fn low_scoring_coverage_is_insufficient_evidence() {
        let primary = paired_metrics(&[(true, true), (false, false)]);
        let mut config = release_config();
        config.min_primary_scored_rate = Some(0.99);
        let mut evidence = inputs(&primary);
        evidence.primary_scored_rate = 0.5;
        evidence.primary_unscored_rate = 0.5;
        let decision = evaluate_gate(&config, &evidence);
        assert_eq!(decision.status, GateStatus::InsufficientEvidence);
    }

    #[test]
    fn not_applicable_cases_do_not_disappear() {
        let primary = paired_metrics(&[(true, true), (false, false)]);
        let mut evidence = inputs(&primary);
        evidence.primary_scored_rate = 0.5;
        evidence.primary_not_applicable_rate = 0.5;
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
}

//! Independent practical release-gate rules.

use serde::{Deserialize, Serialize};

use crate::{config::GateConfig, statistics::PairedMetrics};

/// Inputs required by the gate engine.
#[derive(Debug, Clone)]
pub struct GateInputs<'a> {
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
    /// Optional baseline average cost.
    pub baseline_average_cost: Option<f64>,
    /// Optional candidate average cost.
    pub candidate_average_cost: Option<f64>,
}

/// One transparent gate-rule result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GateRuleResult {
    /// Stable rule name.
    pub rule: String,
    /// Whether the rule was evaluated.
    pub evaluated: bool,
    /// Whether it passed. `None` means not evaluated.
    pub passed: Option<bool>,
    /// Observed value in the rule's documented unit.
    pub observed: Option<f64>,
    /// Configured threshold.
    pub threshold: Option<f64>,
    /// Exact explanation.
    pub message: String,
}

/// Complete release-gate decision.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GateDecision {
    /// True only when every evaluated rule passed.
    pub passed: bool,
    /// Independent rule results.
    pub rules: Vec<GateRuleResult>,
}

/// Evaluate all configured rules without allowing one metric to hide another.
pub fn evaluate_gate(config: &GateConfig, inputs: &GateInputs<'_>) -> GateDecision {
    let mut rules = vec![
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
    rules.push(relative_increase_rule(
        "max_average_cost_increase_percent",
        cost_threshold,
        inputs.baseline_average_cost,
        inputs.candidate_average_cost,
        "average cost increase",
    ));
    GateDecision {
        passed: rules.iter().all(|rule| rule.passed.unwrap_or(true)),
        rules,
    }
}

fn maximum_rule(name: &str, threshold: Option<f64>, observed: f64, label: &str) -> GateRuleResult {
    threshold.map_or_else(
        || not_evaluated(name, format!("No {label} threshold was configured.")),
        |limit| {
            let passed = observed <= limit;
            GateRuleResult {
                rule: name.to_owned(),
                evaluated: true,
                passed: Some(passed),
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
                evaluated: true,
                passed: Some(passed),
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
        evaluated: true,
        passed: Some(false),
        observed: None,
        threshold: Some(threshold),
        message,
    }
}

fn not_evaluated(name: &str, message: String) -> GateRuleResult {
    GateRuleResult {
        rule: name.to_owned(),
        evaluated: false,
        passed: None,
        observed: None,
        threshold: None,
        message,
    }
}

#[cfg(test)]
mod tests {
    use crate::{config::GateConfig, statistics::paired_metrics};

    use super::*;

    #[test]
    fn semantic_failure_is_not_hidden_by_perfect_schema_validity() {
        let primary = paired_metrics(&[(true, false), (true, true)]);
        let config = GateConfig {
            max_primary_regression_pp: Some(1.0),
            min_candidate_schema_validity: Some(1.0),
            ..GateConfig::default()
        };
        let decision = evaluate_gate(
            &config,
            &GateInputs {
                primary: &primary,
                baseline_valid_but_wrong_rate: 0.0,
                candidate_valid_but_wrong_rate: 0.5,
                candidate_schema_validity: 1.0,
                candidate_error_rate: 0.0,
                candidate_timeout_rate: 0.0,
                baseline_p95_latency_ms: None,
                candidate_p95_latency_ms: None,
                baseline_average_cost: None,
                candidate_average_cost: None,
            },
        );
        assert!(!decision.passed);
        assert_eq!(decision.rules[0].passed, Some(false));
        assert_eq!(decision.rules[2].passed, Some(true));
    }
}

//! Paired transition counts, exact McNemar testing, and seeded bootstrap intervals.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

use crate::{CoreError, Result};

/// Complete paired binary transition matrix and effect.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PairedMetrics {
    /// Complete matched denominator.
    pub total: usize,
    /// Baseline passes.
    pub baseline_pass: usize,
    /// Candidate passes.
    pub candidate_pass: usize,
    /// Both variants pass.
    pub both_pass: usize,
    /// Baseline passes and candidate fails.
    pub baseline_only_pass: usize,
    /// Baseline fails and candidate passes.
    pub candidate_only_pass: usize,
    /// Both variants fail.
    pub both_fail: usize,
    /// Candidate minus baseline percentage points.
    pub difference_pp: f64,
    /// Exact two-sided McNemar p-value.
    pub mcnemar_exact_p: f64,
}

/// Deterministic percentile interval for paired percentage-point difference.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BootstrapInterval {
    /// Lower percentile bound in percentage points.
    pub lower_pp: f64,
    /// Upper percentile bound in percentage points.
    pub upper_pp: f64,
    /// Coverage probability.
    pub confidence: f64,
    /// Resample count.
    pub samples: usize,
    /// ChaCha8 seed.
    pub seed: u64,
}

/// Calculate complete-denominator paired metrics.
pub fn paired_metrics(pairs: &[(bool, bool)]) -> PairedMetrics {
    let mut both_pass = 0;
    let mut baseline_only_pass = 0;
    let mut candidate_only_pass = 0;
    let mut both_fail = 0;
    for &(baseline, candidate) in pairs {
        match (baseline, candidate) {
            (true, true) => both_pass += 1,
            (true, false) => baseline_only_pass += 1,
            (false, true) => candidate_only_pass += 1,
            (false, false) => both_fail += 1,
        }
    }
    let total = pairs.len();
    let baseline_pass = both_pass + baseline_only_pass;
    let candidate_pass = both_pass + candidate_only_pass;
    let difference_pp = if total == 0 {
        0.0
    } else {
        100.0 * (candidate_pass as f64 - baseline_pass as f64) / total as f64
    };
    PairedMetrics {
        total,
        baseline_pass,
        candidate_pass,
        both_pass,
        baseline_only_pass,
        candidate_only_pass,
        both_fail,
        difference_pp,
        mcnemar_exact_p: exact_mcnemar_p(baseline_only_pass, candidate_only_pass),
    }
}

/// Exact two-sided binomial McNemar p-value.
pub fn exact_mcnemar_p(baseline_only: usize, candidate_only: usize) -> f64 {
    let discordant = baseline_only + candidate_only;
    if discordant == 0 {
        return 1.0;
    }
    let lower = baseline_only.min(candidate_only);
    let ln_two = std::f64::consts::LN_2;
    let mut log_term = -(discordant as f64) * ln_two;
    let mut log_sum = log_term;
    for index in 1..=lower {
        log_term += ((discordant - index + 1) as f64).ln() - (index as f64).ln();
        log_sum = log_add_exp(log_sum, log_term);
    }
    (2.0 * log_sum.exp()).min(1.0)
}

fn log_add_exp(left: f64, right: f64) -> f64 {
    let maximum = left.max(right);
    if maximum.is_infinite() {
        maximum
    } else {
        maximum + ((left - maximum).exp() + (right - maximum).exp()).ln()
    }
}

/// Calculate a seeded paired percentile bootstrap interval.
pub fn paired_bootstrap(
    pairs: &[(bool, bool)],
    samples: usize,
    confidence: f64,
    seed: u64,
) -> Result<BootstrapInterval> {
    if pairs.is_empty() {
        return Err(CoreError::Statistics(
            "paired bootstrap requires at least one case".to_owned(),
        ));
    }
    if samples == 0 || !(0.0..1.0).contains(&confidence) {
        return Err(CoreError::Statistics(
            "samples must be positive and confidence must be between 0 and 1".to_owned(),
        ));
    }
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut effects = Vec::with_capacity(samples);
    for _ in 0..samples {
        let mut difference = 0_i64;
        for _ in 0..pairs.len() {
            let (baseline, candidate) = pairs[rng.random_range(0..pairs.len())];
            difference += i64::from(candidate) - i64::from(baseline);
        }
        effects.push(100.0 * difference as f64 / pairs.len() as f64);
    }
    effects.sort_by(f64::total_cmp);
    let alpha = (1.0 - confidence) / 2.0;
    let lower = percentile(&effects, alpha);
    let upper = percentile(&effects, 1.0 - alpha);
    Ok(BootstrapInterval {
        lower_pp: lower,
        upper_pp: upper,
        confidence,
        samples,
        seed,
    })
}

fn percentile(sorted: &[f64], probability: f64) -> f64 {
    let position = probability * (sorted.len().saturating_sub(1)) as f64;
    let lower_index = position.floor() as usize;
    let upper_index = position.ceil() as usize;
    if lower_index == upper_index {
        sorted[lower_index]
    } else {
        let weight = position - lower_index as f64;
        sorted[lower_index] * (1.0 - weight) + sorted[upper_index] * weight
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn accepted_research_transition_counts() {
        let pairs = std::iter::repeat_n((true, true), 15)
            .chain(std::iter::repeat_n((true, false), 3))
            .chain(std::iter::repeat_n((false, true), 9))
            .chain(std::iter::repeat_n((false, false), 22))
            .collect::<Vec<_>>();
        let metrics = paired_metrics(&pairs);
        assert_eq!(metrics.baseline_pass, 18);
        assert_eq!(metrics.candidate_pass, 24);
        assert_eq!(metrics.total, 49);
        assert!((metrics.difference_pp - 12.244_897_959).abs() < 1e-9);
        assert!((metrics.mcnemar_exact_p - 0.145_996_093_75).abs() < 1e-12);
    }

    #[test]
    fn bootstrap_is_deterministic() {
        let pairs = vec![(true, false), (false, true), (false, true), (true, true)];
        assert_eq!(
            paired_bootstrap(&pairs, 1_000, 0.95, 17).unwrap(),
            paired_bootstrap(&pairs, 1_000, 0.95, 17).unwrap()
        );
    }

    proptest! {
        #[test]
        fn transition_cells_equal_complete_denominator(pairs in prop::collection::vec((any::<bool>(), any::<bool>()), 0..1000)) {
            let result = paired_metrics(&pairs);
            prop_assert_eq!(result.both_pass + result.baseline_only_pass + result.candidate_only_pass + result.both_fail, pairs.len());
            prop_assert_eq!(result.baseline_pass, result.both_pass + result.baseline_only_pass);
            prop_assert_eq!(result.candidate_pass, result.both_pass + result.candidate_only_pass);
        }

        #[test]
        fn aggregate_metrics_are_independent_of_case_order(
            pairs in prop::collection::vec((any::<bool>(), any::<bool>()), 0..1000)
        ) {
            let expected = paired_metrics(&pairs);
            let reversed = pairs.iter().copied().rev().collect::<Vec<_>>();
            prop_assert_eq!(paired_metrics(&reversed), expected);
        }
    }
}

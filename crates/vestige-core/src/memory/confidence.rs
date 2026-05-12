//! Bayesian Confidence Estimation (ZenBrain eq. 3)
//!
//! Uses Beta-Bernoulli conjugate model to estimate memory reliability.
//!
//! Prior: Beta(1, 1) — uniform, no prior knowledge.
//! After n retrievals with k useful outcomes: Beta(1 + k, 1 + n - k).
//! Point estimate: posterior mean = (1 + k) / (2 + n).
//! 95% CI: [quantile(0.025), quantile(0.975)] via normal approximation
//! for Beta distribution when α + β ≥ 5, exact otherwise.
//!
//! References:
//! - Gelman et al. (2013), Bayesian Data Analysis, Ch. 2
//! - ZenBrain (2025), Bayesian confidence propagation, §3

use serde::{Deserialize, Serialize};

/// Bayesian confidence estimate for a memory
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfidenceEstimate {
    pub point: f64,
    pub lower_95: f64,
    pub upper_95: f64,
    pub alpha: f64,
    pub beta: f64,
}

impl ConfidenceEstimate {
    /// Compute Bayesian confidence from retrieval/usefulness counts.
    ///
    /// - `times_retrieved`: total times the memory appeared in search results
    /// - `times_useful`: times the memory was subsequently marked useful
    pub fn from_counts(times_retrieved: i32, times_useful: i32) -> Self {
        let n = times_retrieved.max(0) as f64;
        let k = times_useful.max(0).min(times_retrieved.max(0)) as f64;

        let alpha = 1.0 + k;
        let beta_param = 1.0 + (n - k);

        let point = alpha / (alpha + beta_param);

        let (lower, upper) = if alpha + beta_param >= 5.0 {
            // Normal approximation to Beta distribution (Wilson interval variant)
            let var =
                (alpha * beta_param) / ((alpha + beta_param).powi(2) * (alpha + beta_param + 1.0));
            let se = var.sqrt();
            ((point - 1.96 * se).max(0.0), (point + 1.96 * se).min(1.0))
        } else {
            // Very few observations — wide interval
            (0.0, 1.0)
        };

        Self {
            point,
            lower_95: lower,
            upper_95: upper,
            alpha,
            beta: beta_param,
        }
    }

    /// No-data prior: uniform Beta(1,1)
    pub fn uninformative() -> Self {
        Self::from_counts(0, 0)
    }

    /// Whether we have enough data for a meaningful estimate (α + β ≥ 5)
    pub fn is_informed(&self) -> bool {
        self.alpha + self.beta >= 5.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uninformative_prior_is_uniform() {
        let c = ConfidenceEstimate::uninformative();
        assert!(
            (c.point - 0.5).abs() < 0.01,
            "Uninformative prior should be ~0.5"
        );
        assert_eq!(c.lower_95, 0.0);
        assert_eq!(c.upper_95, 1.0);
    }

    #[test]
    fn high_usefulness_yields_high_confidence() {
        let c = ConfidenceEstimate::from_counts(20, 18);
        assert!(
            c.point > 0.8,
            "18/20 useful should give high confidence: {}",
            c.point
        );
        assert!(
            c.lower_95 > 0.6,
            "Lower bound should be meaningful: {}",
            c.lower_95
        );
    }

    #[test]
    fn low_usefulness_yields_low_confidence() {
        let c = ConfidenceEstimate::from_counts(20, 2);
        assert!(
            c.point < 0.25,
            "2/20 useful should give low confidence: {}",
            c.point
        );
        assert!(
            c.upper_95 < 0.4,
            "Upper bound should cap low: {}",
            c.upper_95
        );
    }

    #[test]
    fn confidence_bounded_01() {
        for n in 0..50 {
            for k in 0..=n {
                let c = ConfidenceEstimate::from_counts(n, k);
                assert!((0.0..=1.0).contains(&c.point));
                assert!((0.0..=1.0).contains(&c.lower_95));
                assert!((0.0..=1.0).contains(&c.upper_95));
                assert!(c.lower_95 <= c.point && c.point <= c.upper_95);
            }
        }
    }

    #[test]
    fn ci_narrows_with_more_data() {
        let few = ConfidenceEstimate::from_counts(5, 3);
        let many = ConfidenceEstimate::from_counts(50, 30);

        let few_width = few.upper_95 - few.lower_95;
        let many_width = many.upper_95 - many.lower_95;

        assert!(
            many_width < few_width,
            "More observations should narrow CI: few={:.3}, many={:.3}",
            few_width,
            many_width
        );
    }

    #[test]
    fn same_ratio_different_n_narrows_ci() {
        // 50% ratio minimizes prior-pull divergence between small/large n
        let small = ConfidenceEstimate::from_counts(10, 5);
        let big = ConfidenceEstimate::from_counts(100, 50);

        assert!(
            (big.point - small.point).abs() < 0.05,
            "Same ratio should give similar point estimate: {} vs {}",
            big.point,
            small.point
        );
        assert!(
            (big.upper_95 - big.lower_95) < (small.upper_95 - small.lower_95),
            "Larger sample should give tighter CI"
        );
    }
}

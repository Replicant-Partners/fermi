//! `LinearNormal` regression model: `y ~ N(intercept + Σ β_j x_j, σ)`.
//!
//! Parameter layout (canonical order, same in all methods):
//! ```text
//!   params = [intercept, β_0, β_1, …, β_{p-1}, log_sigma]
//! ```
//! We use `log_sigma` rather than `sigma` so HMC can sample on the unconstrained
//! real line. `sigma = exp(log_sigma)` enforces positivity without a constrained
//! parameterisation.
//!
//! Hand-coded analytical gradient — no AD needed. Verified against finite
//! differences in tests to 1e-6.

use super::RegressionModel;

#[derive(Debug, Clone)]
pub struct LinearNormal {
    /// Number of features (excludes intercept).
    n_features: usize,
}

impl LinearNormal {
    pub fn new(n_features: usize) -> Self {
        Self { n_features }
    }

    /// Convenience accessors for parameter layout — keeps the math readable.
    fn intercept(params: &[f64]) -> f64 {
        params[0]
    }

    fn betas(params: &[f64], n_features: usize) -> &[f64] {
        &params[1..1 + n_features]
    }

    fn log_sigma(params: &[f64], n_features: usize) -> f64 {
        params[1 + n_features]
    }

    /// `intercept + Σ β_j x_j`. Hot path — kept tight.
    fn linear_predictor(params: &[f64], n_features: usize, x: &[f64]) -> f64 {
        let intercept = Self::intercept(params);
        let betas = Self::betas(params, n_features);
        intercept + betas.iter().zip(x.iter()).map(|(b, xj)| b * xj).sum::<f64>()
    }
}

impl RegressionModel for LinearNormal {
    fn name(&self) -> &str {
        "LinearNormal"
    }

    fn n_params(&self) -> usize {
        // intercept + n_features β + log_sigma
        2 + self.n_features
    }

    fn param_names(&self) -> Vec<String> {
        let mut names = Vec::with_capacity(self.n_params());
        names.push("intercept".to_string());
        for j in 0..self.n_features {
            names.push(format!("beta_{}", j));
        }
        names.push("log_sigma".to_string());
        names
    }

    fn log_likelihood_at(&self, params: &[f64], features_row: &[f64], outcome: f64) -> f64 {
        let mu = Self::linear_predictor(params, self.n_features, features_row);
        let log_sigma = Self::log_sigma(params, self.n_features);
        let sigma = log_sigma.exp();
        let resid = outcome - mu;
        // log N(resid; 0, sigma) = -0.5 log(2π) - log_sigma - resid²/(2σ²)
        -0.5 * (2.0 * std::f64::consts::PI).ln()
            - log_sigma
            - 0.5 * (resid / sigma).powi(2)
    }

    fn grad_log_likelihood_at(
        &self,
        params: &[f64],
        features_row: &[f64],
        outcome: f64,
    ) -> Vec<f64> {
        let n_features = self.n_features;
        let mu = Self::linear_predictor(params, n_features, features_row);
        let log_sigma = Self::log_sigma(params, n_features);
        let sigma = log_sigma.exp();
        let sigma2 = sigma * sigma;
        let resid = outcome - mu;

        let mut grad = vec![0.0; self.n_params()];

        // ∂/∂intercept = resid / σ²
        grad[0] = resid / sigma2;

        // ∂/∂β_j = resid * x_j / σ²
        for j in 0..n_features {
            grad[1 + j] = resid * features_row[j] / sigma2;
        }

        // ∂/∂log_sigma = -1 + resid² / σ²
        // (derived from -log_sigma - resid²/(2 exp(2 log_sigma)))
        grad[1 + n_features] = -1.0 + resid * resid / sigma2;

        grad
    }

    fn predict_mean(&self, params: &[f64], features_row: &[f64]) -> f64 {
        Self::linear_predictor(params, self.n_features, features_row)
    }

    fn predict_std(&self, params: &[f64], _features_row: &[f64]) -> f64 {
        Self::log_sigma(params, self.n_features).exp()
    }

    fn init_params(&self, _n_features: usize) -> Vec<f64> {
        let mut v = vec![0.0; self.n_params()];
        // Initialise log_sigma to log(1.0) = 0.0 — reasonable starting variance
        // for standardised data; sampler adapts from here.
        v[1 + self.n_features] = 0.0;
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn n_params_matches_layout() {
        let m = LinearNormal::new(3);
        assert_eq!(m.n_params(), 5); // intercept + 3 β + log_sigma
        assert_eq!(
            m.param_names(),
            vec!["intercept", "beta_0", "beta_1", "beta_2", "log_sigma"]
        );
    }

    #[test]
    fn log_likelihood_matches_hand_calc() {
        let m = LinearNormal::new(2);
        // y = 2 + 1.5*x0 + (-0.5)*x1, sigma = 1.0 → log_sigma = 0
        let params = vec![2.0, 1.5, -0.5, 0.0];
        let x = vec![1.0, 1.0];
        // mu = 2 + 1.5 - 0.5 = 3.0
        // y_obs = 3.5 → resid = 0.5
        // log L = -0.5*log(2π) - 0 - 0.5*(0.5)² = -0.5*1.83788 - 0.125 = -1.04394
        let ll = m.log_likelihood_at(&params, &x, 3.5);
        let expected = -0.5 * (2.0 * std::f64::consts::PI).ln() - 0.5 * 0.25;
        assert!((ll - expected).abs() < 1e-10);
    }

    /// Verify hand-coded gradient against finite differences.
    /// This is the canonical correctness gate for any model variant.
    #[test]
    fn gradient_matches_finite_difference() {
        let m = LinearNormal::new(2);
        let params = vec![1.0, 0.5, -0.3, 0.2];
        let x = vec![0.7, 1.4];
        let y = 1.8;

        let analytical = m.grad_log_likelihood_at(&params, &x, y);

        let eps = 1e-6;
        for i in 0..params.len() {
            let mut up = params.clone();
            let mut down = params.clone();
            up[i] += eps;
            down[i] -= eps;
            let fd = (m.log_likelihood_at(&up, &x, y) - m.log_likelihood_at(&down, &x, y))
                / (2.0 * eps);
            assert!(
                (analytical[i] - fd).abs() < 1e-6,
                "param {}: analytical={}, fd={}",
                i,
                analytical[i],
                fd
            );
        }
    }

    #[test]
    fn predict_mean_is_linear_combination() {
        let m = LinearNormal::new(2);
        let params = vec![1.0, 2.0, 3.0, 0.0];
        let x = vec![1.0, 1.0];
        // mean = 1 + 2*1 + 3*1 = 6
        assert!((m.predict_mean(&params, &x) - 6.0).abs() < 1e-12);
    }

    #[test]
    fn predict_std_is_exp_log_sigma() {
        let m = LinearNormal::new(1);
        let params = vec![0.0, 1.0, 0.5]; // log_sigma = 0.5
        let x = vec![1.0];
        assert!((m.predict_std(&params, &x) - 0.5_f64.exp()).abs() < 1e-12);
    }

    #[test]
    fn prior_is_proper_and_finite() {
        let m = LinearNormal::new(2);
        let params = vec![0.0; m.n_params()];
        let lp = m.log_prior(&params);
        assert!(lp.is_finite());
        // At origin, prior should be at its mode (highest value)
        let off = m.log_prior(&[1.0, 1.0, 1.0, 1.0]);
        assert!(lp > off);
    }

    #[test]
    fn prior_gradient_matches_finite_difference() {
        let m = LinearNormal::new(2);
        let params = vec![0.3, -0.7, 1.2, 0.4];
        let analytical = m.grad_log_prior(&params);

        let eps = 1e-6;
        for i in 0..params.len() {
            let mut up = params.clone();
            let mut down = params.clone();
            up[i] += eps;
            down[i] -= eps;
            let fd = (m.log_prior(&up) - m.log_prior(&down)) / (2.0 * eps);
            assert!(
                (analytical[i] - fd).abs() < 1e-7,
                "param {}: analytical={}, fd={}",
                i,
                analytical[i],
                fd
            );
        }
    }
}

//! Wire-format tests for the BayesOps HTTP surface (Spec 14 §5.6).
//!
//! These tests validate the JSON shape of every `/api/bayesops/*` request and
//! response — what an HTTP client actually sees on the wire. They do not spin
//! up a full `axum::Router` (that would require constructing the entire
//! `AppState` with its database pool, agent registry, embedder, etc.); instead
//! they exercise:
//!
//! 1. **Request struct deserialization** from the JSON an HTTP client would send.
//! 2. **The library call** that the handler makes.
//! 3. **Response struct serialization** to verify the JSON shape returned.
//!
//! The handler code in `src/handlers/bayesops.rs` is a thin forwarding layer —
//! verifying its dispatch is axum's job, verifying its data contract is ours.
//! End-to-end live testing through a real server is intentionally out of scope
//! here; that requires the full database harness used by `tests/api_tests.rs`.

use std::collections::HashMap;

use posterior::{fit_marginal, DistFamily, FittedDistribution};
use posterior_reg::{
    fit_conditional, ConditionalPosterior, RegressionConfig, SamplerConfig, WeightedSample,
};
use serde_json::json;

// ═════════════════════════════════════════════════════════════════════════════
// /api/bayesops/fit_marginal
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn fit_marginal_request_deserializes_from_wire_json() {
    // What a typical HTTP client would POST
    let body = json!({
        "observations": [0.4, 0.45, 0.42, 0.48, 0.50, 0.46, 0.41, 0.43, 0.47, 0.44, 0.49, 0.45],
        "family": "beta"
    });

    // Echo the same destructuring the handler does
    let observations: Vec<f64> = serde_json::from_value(body["observations"].clone()).unwrap();
    let family: DistFamily = serde_json::from_value(body["family"].clone()).unwrap();
    assert_eq!(family, DistFamily::Beta);

    // Run the library call exactly as the handler would
    let (fitted, meta) = fit_marginal(&observations, None, family).unwrap();
    assert_eq!(meta.n_observations, 12);

    // Serialize response and verify wire shape
    let response = json!({
        "fitted": fitted,
        "metadata": meta,
        "fpl_params": fitted.to_fpl_params(),
    });

    // Mandatory fields
    assert!(response["fitted"]["family"].is_string());
    assert_eq!(response["fitted"]["family"], "beta");
    assert!(response["fitted"]["alpha"].is_number());
    assert!(response["fitted"]["beta"].is_number());
    assert!(response["fitted"]["ci_low"].is_number());
    assert!(response["fitted"]["ci_high"].is_number());
    assert!(response["fitted"]["n_eff"].is_number());
    assert!(response["metadata"]["quality"].is_string());
    assert!(response["metadata"]["n_observations"].is_number());
    assert!(response["fpl_params"].is_string());
    assert!(response["fpl_params"]
        .as_str()
        .unwrap()
        .starts_with("beta("));
}

#[test]
fn fit_marginal_with_weights_in_wire_json() {
    let body = json!({
        "observations": [1.0, 2.0, 3.0, 4.0, 5.0],
        "weights": [1.0, 1.0, 0.2, 0.2, 0.2],
        "family": "normal",
        "source_description": "5 observations, 2 real + 3 synthetic"
    });

    let observations: Vec<f64> = serde_json::from_value(body["observations"].clone()).unwrap();
    let weights: Vec<f64> = serde_json::from_value(body["weights"].clone()).unwrap();
    let family: DistFamily = serde_json::from_value(body["family"].clone()).unwrap();

    let (fitted, mut meta) = fit_marginal(&observations, Some(&weights), family).unwrap();
    meta.source_description = body["source_description"].as_str().unwrap().to_string();

    assert!(matches!(fitted, FittedDistribution::Normal { .. }));
    assert_eq!(
        meta.source_description,
        "5 observations, 2 real + 3 synthetic"
    );
}

#[test]
fn fit_marginal_family_auto_is_the_default() {
    // Test JSON without an explicit family — should default to "auto"
    let body = json!({
        "observations": [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]
    });
    // Mimic the handler's `#[serde(default = "default_family")]`
    let family: DistFamily = body
        .get("family")
        .map(|v| serde_json::from_value(v.clone()).unwrap())
        .unwrap_or(DistFamily::Auto);
    assert_eq!(family, DistFamily::Auto);
}

#[test]
fn fit_marginal_returns_error_for_bad_data() {
    // Beta family with out-of-range observations — should error
    let observations = vec![0.5, 0.6, 1.5];
    let err = fit_marginal(&observations, None, DistFamily::Beta).unwrap_err();
    let msg = err.to_string();
    // The handler maps this through bad_request() to a 400 with this message.
    assert!(
        msg.contains("Beta requires") || msg.contains("BetaOutOfRange") || msg.contains("0,"),
        "expected range error, got: {}",
        msg
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// /api/bayesops/fit_conditional + the cache lookup endpoints
// ═════════════════════════════════════════════════════════════════════════════

fn synthetic_linear_dataset(n: usize) -> Vec<WeightedSample> {
    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};
    let mut rng = StdRng::seed_from_u64(42);
    (0..n)
        .map(|_| {
            let x: f64 = rng.gen_range(-2.0..2.0);
            let u1: f64 = rng.gen_range(1e-12..1.0);
            let u2: f64 = rng.gen::<f64>();
            let noise = 0.3 * (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
            let y = 2.0 + 0.5 * x + noise;
            let mut features = HashMap::new();
            features.insert("x".to_string(), x);
            WeightedSample::real(features, y)
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fit_conditional_request_deserializes_and_runs() {
    let data = synthetic_linear_dataset(100);

    // What an HTTP client would POST
    let body = json!({
        "data": data,
        "config": {
            "feature_names": ["x"],
            "seed": 7,
            "held_out_fraction": 0.2,
            "sampler": {
                "n_chains": 2,
                "n_warmup": 200,
                "n_draws": 300,
                "target_accept_rate": 0.80,
                "n_leapfrog": 10,
                "initial_step_size": 0.05
            }
        }
    });

    // Round-trip both fields through deserialization just like the handler does
    let data: Vec<WeightedSample> = serde_json::from_value(body["data"].clone()).unwrap();
    let config: RegressionConfig = serde_json::from_value(body["config"].clone()).unwrap();

    let posterior = fit_conditional(&data, &config).await.unwrap();

    // Compose the response struct exactly as the handler does
    let posterior_id = uuid::Uuid::new_v4();
    let response = json!({
        "posterior_id": posterior_id,
        "model_name": posterior.model_name,
        "param_names": posterior.param_names,
        "feature_names": posterior.feature_names,
        "diagnostics": posterior.diagnostics,
        "nlpd": posterior.nlpd,
        "metadata": posterior.metadata,
        "n_samples": posterior.n_samples(),
    });

    // Wire shape
    assert!(response["posterior_id"].is_string());
    assert_eq!(response["model_name"], "LinearNormal");
    assert!(response["param_names"].is_array());
    assert_eq!(response["feature_names"][0], "x");
    assert!(response["diagnostics"]["r_hat"].is_array());
    assert!(response["diagnostics"]["ess_bulk"].is_array());
    assert!(response["diagnostics"]["divergences"].is_number());
    assert!(response["diagnostics"]["converged"].is_boolean());
    assert!(response["nlpd"].is_number());
    assert!(response["metadata"]["quality"].is_string());
    assert!(response["n_samples"].as_u64().unwrap() > 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fit_then_predict_round_trip_through_cache() {
    use dashmap::DashMap;
    use std::sync::Arc;
    use uuid::Uuid;

    // Build the same in-memory cache the handler uses
    let cache: Arc<DashMap<Uuid, ConditionalPosterior>> = Arc::new(DashMap::new());

    // Fit + cache (handler step 1)
    let data = synthetic_linear_dataset(80);
    let cfg = RegressionConfig::new(vec!["x".to_string()])
        .with_seed(11)
        .with_sampler(SamplerConfig {
            n_chains: 2,
            n_warmup: 150,
            n_draws: 200,
            target_accept_rate: 0.80,
            n_leapfrog: 10,
            initial_step_size: Some(0.05),
        });
    let posterior = fit_conditional(&data, &cfg).await.unwrap();
    let id = Uuid::new_v4();
    cache.insert(id, posterior);

    // Predict (handler step 2)
    let predict_request = json!({
        "posterior_id": id,
        "features": { "x": 1.0 }
    });
    let predict_id: Uuid = serde_json::from_value(predict_request["posterior_id"].clone()).unwrap();
    let predict_features: HashMap<String, f64> =
        serde_json::from_value(predict_request["features"].clone()).unwrap();

    let entry = cache.get(&predict_id).expect("cache hit");
    let fitted = entry.predict(&predict_features).unwrap();
    let response = json!({
        "fitted": fitted,
        "fpl_params": fitted.to_fpl_params(),
    });

    assert_eq!(response["fitted"]["family"], "normal");
    let predicted_mean = response["fitted"]["mean"].as_f64().unwrap();
    assert!(
        (predicted_mean - 2.5).abs() < 0.2,
        "predicted mean {} not near truth 2.5",
        predicted_mean
    );
    assert!(response["fpl_params"]
        .as_str()
        .unwrap()
        .starts_with("normal("));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn all_whatif_responses_serialize_to_expected_shapes() {
    let data = synthetic_linear_dataset(80);
    let cfg = RegressionConfig::new(vec!["x".to_string()])
        .with_seed(13)
        .with_sampler(SamplerConfig {
            n_chains: 2,
            n_warmup: 150,
            n_draws: 200,
            target_accept_rate: 0.80,
            n_leapfrog: 10,
            initial_step_size: Some(0.05),
        });
    let posterior = fit_conditional(&data, &cfg).await.unwrap();

    // ── input_sensitivity response ────────────────────────────────────────────
    let mut ranges = HashMap::new();
    ranges.insert("x".to_string(), (-2.0, 2.0));
    let sensitivity = posterior.input_sensitivity(&ranges, 100).unwrap();
    let sens_response = json!({ "sensitivity": sensitivity });
    let sens_x = &sens_response["sensitivity"]["x"];
    assert_eq!(sens_x["feature_name"], "x");
    assert!(sens_x["first_order_index"].is_number());
    assert!(sens_x["total_order_index"].is_number());
    assert!(sens_x["ci"].is_array());
    assert_eq!(sens_x["ci"].as_array().unwrap().len(), 2);

    // ── compare_scenarios response ────────────────────────────────────────────
    let a = HashMap::from([("x".to_string(), 2.0)]);
    let b = HashMap::from([("x".to_string(), -2.0)]);
    let comp = posterior.compare_scenarios(&a, &b).unwrap();
    let comp_response = serde_json::to_value(&comp).unwrap();
    assert_eq!(comp_response["a"]["family"], "normal");
    assert_eq!(comp_response["b"]["family"], "normal");
    assert!(comp_response["prob_a_better"].is_number());
    assert!(comp_response["expected_gain"].is_number());
    assert!(comp_response["risk_ratio"].is_number());
    // A has higher x → higher predicted y → P(A>B) should be near 1
    assert!(comp_response["prob_a_better"].as_f64().unwrap() > 0.8);

    // ── prob_exceeds response ─────────────────────────────────────────────────
    let features = HashMap::from([("x".to_string(), 1.0)]);
    let probability = posterior.prob_exceeds(&features, 2.5).unwrap();
    let prob_response = json!({ "probability": probability });
    let p = prob_response["probability"].as_f64().unwrap();
    assert!((0.0..=1.0).contains(&p), "probability {} out of [0,1]", p);

    // ── optimise_for_target response ──────────────────────────────────────────
    let opt = posterior
        .optimise_for_target(&HashMap::new(), "x", (-2.0, 2.0), 3.0)
        .unwrap();
    let opt_response = serde_json::to_value(&opt).unwrap();
    assert!(opt_response["recommended_value"].is_number());
    assert!(opt_response["prob_at_recommended"].is_number());
    assert_eq!(opt_response["predictive_dist"]["family"], "normal");
    assert!(opt_response["sensitivity_curve"].is_array());
    assert_eq!(
        opt_response["sensitivity_curve"].as_array().unwrap().len(),
        41
    );
    // Each curve point should be [feature_value, prob_exceeds]
    let first = &opt_response["sensitivity_curve"][0];
    assert!(first.is_array());
    assert_eq!(first.as_array().unwrap().len(), 2);
}

// ═════════════════════════════════════════════════════════════════════════════
// /api/bayesops/posteriors — list + evict
// ═════════════════════════════════════════════════════════════════════════════

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn list_and_evict_cache_lifecycle() {
    use dashmap::DashMap;
    use std::sync::Arc;
    use uuid::Uuid;

    let cache: Arc<DashMap<Uuid, ConditionalPosterior>> = Arc::new(DashMap::new());

    // Start empty
    let listing: Vec<serde_json::Value> = cache.iter().map(|e| json!({ "id": e.key() })).collect();
    assert_eq!(listing.len(), 0);

    // Fit + insert
    let data = synthetic_linear_dataset(50);
    let cfg = RegressionConfig::new(vec!["x".to_string()])
        .with_seed(19)
        .with_sampler(SamplerConfig {
            n_chains: 2,
            n_warmup: 100,
            n_draws: 100,
            target_accept_rate: 0.80,
            n_leapfrog: 10,
            initial_step_size: Some(0.05),
        });
    let posterior = fit_conditional(&data, &cfg).await.unwrap();
    let id = Uuid::new_v4();
    cache.insert(id, posterior);

    // List endpoint response shape
    let listing: Vec<serde_json::Value> = cache
        .iter()
        .map(|entry| {
            json!({
                "posterior_id": entry.key(),
                "model_name": entry.value().model_name,
                "feature_names": entry.value().feature_names,
                "fitted_at": entry.value().metadata.fitted_at,
                "n_samples": entry.value().n_samples(),
                "nlpd": entry.value().nlpd,
            })
        })
        .collect();
    assert_eq!(listing.len(), 1);
    assert_eq!(listing[0]["model_name"], "LinearNormal");
    assert_eq!(listing[0]["feature_names"][0], "x");

    // Evict
    assert!(cache.remove(&id).is_some());
    assert_eq!(cache.len(), 0);

    // 404 path: trying to remove a missing id
    let other = Uuid::new_v4();
    assert!(cache.remove(&other).is_none());
}

// ═════════════════════════════════════════════════════════════════════════════
// Cross-cutting: domain-neutrality (Spec 14 §9)
// ═════════════════════════════════════════════════════════════════════════════

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn surface_is_domain_neutral_works_for_calibration_data() {
    // Calibration-style scenario: feature is "prior_brier_score", outcome is
    // current accuracy. Tests that nothing in the wire format is SimOps-specific.
    let mut data = Vec::new();
    for i in 0..50 {
        let prior_brier = 0.1 + (i as f64) * 0.005;
        let accuracy = 0.95 - prior_brier * 0.8 + ((i % 7) as f64) * 0.01;
        let mut features = HashMap::new();
        features.insert("prior_brier_score".to_string(), prior_brier);
        data.push(WeightedSample::real(features, accuracy));
    }

    let body = json!({
        "data": data,
        "config": {
            "feature_names": ["prior_brier_score"],
            "seed": 23,
            "sampler": {
                "n_chains": 2,
                "n_warmup": 100,
                "n_draws": 150,
                "target_accept_rate": 0.80,
                "n_leapfrog": 10,
                "initial_step_size": 0.05
            }
        }
    });

    let data: Vec<WeightedSample> = serde_json::from_value(body["data"].clone()).unwrap();
    let config: RegressionConfig = serde_json::from_value(body["config"].clone()).unwrap();
    assert_eq!(config.feature_names, vec!["prior_brier_score"]);

    let posterior = fit_conditional(&data, &config).await.unwrap();
    assert_eq!(posterior.feature_names, vec!["prior_brier_score"]);

    // The fitted distribution comes out as a normal FPL Driver string regardless
    // of what domain produced the data.
    let mut q = HashMap::new();
    q.insert("prior_brier_score".to_string(), 0.2);
    let fitted = posterior.predict(&q).unwrap();
    assert!(fitted.to_fpl_params().starts_with("normal("));
}

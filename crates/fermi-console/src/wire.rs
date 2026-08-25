//! Wire-contract coercions for the ABW API.
//!
//! Pure functions that enforce, client-side, constraints the server
//! validates and rejects on. They live in the lib target so they can
//! be tested — see [`crate`] docs for why the bin target can't be.

/// Coerce a model output into the `[0,1]` range the API contract
/// requires for `predicted_probability`.
///
/// # Why this exists
///
/// Server-side, both `create_forecast_handler` and
/// `update_forecast_handler` reject out-of-range values with
/// `HTTP 400: predicted_probability must be between 0 and 1`.
///
/// Client-side, `run_simulation` does **not** guarantee that range.
/// When the question carries no `base_rate` the cockpit treats the
/// forecast as non-probabilistic (a count, magnitude or duration) and
/// assigns the raw simulation mean unclamped. But the default Fermi
/// decomposition emits a multiplier chain — `strength_factor *
/// conditions * disruption`, every driver centred on 1.0 — whose
/// product sits around 1.0 and routinely exceeds it.
///
/// The observed failure was silent, total data loss: the save 400'd,
/// the local snapshot still succeeded, the UI reported "Saved just
/// now", and reopening the forecast showed the pre-simulation value.
/// Clamping at the persistence boundary means no call path can
/// reintroduce that. The true mean is preserved in
/// `simulation_results`, which has no range constraint.
///
/// Non-finite input maps to `0.5` rather than propagating: `NaN` and
/// infinity serialise to JSON `null`, which fails deserialisation
/// server-side with a far less legible error than a clamp warning.
pub fn clamp_wire_probability(p: f64) -> f64 {
    if p.is_finite() {
        p.clamp(0.0, 1.0)
    } else {
        0.5
    }
}

/// Coerce an optional confidence-interval bound into the `[0,1]`
/// range the database enforces.
///
/// `fermi_forecasts.confidence_interval_low` and
/// `confidence_interval_high` carry
/// `CHECK (col >= 0 AND col <= 1)` (mig-048, mig-094). Unlike
/// `predicted_probability`, the handler does **not** range-check these
/// in Rust — so an out-of-range value reaches Postgres and surfaces as
/// a constraint violation wrapped in a 500, not a clean 400.
///
/// The console fills both straight from `sim_results.p5` / `p95`,
/// which are exactly as unbounded as the simulation mean. A
/// multiplier-chain model that pushes the mean past 1.0 pushes p95
/// further still — an observed run had `mean 1.068, p95 1.655`.
/// Clamping `predicted_probability` alone therefore just moves the
/// failure one column to the right.
///
/// Non-finite bounds return `None` (omit the field) rather than
/// substituting a value: unlike the point estimate, an interval bound
/// has no defensible stand-in, and both columns are nullable.
pub fn clamp_wire_interval_bound(v: Option<f64>) -> Option<f64> {
    match v {
        Some(x) if x.is_finite() => Some(x.clamp(0.0, 1.0)),
        _ => None,
    }
}

/// What caused a forecast revision, in the shape the timeline reads.
///
/// # Why the trajectory could not answer "what changed my mind"
///
/// `forecast_spacetime` has carried `triggering_agent` and `evidence_delta`
/// since migration 140, and `forecast_timeline_handler` projects both. The
/// console never sent them: the post-simulation persist built
/// `UpdateProbabilityRequest { agent_id: None, evidence_added: None, .. }` with
/// a reason string naming only Monte Carlo statistics.
///
/// The server derives `revision_trigger = if agent_id.is_some() { "agent_correction" }
/// else { "manual" }`, so accepting an agent's multiplier and re-simulating was
/// recorded as a *manual* edit by *nobody*, citing no evidence. Every column
/// needed to answer "how did research move my inside view" existed and was NULL
/// on the only path that fills them.
///
/// This is the payload for the case where the cause is known. A sim run for any
/// other reason still sends `None`, because inventing an attribution is worse
/// than admitting there isn't one.
#[derive(Debug, Clone, PartialEq)]
pub struct RevisionAttribution {
    /// The agent's ABW id — what the server keys `triggering_agent` on.
    pub agent_id: String,
    /// The agent's bound FPL name, which encodes the driver it was hired for.
    pub bound_name: String,
    pub driver: String,
    pub evidence_id: String,
    pub previous_p50: f64,
    pub updated_p50: f64,
}

impl RevisionAttribution {
    /// The `evidence_added` body.
    ///
    /// Deliberately the same key set the BayesOps accept path already writes
    /// (`kind`, plus the before/after pair), so the timeline has one shape to
    /// render rather than two. `kind` distinguishes them.
    pub fn evidence_delta(&self) -> serde_json::Value {
        serde_json::json!({
            "kind": "agent_suggestion_accepted",
            "agent_id": self.agent_id,
            "bound_name": self.bound_name,
            "driver_name": self.driver,
            "evidence_id": self.evidence_id,
            "previous_p50": self.previous_p50,
            "updated_p50": self.updated_p50,
        })
    }

    /// The human-readable revision reason.
    ///
    /// Replaces "Local Monte Carlo simulation: mean=…", which described the
    /// arithmetic and not the cause.
    pub fn reason(&self) -> String {
        format!(
            "Accepted {}'s suggestion for {}: p50 {:.3} \u{2192} {:.3}",
            self.agent_id, self.driver, self.previous_p50, self.updated_p50
        )
    }
}


// ═══════════════════════════════════════════════════════════════════
// Per-agent measured contribution
// ═══════════════════════════════════════════════════════════════════

/// Read `GET /api/agents/contributions` into the router's record type.
///
/// Lives here rather than beside the other API structs because the API
/// client is in the BINARY target, where `#[cfg(test)]` modules cannot be
/// compiled (see the crate docs). This conversion has three ways to be
/// silently wrong — a renamed field, a null read as zero, a negative count
/// coerced — and all three would degrade specialist ranking without
/// erroring, so it belongs where it can be asserted.
///
/// The wire shape, from `handlers::agents::agent_contributions_handler`:
///
/// ```json
/// { "contributions": [ { "agent_name": "weather_oracle",
///                        "mean_shapley": 0.031,
///                        "n_forecasts": 14,
///                        "n_clusters": 5,
///                        "ci_low": 0.004,
///                        "ci_high": 0.058 } ],
///   "count": 1 }
/// ```
///
/// Rows without an `agent_name` or a numeric `mean_shapley` are dropped:
/// there is nothing to rank and nothing to rank it by. `ci_low` is carried
/// through as `Option` and MUST NOT be defaulted — see
/// [`crate::routing::Proven::is_established`].
pub fn agent_contributions_from_json(payload: &serde_json::Value) -> Vec<crate::routing::Proven> {
    payload
        .get("contributions")
        .and_then(|c| c.as_array())
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|row| {
            Some(crate::routing::Proven {
                agent: row.get("agent_name")?.as_str()?.trim().to_string(),
                mean_shapley: row.get("mean_shapley")?.as_f64()?,
                // A count is informational, not load-bearing; a missing or
                // nonsensical one must not discard an otherwise usable row.
                n_forecasts: row
                    .get("n_forecasts")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0)
                    .max(0) as u32,
                // Deliberately NOT `.unwrap_or(0.0)`. A null interval means
                // the server had too few independent clusters to say
                // anything; defaulting it to zero would make every thin
                // record read as "exactly at the threshold" and, with a
                // `>` test, is only saved from promoting them by the
                // strictness of the comparison. Keep the absence.
                ci_low: row.get("ci_low").and_then(|v| v.as_f64()),
            })
            .filter(|p| !p.agent.is_empty())
        })
        .collect()
}

#[cfg(test)]
mod contribution_wire_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_documented_shape_round_trips() {
        let payload = json!({
            "contributions": [
                { "agent_name": "weather_oracle", "mean_shapley": 0.031,
                  "n_forecasts": 14, "n_clusters": 5,
                  "ci_low": 0.004, "ci_high": 0.058 }
            ],
            "count": 1
        });
        let got = agent_contributions_from_json(&payload);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].agent, "weather_oracle");
        assert_eq!(got[0].n_forecasts, 14);
        assert_eq!(got[0].ci_low, Some(0.004));
        assert!(got[0].is_established());
    }

    #[test]
    fn a_null_interval_stays_absent() {
        // The whole point of carrying an Option. `ci_low: null` is the
        // server saying it had too few independent clusters to speak;
        // reading it as 0.0 would turn "no evidence" into "exactly
        // borderline".
        let payload = json!({
            "contributions": [
                { "agent_name": "thin", "mean_shapley": 0.9,
                  "n_forecasts": 2, "n_clusters": 1,
                  "ci_low": null, "ci_high": null }
            ]
        });
        let got = agent_contributions_from_json(&payload);
        assert_eq!(got[0].ci_low, None);
        assert!(
            !got[0].is_established(),
            "a two-forecast record was promoted over a declared specialist"
        );
    }

    #[test]
    fn a_row_with_nothing_to_rank_by_is_dropped() {
        let payload = json!({
            "contributions": [
                { "agent_name": "no_score" },
                { "mean_shapley": 0.1 },
                { "agent_name": "   ", "mean_shapley": 0.1 },
                { "agent_name": "ok", "mean_shapley": 0.1, "ci_low": 0.01 }
            ]
        });
        let got = agent_contributions_from_json(&payload);
        assert_eq!(got.len(), 1, "{got:?}");
        assert_eq!(got[0].agent, "ok");
    }

    #[test]
    fn a_missing_or_absurd_count_does_not_discard_the_row() {
        let payload = json!({
            "contributions": [
                { "agent_name": "a", "mean_shapley": 0.1, "ci_low": 0.01 },
                { "agent_name": "b", "mean_shapley": 0.1, "n_forecasts": -3, "ci_low": 0.01 }
            ]
        });
        let got = agent_contributions_from_json(&payload);
        assert_eq!(got.len(), 2);
        assert!(got.iter().all(|p| p.n_forecasts == 0 && p.is_established()));
    }

    #[test]
    fn an_empty_or_malformed_payload_is_no_record_rather_than_a_panic() {
        // A server without the endpoint answers 404 and the caller never
        // gets here, but a proxy returning something shaped differently
        // must degrade to "rank on declarations", not take the app down.
        for payload in [json!({}), json!({"contributions": null}), json!([])] {
            assert!(agent_contributions_from_json(&payload).is_empty());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_range_values_pass_through_unchanged() {
        assert_eq!(clamp_wire_probability(0.0), 0.0);
        assert_eq!(clamp_wire_probability(0.5), 0.5);
        assert_eq!(clamp_wire_probability(1.0), 1.0);
        assert_eq!(clamp_wire_probability(0.0208), 0.0208);
    }

    #[test]
    fn clamps_the_multiplier_model_case() {
        // The exact production value: a three-driver multiplier chain
        // with no base_rate produced 1.068, the PUT 400'd, and the
        // simulation was lost on every save.
        assert_eq!(clamp_wire_probability(1.068), 1.0);
        // The console displayed 106.79% in the reported session.
        assert_eq!(clamp_wire_probability(1.0679), 1.0);
    }

    #[test]
    fn clamps_both_ends() {
        assert_eq!(clamp_wire_probability(-0.3), 0.0);
        assert_eq!(clamp_wire_probability(42.0), 1.0);
    }

    #[test]
    fn non_finite_becomes_max_entropy_not_null() {
        // Serialising NaN/inf yields JSON `null`, which the server
        // rejects with a much worse error than a clamp warning.
        assert_eq!(clamp_wire_probability(f64::NAN), 0.5);
        assert_eq!(clamp_wire_probability(f64::INFINITY), 0.5);
        assert_eq!(clamp_wire_probability(f64::NEG_INFINITY), 0.5);
    }

    // ── interval bounds ───────────────────────────────────────

    #[test]
    fn interval_bounds_in_range_pass_through() {
        assert_eq!(clamp_wire_interval_bound(Some(0.0)), Some(0.0));
        assert_eq!(clamp_wire_interval_bound(Some(0.637)), Some(0.637));
        assert_eq!(clamp_wire_interval_bound(Some(1.0)), Some(1.0));
    }

    #[test]
    fn clamps_the_observed_p95_constraint_violation() {
        // The production failure: mean 1.068 clamped fine, but p95
        // 1.655 went straight to Postgres and tripped
        // fermi_forecasts_confidence_interval_high_check as a 500.
        assert_eq!(clamp_wire_interval_bound(Some(1.655)), Some(1.0));
    }

    #[test]
    fn interval_bounds_clamp_below_zero() {
        assert_eq!(clamp_wire_interval_bound(Some(-0.2)), Some(0.0));
    }

    #[test]
    fn absent_and_non_finite_bounds_are_omitted() {
        // Nullable columns — omitting beats inventing a bound.
        assert_eq!(clamp_wire_interval_bound(None), None);
        assert_eq!(clamp_wire_interval_bound(Some(f64::NAN)), None);
        assert_eq!(clamp_wire_interval_bound(Some(f64::INFINITY)), None);
        assert_eq!(clamp_wire_interval_bound(Some(f64::NEG_INFINITY)), None);
    }

    #[test]
    fn interval_output_always_satisfies_the_check_constraint() {
        for raw in [
            Some(-1e9),
            Some(-0.0001),
            Some(0.0),
            Some(0.637),
            Some(1.0),
            Some(1.655),
            Some(1e9),
            Some(f64::NAN),
            Some(f64::INFINITY),
            None,
        ] {
            if let Some(out) = clamp_wire_interval_bound(raw) {
                assert!(
                    out.is_finite() && (0.0..=1.0).contains(&out),
                    "clamp_wire_interval_bound({raw:?}) = {out} violates CHECK (col >= 0 AND col <= 1)"
                );
            }
        }
    }

    #[test]
    fn clamping_preserves_low_le_high_ordering() {
        // Both bounds clamp monotonically, so a valid interval can
        // collapse to a point but can never invert.
        for (lo, hi) in [(0.2, 0.8), (0.637, 1.655), (1.2, 3.4), (-0.5, 0.1)] {
            let clo = clamp_wire_interval_bound(Some(lo)).unwrap();
            let chi = clamp_wire_interval_bound(Some(hi)).unwrap();
            assert!(clo <= chi, "interval inverted: {lo}..{hi} -> {clo}..{chi}");
        }
    }

    #[test]
    fn output_is_always_serialisable_as_a_probability() {
        for raw in [
            -1e9,
            -0.0001,
            0.0,
            0.5,
            1.0,
            1.0000001,
            1.068,
            1e9,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ] {
            let out = clamp_wire_probability(raw);
            assert!(
                out.is_finite() && (0.0..=1.0).contains(&out),
                "clamp_wire_probability({raw}) = {out} violates the wire contract"
            );
        }
    }

    // ── Revision attribution ─────────────────────────────────────────────

    fn attribution() -> RevisionAttribution {
        RevisionAttribution {
            agent_id: "weather_oracle".into(),
            bound_name: "weather_oracle_ensemble_spread".into(),
            driver: "ensemble_spread".into(),
            evidence_id: "weather_oracle_ensemble_spread_0".into(),
            previous_p50: 1.0,
            updated_p50: 1.25,
        }
    }

    /// The delta names the driver, the agent, and the evidence that carried it.
    ///
    /// These three are what the trajectory needs to answer "how did research
    /// change my inside view". They were all discoverable at the accept site and
    /// none of them was sent.
    #[test]
    fn the_evidence_delta_carries_the_join_keys() {
        let d = attribution().evidence_delta();

        assert_eq!(d["kind"], "agent_suggestion_accepted");
        assert_eq!(d["agent_id"], "weather_oracle");
        assert_eq!(
            d["driver_name"], "ensemble_spread",
            "without the driver the timeline can say a number moved but not \
             which part of the model moved it"
        );
        assert_eq!(
            d["evidence_id"], "weather_oracle_ensemble_spread_0",
            "the evidence id is the only link back to what the agent actually said"
        );
        assert_eq!(d["previous_p50"], 1.0);
        assert_eq!(d["updated_p50"], 1.25);
    }

    /// The bound name is retained, because it is the agent-to-driver join key.
    ///
    /// `process_agent_evidence` deliberately drops it when writing the workspace
    /// log ("attribute the message to the ABW id, not to this program's bound
    /// name"), which is right for that log and wrong as a reason to lose it
    /// everywhere. Both identifiers travel here.
    #[test]
    fn both_the_abw_id_and_the_bound_name_survive() {
        let d = attribution().evidence_delta();
        assert_eq!(d["agent_id"], "weather_oracle");
        assert_eq!(d["bound_name"], "weather_oracle_ensemble_spread");
    }

    /// The reason states the cause, not the arithmetic.
    #[test]
    fn the_reason_names_the_agent_and_the_driver() {
        let r = attribution().reason();
        assert!(r.contains("weather_oracle"), "{r}");
        assert!(r.contains("ensemble_spread"), "{r}");
        assert!(
            !r.contains("Monte Carlo"),
            "the old reason described how the number was computed, which was \
             never the question: {r}"
        );
    }
}

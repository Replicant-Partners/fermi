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
}

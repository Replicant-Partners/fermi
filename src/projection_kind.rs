//! Is this observation a model **projection** or a **measurement**?
//!
//! One definition, because there were two and they did not overlap.
//!
//! # The disagreement, measured
//!
//! Loop 5.A's projection-accuracy path scores a physical measurement against
//! what a model projected. It has
//! written zero signals, and the liveness rung reported that against **12,167
//! opportunities** — a number that made the loop look like a wiring problem at
//! the trigger site. It is not. The producer and the reader disagree about what
//! a projection *looks like*, and have since the reader was written:
//!
//! | | tag written | tag matched |
//! |---|---|---|
//! | dynamics runner (`POST /api/observations`) | `extra.source_kind = "dynamics_projection"` | — |
//! | agent tool (`simops_write_observation`) | `extra.source = "simops_simulation"` | — |
//! | `eval_projection`, `simops_benchmark`, `observations` | — | `extra.source = "simops_simulation"` |
//!
//! Against production: **12,167** projection rows, all of them
//! `source_kind = "dynamics_projection"` with no `source` key at all, and
//! **0** rows anywhere in `sosa_observations` carrying
//! `source = "simops_simulation"`. The reader's predicate selects the empty set.
//! Every consumer downstream of it is therefore correct-looking, exercised, and
//! reading nothing.
//!
//! The agent tool that writes the tag the readers look for is real code with
//! real tests. It has simply never been the thing that runs — the projections
//! come from an external `kask:dynamics/...` runner posting to the observations
//! API. So this was not a typo anyone could have found by reading either side
//! on its own; it needed the row counts.
//!
//! # Why a macro and not a function
//!
//! [`crate::liveness_trust::LivenessContract`] holds its queries as
//! `&'static str`, and a contract that quotes the predicate instead of
//! referencing it is a third copy waiting to drift from the other two. A
//! `macro_rules!` expanding to a string literal can be spliced into a `const`
//! with `concat!`; a `fn` cannot. The Rust-side [`is_projection`] and the SQL
//! side are built from the same two constants below, and
//! `the_sql_and_rust_predicates_agree` runs both over the live table and
//! compares the counts.
//!
//! # What this does not decide
//!
//! Whether a projection has a *counterpart* to be scored against. It does not,
//! today: zero real observations share an `observable_property` with any of the
//! 61 distinct projections on file. That is a separate fact and it belongs in
//! the liveness chain, not in a predicate.

/// Written by `agent_backend::simops_tools::execute_simops_write_observation`.
///
/// Zero rows in production. Kept because the tool is reachable and the day it
/// runs its output must be recognised, not because anything has used it.
pub const SOURCE_SIMOPS_SIMULATION: &str = "simops_simulation";

/// Written by the external dynamics runner via `POST /api/observations`.
///
/// **12,167 rows.** This is what a projection actually looks like here, and no
/// consumer recognised it until the counts were compared.
pub const SOURCE_KIND_DYNAMICS_PROJECTION: &str = "dynamics_projection";

/// The predicate, as SQL, over an `extra` JSONB column.
///
/// `is_projection_sql!()` for a bare `extra`; `is_projection_sql!("syn")` for
/// `syn.extra`. Expands to a string literal so it can be `concat!`ed into the
/// `const` queries in [`crate::liveness_trust`].
///
/// Note the asymmetry with the negation: **absence of both tags means real**.
/// `NOT (...)` is therefore correct for the measurement side even when `extra`
/// is `NULL`-ish, which the untagged 7,576 real rows rely on.
#[macro_export]
macro_rules! is_projection_sql {
    () => {
        "(extra->>'source' = 'simops_simulation' \
          OR extra->>'source_kind' = 'dynamics_projection')"
    };
    ($alias:literal) => {
        concat!(
            "(",
            $alias,
            ".extra->>'source' = 'simops_simulation' OR ",
            $alias,
            ".extra->>'source_kind' = 'dynamics_projection')"
        )
    };
}

/// The predicate over a bare `extra` column, as a `&'static str`.
pub const IS_PROJECTION_SQL: &str = is_projection_sql!();

/// Is this observation's `extra` blob a model projection?
///
/// Built from the same two constants as the SQL, so the two cannot drift
/// without the constants moving together.
pub fn is_projection(extra: &serde_json::Value) -> bool {
    let tag = |key: &str| extra.get(key).and_then(serde_json::Value::as_str);
    tag("source") == Some(SOURCE_SIMOPS_SIMULATION)
        || tag("source_kind") == Some(SOURCE_KIND_DYNAMICS_PROJECTION)
}

/// Is this a real measurement — something the world said, rather than
/// something a model said?
///
/// Stated as its own function rather than left to callers to negate. Three
/// call sites negated it by hand and one of them (`resolve_against_projection`)
/// used `!= "simops_simulation"`, which classified every one of the 12,167
/// dynamics projections as a real measurement. Nothing came of it only because
/// the commitment table those projections would have been scored against is
/// also empty.
pub fn is_measurement(extra: &serde_json::Value) -> bool {
    !is_projection(extra)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The shape that actually fills the table, reduced to the keys that decide.
    fn dynamics_runner_row() -> serde_json::Value {
        json!({
            "projection_id": "proj-coupled-e0fcebcf-c9aa-4eef-b638-6a48dedb3cc7",
            "source_kind": "dynamics_projection",
            "model_uri": "kask:dynamics/kombucha_f2_carbonation@v1",
            "stage_id": "secondary_fermentation",
            "twin_id": "primary",
        })
    }

    #[test]
    fn the_shape_that_fills_the_table_is_recognised_as_a_projection() {
        // The regression this module exists for. Before it, every consumer
        // matched `source = "simops_simulation"` and this row — all 12,167 of
        // it — read as a real measurement.
        assert!(is_projection(&dynamics_runner_row()));
        assert!(!is_measurement(&dynamics_runner_row()));
    }

    #[test]
    fn the_agent_tool_shape_is_still_recognised() {
        // Zero rows in production, and it must keep working anyway: the old
        // predicate was not wrong about this case, it was incomplete, and a
        // fix that traded one blind spot for another would be no better.
        let row = json!({ "source": "simops_simulation", "projection_id": "p1" });
        assert!(is_projection(&row));
    }

    #[test]
    fn an_untagged_reading_is_a_measurement() {
        // 7,576 production rows carry neither tag. If absence read as
        // "projection" the resolution hook would score readings against
        // themselves.
        assert!(is_measurement(&json!({ "sensor": "ph-01" })));
        assert!(is_measurement(&json!({})));
    }

    #[test]
    fn a_projection_id_alone_does_not_make_it_a_projection() {
        // A real reading is expected to carry the `projection_id` of the
        // projection it settles — that is the link Loop 5.A needs. If the
        // presence of the key were the predicate, the measurement would be
        // classified as the prediction and scored against itself.
        let real_reading_answering_a_projection = json!({
            "projection_id": "proj-coupled-e0fcebcf",
            "sensor_id": "ph-01",
        });
        assert!(is_measurement(&real_reading_answering_a_projection));
    }

    #[test]
    fn the_sql_and_the_rust_name_the_same_two_tags() {
        // Not a proof that they agree — `the_sql_and_rust_predicates_agree` in
        // the live tier does that against the table. This is the cheap half:
        // the constants are the single source, so a change to one that does not
        // reach the other cannot compile past here.
        assert!(IS_PROJECTION_SQL.contains(SOURCE_SIMOPS_SIMULATION));
        assert!(IS_PROJECTION_SQL.contains(SOURCE_KIND_DYNAMICS_PROJECTION));
        assert!(is_projection_sql!("syn").contains("syn.extra"));
    }
}

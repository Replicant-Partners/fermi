//! What `anomaly_events` will accept, declared once in Rust.
//!
//! # The incident
//!
//! `anomaly_events` is Loop 2's only input. It held zero rows against 1,411
//! timeline entries, and the fix for that — commit `3e6c9e08`, "the loop
//! required its own output as its input" — raises a `grounding` anomaly when
//! the grounding contract finds a violation. It constructs the event with:
//!
//! ```ignore
//! kind:     "grounding".to_string(),
//! severity: "L1".to_string(),
//! ```
//!
//! The table says:
//!
//! ```sql
//! severity TEXT NOT NULL DEFAULT 'warning'
//!     CHECK (severity IN ('info', 'warning', 'critical'))
//! ```
//!
//! **`L1` is not in that set.** Every grounding anomaly the seed produced was
//! rejected by the database. The insert is `tokio::spawn`ed and its error is
//! `tracing::warn!`ed, so the request succeeded, the log line scrolled past, and
//! `anomaly_events` stayed at zero — with a note in the handoff saying to watch
//! it after the next traffic, which would never have arrived.
//!
//! Verified against production before this module was written: the exact row
//! the handler builds fails with
//! `violates check constraint "anomaly_events_severity_check"`, and the same row
//! with `warning` inserts.
//!
//! `L1` was not careless. It is a coherent severity scheme — the comment
//! explains it as "a reviewable defect in one output, not a fleet-wide safety
//! event" — and it is a *second* scheme, invented at the writer, for a column
//! that already had one. That is the failure mode `assertions.rs` names in "One
//! ladder, not two": the copy nearest the writer wins, right up until something
//! downstream disagrees, and here the thing downstream was a CHECK constraint.
//!
//! # What this module is for
//!
//! Every kind and severity the platform can construct, named here, with
//! `tests/anomaly_firing_probe.rs` asserting **both directions** against the
//! live constraint:
//!
//! * everything declared here is accepted by the table — catches a Rust
//!   vocabulary the schema has not caught up with, which is this incident;
//! * everything the table accepts is declared here — catches a migration that
//!   widens the CHECK for a producer nobody wrote, which is what migration 200
//!   did for `grounding` and what left `AnomalyKind` without the variant.
//!
//! Neither direction can be checked by reading either side.

/// Detected by `PersonaDriftMonitor` — the agent's embedding has moved away
/// from its persona baseline across a version boundary.
pub const KIND_DRIFT: &str = "drift";
/// The same dimension flagged as conflicting in N consecutive entries.
pub const KIND_ROLLING_CONFLICT: &str = "rolling_conflict";
/// A dyad's rapport dropped sharply.
pub const KIND_RUPTURE: &str = "rupture";
/// A safety evaluator flagged the response.
pub const KIND_SAFETY: &str = "safety";
/// The agent populated an output field no tool of its could supply.
///
/// Added to the CHECK by migration 200 and to `AnomalyKind` never: it is
/// constructed directly by `handlers::execution`, not by `AnomalyDetector`,
/// because it is detected on the execute path rather than by scanning the
/// timeline.
pub const KIND_GROUNDING: &str = "grounding";
/// The agent left a contracted field empty that its own declared tool can fill.
///
/// The mirror of [`KIND_GROUNDING`]. That one is fabrication — a field populated
/// from nowhere. This is the opposite and had no name: a field left empty while
/// the block is graded `tool_no_match`, which reads as "the tool answered and had
/// nothing".
///
/// The grade cannot tell the two apart. `grounding_trust` says so outright —
/// *"Content present ≈ tool returned data"* — because it is inferred from the
/// field being empty rather than from the tool being asked. Two production runs
/// carry the identical grade and opposite findings: `genome_profiler` called
/// NCBI and there is no sequenced genome for the beetle, and `football_analyst`
/// never called `fixtures/statistics`, where the xG it reported as unavailable
/// lives.
///
/// Raised **only from an actual tool run**, never inferred from a grade. The
/// evidence is the point: without it this is one more opinion about an agent,
/// and with it, it is a fact that can become a correction.
pub const KIND_CONTRADICTED: &str = "contradicted";

/// Every kind `anomaly_events.kind` accepts.
pub const KINDS: &[&str] = &[
    KIND_DRIFT,
    KIND_ROLLING_CONFLICT,
    KIND_RUPTURE,
    KIND_SAFETY,
    KIND_GROUNDING,
    KIND_CONTRADICTED,
];

/// Recorded, not routed.
pub const SEV_INFO: &str = "info";
/// Reviewable. The default, and the right level for a defect in one output.
pub const SEV_WARNING: &str = "warning";
/// Reserved for what should interrupt someone.
pub const SEV_CRITICAL: &str = "critical";

/// Every severity `anomaly_events.severity` accepts.
pub const SEVERITIES: &[&str] = &[SEV_INFO, SEV_WARNING, SEV_CRITICAL];

/// Flag prefixes on `agent_timeline_entries.anomaly_flags` that a detector
/// matches, paired with the kind they raise.
///
/// The detector reads `{category}:{value}` strings written by the live scorer.
/// A producer that emits a shape not listed here writes a flag no detector
/// reads — a detector that cannot fire, and invisible, because an unmatched
/// flag looks exactly like a quiet one.
pub const ACTIONABLE_FLAG_PREFIXES: &[(&str, &str)] = &[
    ("safety:", KIND_SAFETY),
    ("conflict:", KIND_ROLLING_CONFLICT),
    ("rupture:", KIND_RUPTURE),
    ("drift:", KIND_DRIFT),
];

/// Flag prefixes that are deliberately not actionable, with the reason.
///
/// Declared so that the live vocabulary check can tell "bookkeeping" from "a
/// flag whose detector was never written, or whose name was mistyped". Without
/// this list the check would have to allow every unmatched prefix, which is the
/// same as not having it.
pub const BOOKKEEPING_FLAG_PREFIXES: &[(&str, &str)] = &[(
    "social:",
    "`social:observed` marks that the dyad pass already folded this exchange \
     into dyad_state, so pass 2 does not count it twice. It records work done, \
     not a defect found.",
)];

/// Does any detector match this flag?
pub fn is_actionable_flag(flag: &str) -> Option<&'static str> {
    ACTIONABLE_FLAG_PREFIXES
        .iter()
        .find(|(p, _)| flag.starts_with(p))
        .map(|(_, kind)| *kind)
}

/// Is this flag declared as deliberately inert?
pub fn is_bookkeeping_flag(flag: &str) -> Option<&'static str> {
    BOOKKEEPING_FLAG_PREFIXES
        .iter()
        .find(|(p, _)| flag.starts_with(p))
        .map(|(_, why)| *why)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_severity_that_was_rejected_is_not_in_the_vocabulary() {
        // The regression, stated as the thing it is: `L1` reads like a severity
        // and is not one here, so a writer reaching for it has to pass through
        // this module and find out.
        assert!(!SEVERITIES.contains(&"L1"));
        assert!(SEVERITIES.contains(&SEV_WARNING));
    }

    #[test]
    fn the_grounding_kind_is_declared_even_though_no_detector_enum_has_it() {
        // `AnomalyKind` (observability crate) has four variants and this is the
        // fifth kind the table accepts. Enumerating kinds from that enum would
        // therefore under-count, and a probe built on it would have declared
        // the vocabulary sound while the only kind actually being written was
        // the one it could not see.
        assert!(KINDS.contains(&KIND_GROUNDING));
        assert_eq!(KINDS.len(), 6);
    }

    /// The two grounding kinds are opposites and must both exist.
    ///
    /// `grounding` is a field populated from nowhere. `contradicted` is a field
    /// left empty that the agent's own tool can fill. They arrive at the HITL
    /// queue as the same shape of row and imply opposite corrections — stop
    /// inventing, versus start asking — so a vocabulary carrying only one of
    /// them can describe only half of what grounding actually finds.
    #[test]
    fn fabricating_a_value_and_failing_to_fetch_one_are_different_kinds() {
        assert!(KINDS.contains(&KIND_CONTRADICTED));
        assert_ne!(KIND_GROUNDING, KIND_CONTRADICTED);
    }

    #[test]
    fn every_actionable_prefix_raises_a_declared_kind() {
        for (prefix, kind) in ACTIONABLE_FLAG_PREFIXES {
            assert!(
                KINDS.contains(kind),
                "{prefix} raises `{kind}`, which the table would reject"
            );
        }
    }

    #[test]
    fn a_flag_is_actionable_or_bookkeeping_but_not_both() {
        for (prefix, _) in ACTIONABLE_FLAG_PREFIXES {
            assert!(
                is_bookkeeping_flag(prefix).is_none(),
                "{prefix} is declared as both actionable and inert"
            );
        }
        // The one production flag, and it is inert on purpose.
        assert!(is_actionable_flag("social:observed").is_none());
        assert!(is_bookkeeping_flag("social:observed").is_some());
    }

    #[test]
    fn the_prefixes_the_detector_actually_matches_are_the_ones_declared() {
        // `detect_in_window_with_window` matches these four literals. Keeping
        // the list here in step with that function is manual, so it is asserted
        // in the shape a reader can check against the source in one glance.
        let declared: Vec<&str> = ACTIONABLE_FLAG_PREFIXES.iter().map(|(p, _)| *p).collect();
        assert_eq!(declared, vec!["safety:", "conflict:", "rupture:", "drift:"]);
    }

    #[test]
    fn every_bookkeeping_exemption_gives_a_reason() {
        for (prefix, why) in BOOKKEEPING_FLAG_PREFIXES {
            assert!(
                why.len() > 60,
                "{prefix} is exempted without a reason, which makes it permanent"
            );
        }
    }
}

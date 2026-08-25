//! What became of a run's quantified judgement, and why when the answer is
//! "nothing".
//!
//! # Why this is in the library
//!
//! The decision belongs to `handlers::workspace::agent_params_hook`, which lives
//! in the `api-server` binary. An integration test cannot reach it, so the
//! decision could not be registered in `tests/falsification_registry.rs` — and
//! the rule that registry now enforces is *do not add a decision without a
//! falsification*. A registry whose author exempted his own work would be worth
//! nothing.
//!
//! This is the same module boundary, and the same remedy, as
//! [`crate::projection_commit`]: a pure function stranded in the binary, moved
//! to the library and re-exported, so the layer that owns the decision is the
//! layer that can be tested.
//!
//! # The defect it replaces
//!
//! `apply_agent_multipliers` returned `Result<bool, String>` and the caller in
//! `execution.rs` discarded the false case with:
//!
//! ```ignore
//! Ok(false) => {} // no multiplier found, nothing to do
//! ```
//!
//! `Ok(false)` covered three different states, and the comment named only one of
//! them. `forecast_agent_claims` has held zero rows since migration 187 and
//! Loop 4 stalls on it — so the first *bound* run that still produces no claim
//! is the observation the loop has been waiting for, and under a `bool` it would
//! have arrived silent and indistinguishable from the 65 unbound runs before it.

use uuid::Uuid;

/// The `invocation` key carrying the forecast a run was commissioned for.
///
/// # The seam this closes
///
/// The Fermi Console serialises [`ClaimBinding`]'s two halves out of
/// `negotiate::InvocationProvenance`, whose serde field names are `forecast_id`
/// and `driver`, both `skip_serializing_if = "Option::is_none"` — so an absent
/// half is an absent *key*. The server read them back as string literals typed
/// separately in `execution.rs` and `execution_stream.rs`.
///
/// Four independent spellings, across two crates, and nothing comparing them.
/// A rename on either side yields zero claims, silently, which is
/// indistinguishable from "no forecast-bound run has happened" — the state
/// `forecast_agent_claims` has been in since migration 187 created it.
///
/// Declared here, read by [`binding_from_invocation`], and pinned against the
/// console's own serialisation by
/// `crates/fermi-console/tests/invocation_envelope.rs`.
pub const KEY_FORECAST_ID: &str = "forecast_id";
/// The `invocation` key carrying the driver the run was researching.
pub const KEY_DRIVER: &str = "driver";

/// Recover a [`ClaimBinding`] from a request's `invocation` envelope.
///
/// `workspace_id` is never on the wire — the handler knows it from the tool
/// context or not at all — so this fills the two halves a caller can state and
/// leaves the third to the caller of this function.
///
/// Tolerant throughout: a missing, non-object or wrongly-typed field means "no
/// forecast binding", never a failure, because a caller that says nothing about
/// a forecast is the normal case.
///
/// # Why this one duplication was worth removing
///
/// `execution_stream.rs` says of its neighbours: *"deliberately mirrors
/// `execution.rs` rather than sharing a helper — the thing worth preventing is
/// not the duplication but the two paths silently DIVERGING."* That reasoning
/// holds for the episode, credit and royalty logic around it, which differ in
/// ways a shared helper would have to paper over.
///
/// It does not hold for reading two keys off a JSON object. There the
/// divergence *is* the failure, it is invisible, and there is nothing for a
/// shared function to paper over — so this is one function and the keys are one
/// constant each.
pub fn binding_from_invocation(invocation: Option<&serde_json::Value>) -> ClaimBinding {
    let Some(obj) = invocation.and_then(|v| v.as_object()) else {
        return ClaimBinding::default();
    };
    let field = |k: &str| {
        obj.get(k)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    ClaimBinding {
        workspace_id: None,
        forecast_id: field(KEY_FORECAST_ID),
        driver: field(KEY_DRIVER),
    }
}

/// What this run's claim can be attached to.
///
/// A claim is only worth writing if attribution can later find it, and what
/// makes it findable is its binding. `load_agent_claims`
/// (`handlers::attribution`) accepts either, and *prefers* the forecast:
/// `ORDER BY driver, (forecast_id = $1) DESC` — an explicit binding beats the
/// (workspace, driver, as-of) temporal inference.
///
/// Both may be present; at least one must be, which mig-213 enforces as
/// `forecast_agent_claims_has_binding`. An unbound claim is unreachable by
/// either arm of that reader's WHERE clause and so is indistinguishable from a
/// claim that was never written.
#[derive(Debug, Clone, Default)]
pub struct ClaimBinding {
    pub workspace_id: Option<Uuid>,
    pub forecast_id: Option<String>,
    /// The single driver the caller KNOWS this run was researching.
    ///
    /// When present it replaces `resolve_driver_prefixes` entirely rather than
    /// adding to it. Inference is strictly worse information than a statement,
    /// and the specific harm is on record: `football_analyst` covers three
    /// drivers, so the inferred path copies ONE multiplier into three rows and
    /// records three judgements where one was made — see
    /// `assertions::is_restatement` and migration 205's header, which record the
    /// same defect from the extractor's side.
    pub driver: Option<String>,
}

/// What became of a run's quantified judgement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimOutcome {
    /// Attempted, one claim per driver the judgement was bound to.
    ///
    /// "Attempted", not "landed": the INSERT is best-effort and counted by
    /// [`crate::write_accounting::Sink::ForecastAgentClaims`], which is the
    /// layer that owns the question of whether it was refused. Two answers to
    /// that would be one too many.
    Recorded { drivers: usize },
    /// Neither a stated driver nor a workspace to read a program from.
    ///
    /// **The state all 65 quantified judgements on file are in.** Not a fault
    /// in the hook: `execution.rs` will not spawn it without a binding at all,
    /// so this is reachable only for a run that has a binding and no driver.
    Unbound,
    /// A workspace, and its program binds no driver to this agent.
    ///
    /// A configuration fault and the most actionable of the three: the agent ran
    /// in a workspace whose FPL does not mention it, so nothing it claims can
    /// reach a parameter.
    NoDriverForAgent,
    /// A driver to bind to, and no recoverable judgement in the evidence.
    ///
    /// The model's doing rather than the platform's — but worth distinguishing,
    /// because the last time these two were conflated the cause was a pattern
    /// that could not read `**1.15**`, and it cost 12 of the 22 lines this
    /// platform had produced.
    NoJudgement { evidence_blocks: usize },
}

impl ClaimOutcome {
    /// One word for a log field, so the three silent states are greppable.
    pub fn label(self) -> &'static str {
        match self {
            ClaimOutcome::Recorded { .. } => "recorded",
            ClaimOutcome::Unbound => "unbound",
            ClaimOutcome::NoDriverForAgent => "no_driver_for_agent",
            ClaimOutcome::NoJudgement { .. } => "no_judgement",
        }
    }

    /// Was a claim written for this run?
    pub fn recorded(self) -> bool {
        matches!(self, ClaimOutcome::Recorded { .. })
    }
}

/// The decision, separated from the two queries that feed it.
///
/// Pure, so the four outcomes are a table a reader can check rather than a path
/// through an async function with two round trips in it.
///
/// `driver_prefixes` and `assertions` are counts because that is all the
/// decision uses. Passing the vectors would invite the classification to start
/// reading them and become two decisions in one function, which is how the
/// `bool` came to mean three things.
pub fn classify_claim(
    binding: &ClaimBinding,
    driver_prefixes: usize,
    assertions: usize,
    evidence_blocks: usize,
) -> ClaimOutcome {
    if driver_prefixes == 0 {
        // Which kind of nothing. A workspace was consulted and had no driver
        // for this agent — a fault someone can fix — versus a request that
        // named neither, which is a fact about the caller.
        return if binding.workspace_id.is_some() {
            ClaimOutcome::NoDriverForAgent
        } else {
            ClaimOutcome::Unbound
        };
    }
    if assertions == 0 {
        return ClaimOutcome::NoJudgement { evidence_blocks };
    }
    ClaimOutcome::Recorded {
        drivers: driver_prefixes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bound_to_workspace() -> ClaimBinding {
        ClaimBinding {
            workspace_id: Some(Uuid::from_u128(1)),
            forecast_id: None,
            driver: None,
        }
    }

    fn bound_to_forecast() -> ClaimBinding {
        ClaimBinding {
            workspace_id: None,
            forecast_id: Some("fc-1".into()),
            driver: None,
        }
    }

    /// The four states the `bool` could not tell apart.
    #[test]
    fn the_three_silent_outcomes_are_distinguishable() {
        // A workspace was consulted and its program binds no driver to this
        // agent. Actionable: the FPL does not mention it.
        assert_eq!(
            classify_claim(&bound_to_workspace(), 0, 0, 2),
            ClaimOutcome::NoDriverForAgent
        );
        // No workspace to read a program from and no driver stated. The state
        // every quantified judgement on file is in.
        assert_eq!(
            classify_claim(&bound_to_forecast(), 0, 0, 2),
            ClaimOutcome::Unbound
        );
        // Somewhere to put it, and the model wrote no number.
        assert_eq!(
            classify_claim(&bound_to_forecast(), 1, 0, 2),
            ClaimOutcome::NoJudgement { evidence_blocks: 2 }
        );
        // Both: one claim per driver.
        assert_eq!(
            classify_claim(&bound_to_workspace(), 3, 1, 2),
            ClaimOutcome::Recorded { drivers: 3 }
        );

        // Four outcomes, four labels. A collision would put two states back in
        // one log field, which is the defect this replaced.
        let labels = [
            ClaimOutcome::Recorded { drivers: 1 }.label(),
            ClaimOutcome::Unbound.label(),
            ClaimOutcome::NoDriverForAgent.label(),
            ClaimOutcome::NoJudgement { evidence_blocks: 0 }.label(),
        ];
        let uniq: std::collections::HashSet<_> = labels.iter().collect();
        assert_eq!(uniq.len(), labels.len(), "two outcomes share a label");
    }

    /// Only `Recorded` counts as a claim.
    ///
    /// The old `bool` returned `true` from the forecast-bound early return and
    /// `true` from the end of the workspace path, and `false` from two places
    /// that meant different things. `recorded()` must not quietly re-acquire
    /// that shape by treating a declined outcome as success.
    #[test]
    fn a_declined_outcome_is_not_a_recorded_one() {
        assert!(ClaimOutcome::Recorded { drivers: 1 }.recorded());
        assert!(!ClaimOutcome::Unbound.recorded());
        assert!(!ClaimOutcome::NoDriverForAgent.recorded());
        assert!(!ClaimOutcome::NoJudgement { evidence_blocks: 9 }.recorded());
    }

    /// Both halves survive the envelope, and neither is invented.
    #[test]
    fn the_invocation_envelope_yields_both_halves_or_neither() {
        let full = serde_json::json!({
            "forecast_id": "fc-1",
            "driver": "gdp_growth",
            "query_source": "user_authored"
        });
        let b = binding_from_invocation(Some(&full));
        assert_eq!(b.forecast_id.as_deref(), Some("fc-1"));
        assert_eq!(b.driver.as_deref(), Some("gdp_growth"));
        assert!(
            b.workspace_id.is_none(),
            "workspace_id is never on the wire"
        );

        // An absent key is the console's representation of `None`
        // (`skip_serializing_if`), so it must read as `None` and not as a
        // binding.
        let half = serde_json::json!({ "forecast_id": "fc-1" });
        let b = binding_from_invocation(Some(&half));
        assert_eq!(b.forecast_id.as_deref(), Some("fc-1"));
        assert!(b.driver.is_none());

        // Whitespace is not a binding. A driver of `"  "` would satisfy
        // `is_some()` at the gate and then resolve to a prefix of nothing.
        let blank = serde_json::json!({ "forecast_id": "  ", "driver": "" });
        let b = binding_from_invocation(Some(&blank));
        assert!(b.forecast_id.is_none() && b.driver.is_none());

        // Not an object, and absent, are both "no binding" and never a panic.
        assert!(binding_from_invocation(None).forecast_id.is_none());
        assert!(binding_from_invocation(Some(&serde_json::json!("nope")))
            .driver
            .is_none());
        assert!(
            binding_from_invocation(Some(&serde_json::json!({ "driver": 7 })))
                .driver
                .is_none()
        );
    }

    /// A workspace binding outranks the absence of a driver statement.
    ///
    /// Both `Unbound` and `NoDriverForAgent` come from `driver_prefixes == 0`,
    /// and only the binding distinguishes them. Asserted because a reader
    /// skimming the function sees one `if` and could reasonably collapse the
    /// two — which would put the actionable case back under the inert one.
    #[test]
    fn the_two_kinds_of_nothing_are_told_apart_by_the_binding_alone() {
        let with_ws = ClaimBinding {
            workspace_id: Some(Uuid::nil()),
            forecast_id: Some("fc-1".into()),
            driver: None,
        };
        let without_ws = ClaimBinding {
            workspace_id: None,
            forecast_id: Some("fc-1".into()),
            driver: None,
        };
        assert_eq!(
            classify_claim(&with_ws, 0, 0, 0),
            ClaimOutcome::NoDriverForAgent
        );
        assert_eq!(classify_claim(&without_ws, 0, 0, 0), ClaimOutcome::Unbound);
    }
}

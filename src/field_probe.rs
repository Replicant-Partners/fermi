//! Running the tool a field contract names.
//!
//! # Why
//!
//! A contract says which tool could settle a field:
//!
//! ```text
//! FieldContract {
//!     agent_id: "football_analyst",
//!     path: "head_to_head",
//!     grounding: Grounding::Sourced { tool: "call_football_api", .. },
//! }
//! ```
//!
//! and the trace printed `call_football_api` beside the row and offered no way to
//! run it. Sixteen tools are named across the contracts, on rows a reader can do
//! nothing about. **A name the platform can print and cannot offer is a
//! description, not an affordance**, and the screen was made of them.
//!
//! # What this does NOT do, and why not
//!
//! It does not decide anything. It runs the tool and hands back what came out.
//!
//! The temptation is to compare the tool's answer to the agent's claim and write
//! a verdict, and it cannot be done honestly, because the contract does not say
//! **where in the response** the value lives. `response_field` is prose:
//!
//! ```text
//! response_field: "standings (rank, points, form, home/away splits)"
//! response_field: "fixtures/headtohead"
//! ```
//!
//! One of those is an endpoint path and the other is a sentence. Matching a
//! claimed number against a response by looking for it is string-matching
//! dressed as verification — the same move that produced the genome error, one
//! layer along. So the platform performs the retrieval, and a person performs the
//! comparison, and the settle form is right there to record what they concluded.
//!
//! It also cannot fill in the query. `call_football_api` wants
//! `{endpoint, params}` with a league id, a season and a team id; those come from
//! what the episode was **about**, not from the contract. The caller supplies
//! them, and the UI says so rather than pretending to know.
//!
//! # The narrow door
//!
//! Only tools that need no [`ToolContext`] — no workspace, no memory store, no
//! credentials of ours, no delegation. A read-only surface must not be the door
//! to any of those. See [`crate::agent_backend::tools::CONTEXT_FREE_TOOLS`].

use serde_json::Value;

/// The tool a contract names for this field, if it names one.
///
/// Read from the contract rather than accepted from the caller. A probe endpoint
/// that ran whatever tool the request asked for would be a general-purpose
/// outbound HTTP proxy with an audit trail that said "field verification".
pub fn declared_tool(agent_id: &str, path: &str) -> Option<&'static str> {
    crate::grounding_trust::contracts_for(agent_id)
        .find(|c| c.path == path)
        .and_then(|c| match c.grounding {
            crate::grounding_trust::Grounding::Sourced { tool, .. } => Some(tool),
            _ => None,
        })
}

/// What the contract says the answer lives in, when it says anything.
///
/// Prose as often as not, which is why it is surfaced to the caller as a hint
/// rather than used to build the call. Where it happens to be an endpoint path —
/// `fixtures/headtohead` — the UI can prefill it, and where it is a sentence the
/// caller reads it and decides.
pub fn response_hint(agent_id: &str, path: &str) -> Option<&'static str> {
    crate::grounding_trust::contracts_for(agent_id)
        .find(|c| c.path == path)
        .and_then(|c| match c.grounding {
            crate::grounding_trust::Grounding::Sourced { response_field, .. } => {
                Some(response_field)
            }
            _ => None,
        })
}

/// Can this field's tool actually be run from a surface?
pub fn is_runnable(tool: &str) -> bool {
    crate::agent_backend::tools::is_context_free(tool)
}

/// How much of a tool response travels back.
///
/// API-Football returns whole seasons. The caller is reading it to decide one
/// field, and a megabyte through the JSON encoder to answer that is a bad trade —
/// but a silent truncation is worse, so the outcome says when it cut.
pub const RESPONSE_CHARS: usize = 12_000;

/// The outcome of running a named tool.
#[derive(Debug, serde::Serialize)]
pub struct Probe {
    pub tool: &'static str,
    /// `true` when the tool returned. **Not** a verdict about the field: a tool
    /// can answer perfectly and still have nothing for this fixture, which is
    /// what `tool_no_match` exists to say.
    pub ok: bool,
    pub response: String,
    pub truncated: bool,
    pub chars: usize,
}

/// Run the tool the contract names for this field.
pub async fn run(tool: &'static str, input: &Value) -> Probe {
    let (ok, body) = match crate::agent_backend::tools::execute_context_free(tool, input).await {
        Ok(s) => (true, s),
        // The error is the answer here, and it is often the useful one: a
        // missing API key, a refused endpoint, a rate limit. Returned rather
        // than logged, because the person who clicked is the person who needs it.
        Err(e) => (false, e),
    };
    let chars = body.chars().count();
    Probe {
        tool,
        ok,
        response: body.chars().take(RESPONSE_CHARS).collect(),
        truncated: chars > RESPONSE_CHARS,
        chars,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The runnable list and the dispatcher must agree.
    ///
    /// A surface offers a button for every tool in `CONTEXT_FREE_TOOLS`. If the
    /// dispatcher then refuses one, the refusal arrives after the click — which
    /// is worse than never offering it, because the reader has already decided
    /// the platform could do the thing.
    #[tokio::test]
    async fn every_offered_tool_is_actually_dispatchable() {
        for tool in crate::agent_backend::tools::CONTEXT_FREE_TOOLS {
            // An empty input: every one of these must fail on a MISSING
            // PARAMETER or a missing key, never on being unknown. The unknown
            // branch is the one that means the two lists have drifted.
            let out =
                crate::agent_backend::tools::execute_context_free(tool, &serde_json::json!({}))
                    .await;
            if let Err(e) = out {
                assert!(
                    !e.contains("cannot be run from here"),
                    "`{tool}` is offered as runnable and the dispatcher does not \
                     know it. The button would be refused after the click."
                );
            }
        }
    }

    /// Which contract-named tools a reader can actually run, and which cannot.
    ///
    /// The number that matters: every tool a contract names is printed on the
    /// trace beside a row, so the ones that are not runnable are precisely the
    /// rows where the name is still a description.
    ///
    /// The unrunnable set is pinned rather than counted, because each entry has
    /// a different reason and the reasons are the useful part. If the list
    /// shrinks, a row somewhere gained a button and this fails so the change is
    /// noticed; if it grows, a contract has started naming something a surface
    /// can never offer.
    #[test]
    fn every_contract_named_tool_is_runnable_or_says_why_not() {
        /// Named by a contract, and not reachable from a read-only surface.
        ///
        /// Alphabetical, because the set it is compared against comes from a
        /// `BTreeSet` and a hand-ordered list would fail on ordering rather than
        /// on the thing this test is about.
        const NEEDS_CONTEXT: &[(&str, &str)] = &[
            (
                "reduct_add_block",
                "writes to a Reduct project on the agent owner's credentials",
            ),
            (
                "reduct_create_reel",
                "writes to a Reduct project on the agent owner's credentials",
            ),
            (
                "reduct_get_project",
                "reads a Reduct project on the agent owner's credentials",
            ),
            (
                "reduct_get_transcript",
                "reads a Reduct project on the agent owner's credentials",
            ),
            (
                "scan_nearby_creatures",
                "reads the caller's creature and its neighbourhood out of the \
                 memory store, so it has no meaning without a ToolContext",
            ),
        ];

        let named: std::collections::BTreeSet<&str> = crate::grounding_trust::FIELD_CONTRACTS
            .iter()
            .filter_map(|c| match c.grounding {
                crate::grounding_trust::Grounding::Sourced { tool, .. } => Some(tool),
                _ => None,
            })
            .collect();

        let blocked: Vec<&str> = named.iter().copied().filter(|t| !is_runnable(t)).collect();
        let declared: Vec<&str> = NEEDS_CONTEXT.iter().map(|(t, _)| *t).collect();

        assert_eq!(
            blocked, declared,
            "the set of contract-named tools that cannot be run from a surface \
             has changed. Every one of these is printed on the trace beside a \
             row, so an entry here is a row where the tool's name is a \
             description and nothing more."
        );

        for (tool, why) in NEEDS_CONTEXT {
            assert!(
                why.len() > 30,
                "{tool} is excluded with no real reason, and \"it needs context\" \
                 is what would be assumed rather than checked"
            );
        }

        // And the useful figure, asserted so it cannot quietly regress.
        let runnable = named.len() - blocked.len();
        assert!(
            runnable >= 11,
            "only {runnable} of {} contract-named tools can be run from a \
             surface; it was 11",
            named.len()
        );
    }

    /// The tool is read from the contract, never from the request.
    #[test]
    fn the_tool_comes_from_the_contract() {
        assert_eq!(
            declared_tool("football_analyst", "head_to_head"),
            Some("call_football_api")
        );
        assert_eq!(declared_tool("football_analyst", "no_such_field"), None);
        assert_eq!(declared_tool("no_such_agent", "head_to_head"), None);
    }
}

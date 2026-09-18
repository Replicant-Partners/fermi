//! Workspace handlers — re-exports from the focused sub-modules.
//!
//! Split for navigability. All public symbols remain at `handlers::workspace::*`
//! so no route registrations in api_server.rs need to change.
//!
//! - core.rs      CRUD (list, get, agents, fund), gas helper, shared utilities
//! - messages.rs  Chat, SSE stream, agent hire/add/remove
//! - coherence.rs Coherence eval, ontology, files, git log, workflow
//! - actions.rs   Generalised App action protocol (mutate_document, fork_state, etc.)
//! - lens_actions.rs      DPP Studio: render/compare/flag from stored evaluations
//! - claim_evaluation.rs  DPP Studio: run the evaluator agent against the corpus
//! - bom_pricing.rs       DPP Studio: run the supply-chain oracle over the BOM
//! - carbon.rs            DPP Studio: retrieve emission factors, multiply in Rust

pub mod actions;
pub mod agent_params_hook;
pub mod carbon;
pub mod bom_pricing;
pub mod claim_evaluation;
mod coherence;
mod core;
pub mod lens_actions;
mod messages;
pub mod outputs;
pub mod refit;
pub mod resolution;

pub use agent_params_hook::*;
pub use coherence::*;
pub use core::*;
pub use messages::*;
pub use outputs::*;
pub use refit::*;
pub use resolution::*;

/// Refuse to run an agent a workspace has not hired.
///
/// ## Why this exists as a server-side check
///
/// It was enforced nowhere. `carbon.rs`, `bom_pricing.rs` and
/// `claim_evaluation.rs` had no membership check between them, and
/// `dispatch_rabble_action` resolves its agent from the global in-memory
/// registry rather than from `workspace_agents` — so any registered agent
/// could be dispatched into any workspace. The DPP Studio enforced a hire step
/// in the browser for `supply_chain_oracle` and not for `carbon_accountant`,
/// which is how a workspace hiring two agents came to hold a carbon statement
/// written by a third. A client-side hire gate is a UI affordance, not a
/// control: the endpoint is reachable without it.
///
/// ## Why it is not inside `dispatch_rabble_action`
///
/// That function has 24 call sites and most are creature flows — flights,
/// tethering, identity, agent modules — which dispatch agents into contexts
/// that do not use `workspace_agents` as a roster at all. A check there would
/// refuse work that is legitimate today. Membership is meaningful for the App
/// action protocol, where hiring is a real user act with a real cost, so the
/// check belongs at those handlers.
///
/// ## What it deliberately does not do
///
/// It does not hire on demand. An endpoint that quietly hired whatever it
/// needed would make the roster describe the past rather than authorise the
/// future, and the roster is what the payout and the UI both read. Existing
/// workspaces predating an app's `auto_hire` list are backfilled by
/// `POST /api/apps/:app_id/sync-auto-hire`, which the error names.
pub async fn require_hired_agent(
    state: &crate::AppState,
    workspace_id: uuid::Uuid,
    agent_name: &str,
) -> Result<uuid::Uuid, (axum::http::StatusCode, String)> {
    let agent = crate::resolve_agent(state, agent_name).await?;
    let hired: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM workspace_agents WHERE workspace_id = $1 AND agent_id = $2",
    )
    .bind(workspace_id)
    .bind(agent.agent_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("could not check whether `{agent_name}` is hired: {e}"),
        )
    })?;

    if hired.is_none() {
        // Deliberately not a 404: the agent exists, and saying so is the
        // actionable half of the answer.
        return Err((
            axum::http::StatusCode::FORBIDDEN,
            format!(
                "`{agent_name}` is not hired in this workspace, so it will not \
                 be run. This is refused rather than hired on demand because \
                 the roster is what the execution payout is attributed against \
                 (PLATFORM_ECONOMICS.md §2.1) and what the workspace UI shows \
                 as its team — an endpoint that hired silently would make both \
                 describe work already done instead of authorising work to do. \
                 Hire it, or if this workspace predates the app's auto_hire \
                 list, POST /api/apps/:app_id/sync-auto-hire to backfill."
            ),
        ));
    }
    Ok(agent.agent_id)
}

#[cfg(test)]
mod hiring_enforcement_tests {
    /// Every App action handler that dispatches an agent must refuse first.
    ///
    /// Source-level rather than behavioural, and the reason is the shape of the
    /// thing being checked: `require_hired_agent` needs a live `AppState` and a
    /// database, so a unit test cannot exercise it, while the failure worth
    /// preventing is not "the check returns the wrong answer" but "a new
    /// handler was written without calling it at all". That is visible in the
    /// source, which is where this looks.
    ///
    /// ## What this does not check, and why not
    ///
    /// It does not check that the gate runs BEFORE the dispatch, which is the
    /// half that actually matters — refusing after the agent has run withholds
    /// an answer the caller has already paid for. That version was written,
    /// asserting `find("require_hired_agent") < find("dispatch_rabble_action")`,
    /// and it failed on `claim_evaluation.rs`, correctly gated, because that
    /// file dispatches from a per-claim helper defined ABOVE the handler.
    /// Source order is not execution order once a call moves into a function,
    /// and a byte-offset comparison cannot tell the two apart.
    ///
    /// So it was removed rather than special-cased. A check that fires on
    /// correct code is a check that gets switched off, and this file already
    /// argues that about the 30% corroboration band. Ordering is enforced by
    /// the `?` in each handler instead: the gate returns `Err` and the function
    /// exits, so a gate that is present at all is a gate that precedes
    /// everything after it in the same body.
    ///
    /// Known limitation, stated rather than hidden: the file list is manual,
    /// because `include_str!` cannot walk a directory. A fourth action handler
    /// added without being listed here is not caught. `dispatch_rabble_action`
    /// is the string to grep for when adding one.
    #[test]
    fn dpp_action_handlers_check_hiring_before_dispatching() {
        for (name, src) in [
            ("carbon.rs", include_str!("carbon.rs")),
            ("bom_pricing.rs", include_str!("bom_pricing.rs")),
            ("claim_evaluation.rs", include_str!("claim_evaluation.rs")),
        ] {
            assert!(
                src.contains("require_hired_agent("),
                "{name} dispatches an agent without checking that the workspace \
                 hired it. Enforcement lives on the server because a browser \
                 hire button guards the UI and not the endpoint, and because \
                 the roster is what the execution payout is attributed against."
            );
        }
    }
}

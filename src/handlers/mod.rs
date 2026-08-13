//! Handler modules — organized by domain.
//!
//! New handlers go into focused modules here rather than api_server.rs.
//! Shared helpers (resolve_agent, resolve_agent_card, create_notification)
//! live in api_server.rs as pub(crate) functions.

/// Is this row test cruft rather than a real agent?
///
/// Integration tests have been inserting `test_agent_<uuid>` rows into the
/// shared database for a long time (v0.10.20's audit found 565). Several
/// surfaces filtered them out with an inline
/// `!name.starts_with("test_agent_")`, and several — notably the
/// Observatory fleet endpoints — did not, which is why the clinical view
/// opens on a wall of `test_agent_*` entries instead of the operator's
/// actual agents.
///
/// One definition, so the next surface can't drift. Note this only hides
/// them; deleting them is `/api/admin/agents/cleanup-test-cruft`, which is
/// safety-gated (zero executions, past a grace period, never curated or
/// system tier).
pub fn is_test_cruft(agent_name: &str) -> bool {
    agent_name.starts_with("test_agent_")
}

pub mod admin;
pub mod admin_rbac;
pub mod agent_funding;
pub mod agent_wallet;
pub mod agents;
/// Driver annotations — objections anchored to a specific assumption.
/// See docs/specs/SPEC_32_DRIVER_ANNOTATIONS.md.
pub mod annotations;
pub mod apps;
pub mod attribution;
pub mod auth;
pub mod bayesops;
pub mod beacons;
pub mod billing;
/// Team collaboration surfaces — share provenance, actor attribution,
/// activity feeds. See docs/specs/SPEC_26_TEAM_COLLABORATION.md.
pub mod collab;
pub mod composition;
pub mod composition_evolution;
pub mod consolidation;
pub mod creatures;
pub mod dashboard;
pub mod dreaming_maturity;
/// Ecology — population, habitats, niches and governance provenance.
/// The "what lives here" lens, sibling to the Observatory's clinical one.
pub mod ecology;
/// Platform economics — real LLM cost vs. credit revenue, attributed by
/// the funding principal recorded at execution time (SPEC_28).
/// See docs/plans/PLATFORM_ECONOMICS.md.
pub mod economics;
pub mod eval;
pub mod eval_brier;
pub mod eval_judge;
pub mod eval_projection;
pub mod evolution;
pub mod execution;
pub mod execution_stream;
pub mod forecast_benchmark;
/// Forecast version history on the workspace git substrate — history,
/// diff, revert. See docs/specs/SPEC_31_FORECAST_HISTORY.md.
pub mod forecast_git;
pub mod forecasts;
pub mod governance;
/// Admin "view as user" — short-lived, read-only, fully audited
/// impersonation for support and debugging.
/// See docs/specs/SPEC_33_IMPERSONATION.md.
pub mod impersonation;
pub mod invites;
pub mod kg;
pub mod lifecycle;
pub mod marketplace;
pub mod mcp;
pub mod metrics;
pub mod misc;
pub mod notebooks;
pub mod observations;
pub mod observatory;
pub mod ontology;
/// The ops board — detected coordination work for a team.
/// See docs/specs/SPEC_27_TEAM_OPS.md.
pub mod ops;
pub mod orchestras;
pub mod pages;
pub mod pending_cascades;
pub mod polymarket;
pub mod profile;
pub mod push;
pub mod qr_codes;
pub mod rabble_chat;
pub mod rabble_workspace;
pub mod rbac_self_check;
pub mod relationships;
pub mod shares;
pub mod simops;
pub mod simops_benchmark;
pub mod social;
pub mod streams;
pub mod swarm_algorithms;
pub mod swarm_telemetry;
pub mod teams;
pub mod users;
pub mod wallet;
pub mod wizard;
pub mod workspace;
pub mod xaman;

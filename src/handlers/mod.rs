//! Handler modules — organized by domain.
//!
//! New handlers go into focused modules here rather than api_server.rs.
//! Shared helpers (resolve_agent, resolve_agent_card, create_notification)
//! live in api_server.rs as pub(crate) functions.

/// Is this row test cruft rather than a real agent?
///
/// **Re-exported, not defined here.** The definition moved to
/// [`fermi::declaration_ladder::is_test_cruft`] when the declaration ladder
/// needed it: 110 of the 206 agents that have produced an episode are
/// `test_agent_*` rows declaring nothing at all, so the predicate is now
/// load-bearing for *which worklist an agent belongs to* — pruning cruft and
/// retrofitting a real agent are different efforts with different owners, and
/// mixing them makes the retrofit look twice its size.
///
/// The ladder lives in the library and the five callers here are in the binary,
/// so the choice was one definition in the library or two definitions. Kept as a
/// re-export rather than moving the call sites, because the call sites are the
/// point: several surfaces once filtered inline and several — notably the
/// Observatory fleet endpoints — did not, which is why the clinical view opened
/// on a wall of `test_agent_*` entries instead of the operator's actual agents.
///
/// This only hides them; deleting them is
/// `/api/admin/agents/cleanup-test-cruft`, which is safety-gated (zero
/// executions, past a grace period, never curated or system tier).
pub use fermi::declaration_ladder::is_test_cruft;

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
pub mod bestiary;
pub mod billing;
/// Team collaboration surfaces — share provenance, actor attribution,
/// activity feeds. See docs/specs/SPEC_26_TEAM_COLLABORATION.md.
pub mod collab;
pub mod composition;
pub mod composition_evolution;
pub mod consolidation;
/// Compiling an output-contract sketch for the create wizard — the same
/// code path the publish gate uses, so the two cannot give different
/// answers. See `src/contract_sketch.rs`.
pub mod contracts;
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
// Gates — what the platform refused, and what it cannot refuse. The register,
// the enforcement map (which verbs a gate can actually stop), and the durable
// receipt record. Before this the platform had a record of every request it
// served and none of any it refused.
pub mod gates;
pub mod governance;
/// Admin "view as user" — short-lived, read-only, fully audited
/// impersonation for support and debugging.
/// See docs/specs/SPEC_33_IMPERSONATION.md.
pub mod impersonation;
pub mod invites;
pub mod kg;
pub mod lifecycle;
pub mod live_observability;
pub mod marketplace;
pub mod mcp;
pub mod metrics;
pub mod misc;
pub mod notebooks;
pub mod observations;
// The loop surface, for people. One shape over `loop_model` +
// `panel_absence` + `outcome_trust`, replacing an admin diagnostics blob and a
// 610-line second answer to the first one's question.
pub mod loops;
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

// Rounds — what needs you, in what order. The dashboard is a directory with ~30
// actions at equal weight; a round is an ordered visit to whoever needs
// attention. Also the first surface that shows the platform's own blind spots,
// by rendering panel_absence through panel_contract at scan density.
pub mod relationships;
pub mod rounds;
pub mod shares;
pub mod simops;
pub mod simops_benchmark;
pub mod social;

// One specimen, three tabs. Replaces the eight-tab agent page, whose inventory
// found thirteen metrics rendered in more than one place and several under
// different names for the same number. Composed server-side so there is one
// producer per number, and absent rather than zero where nothing was measured.
pub mod specimen;
pub mod streams;
pub mod swarm_algorithms;
pub mod swarm_telemetry;
pub mod teams;
pub mod users;
pub mod wallet;
pub mod wild;
pub mod wizard;
pub mod workspace;
pub mod xaman;

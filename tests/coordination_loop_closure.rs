//! # Two things Loop 3 claimed and did not do
//!
//! Loop 3's claim is *"a composition notices its own incoherence and
//! coordinates out of it within a session"*, and it rests on two mechanisms
//! that were each broken in a way no existing test could see.
//!
//! ## 1. The coordinator did not fold its own knowledge back in
//!
//! `cohere_and_coordinate`'s card opens Stage 4 with *"Read consolidated
//! memory: review your past dreaming episodes for this workspace. What
//! coherence patterns recur? Which principles are chronically weak?"*
//!
//! Nothing was behind that. `handlers::workspace::coherence` was the only agent
//! execution path on the platform that called neither `enrich_with_kg_context`
//! (so no learned rule ever reached the strategist's prompt) nor
//! `agent_output_to_episode` (so no run of it ever became something to learn
//! from). A closed circle of zero: no episodes → nothing to consolidate → no
//! rules → nothing to retrieve. The agent appointed the platform's longitudinal
//! learner was the one agent excluded from longitudinal learning, and every
//! session it opened, it opened as its first.
//!
//! Invisible from every other surface. Loop 1's `episodes` stage counts rows
//! platform-wide and was never empty; nothing asked *which agents* were
//! producing them, and an agent that writes none is indistinguishable from one
//! that has not run.
//!
//! ## 2. The coordinator did not ask agents for their plans
//!
//! Stage 0 called `declare_intention` once per member, describing what the
//! strategist *supposed* each was about to do from a twenty-message transcript.
//! No member was ever asked. `workspace_intentions` had no column recording who
//! wrote a row, so a member's own plan and the coordinator's guess about it were
//! byte-identical, and the conflict checker compared guesses to each other.
//!
//! ReMALIS (arXiv:2407.12532 §3.1) separates the two objects: agent *i* holds a
//! private intention `I_i = (γ_i, Σ_i, π_i, δ_i)`; what another party can hold
//! is a belief `b_j(I_i | m_ji)` formed from a message *i* actually sent. §4.4
//! Table 3 prices the difference — 31%/23%/17% sub-task alignment with no
//! communication against 91%/71%/62% with full intention sharing. Declaring on
//! an agent's behalf is the no-communication row wearing the full-sharing row's
//! vocabulary.
//!
//! ## Why these are source scans
//!
//! Both properties are "this handler does a thing", cross-file and behind an
//! axum handler needing an `AppState`, a database, credits and a live model.
//! `tests/execute_path_parity.rs` and `tests/gate_trust_coverage.rs` establish
//! the pattern for exactly this reason. A source scan is the weaker instrument
//! and is chosen knowingly: what it can do is stop a fixed path silently
//! reverting, which is the failure that actually happens.

use std::path::Path;

fn read(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

/// The file with its `use` declarations and comments removed.
///
/// **Not a nicety — the first version of this suite was vacuous without it.**
/// `handlers::workspace::coherence` already contained the string
/// `agent_output_to_episode` before any of this was fixed, in an import it
/// never called, and the comment beside the import said so out loud:
/// *"this handler imports `agent_output_to_episode` and never calls it"*. A
/// scan for the bare name would have passed against precisely the defect it
/// was written to catch, and passed again the moment someone deleted the call
/// and left the import — which is what rustc's unused-import warning nudges
/// people to do in the other direction and nothing stops here.
///
/// The same trap sits under `enrich_with_kg_context`, imported on its own line.
/// So: judge the code, not the header.
fn code_of(rel: &str) -> String {
    read(rel)
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with("use ") && !t.starts_with("//")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn card() -> serde_json::Value {
    serde_json::from_str(&read(
        "agents/curated/cohere_and_coordinate/agent_card.json",
    ))
    .expect("cohere_and_coordinate card is not valid JSON")
}

/// Every path that runs an agent against a user-visible query.
///
/// Named individually rather than globbed. A glob would have absorbed the
/// coherence handler silently, which is how it stayed the odd one out.
const AGENT_EXECUTION_PATHS: &[(&str, &str)] = &[
    ("POST /api/agents/:id/execute", "src/handlers/execution.rs"),
    (
        "POST /api/agents/:id/execute/stream",
        "src/handlers/execution_stream.rs",
    ),
    ("workspace @mention", "src/handlers/workspace/messages.rs"),
    (
        "workspace coherence strategist",
        "src/handlers/workspace/coherence.rs",
    ),
];

// ─── 1. knowledge folds back in ────────────────────────────────────────

/// Every execution path retrieves what the agent has learned.
///
/// Retrieval is the only step at which a consolidated rule changes behaviour.
/// A path that skips it runs an agent with a hundred learned rules exactly as
/// it runs one that has never dreamed.
#[test]
fn every_agent_execution_path_retrieves_learned_knowledge() {
    for (what, file) in AGENT_EXECUTION_PATHS {
        assert!(
            code_of(file).contains("enrich_with_kg_context("),
            "{what} ({file}) executes an agent without retrieving its semantic \
             rules. Loop 1's write half runs and its read half does not, so the \
             agent's own experience cannot reach its next prompt. (An import \
             does not count — see `code_of`.)"
        );
    }
}

/// Every execution path records the run as something to learn from.
///
/// The mirror of the test above, and the half that was missing for the
/// strategist. Retrieval with nothing to retrieve is not a working loop; it is
/// the same silence with more machinery behind it.
#[test]
fn every_agent_execution_path_persists_an_episode() {
    for (what, file) in AGENT_EXECUTION_PATHS {
        let src = code_of(file);
        assert!(
            src.contains("agent_output_to_episode("),
            "{what} ({file}) runs an agent and drops the run. Consolidation \
             reads `episodes`; a path that writes none gives that agent nothing \
             to dream on, and no amount of retrieval will help. (An import does \
             not count — this exact file once imported it and never called it.)"
        );
        assert!(
            src.contains("store_episode"),
            "{what} ({file}) builds an episode and never stores it. Constructing \
             the struct is not the write."
        );
    }
}

/// The strategist's run is a parent, not an orphan.
///
/// It delegates — `solicit_agent_plan` asks each member for a plan,
/// `execute_agent` invokes peers — and while its own run persisted no row those
/// children had nothing to point at and were recorded as roots. The cost of a
/// coordination session was scattered across unattributable episodes.
#[test]
fn delegations_from_the_strategist_hang_off_its_own_episode() {
    let src = code_of("src/handlers/workspace/coherence.rs");
    assert!(
        src.contains("parent_episode_id: Some(strategist_episode_id)"),
        "the strategist's ToolContext must carry its own pre-minted episode id, \
         or everything it delegates is recorded as a root with no caller"
    );
    assert!(
        src.contains("episode.episode_id = strategist_episode_id"),
        "the stored episode must use the id already advertised to the tool \
         context, or children stamped it with an id that resolves to nothing"
    );
}

// ─── 2. the coordinator asks ───────────────────────────────────────────

/// The card declares the asking tool and the runtime can dispatch it.
#[test]
fn the_strategist_can_ask_a_member_for_its_plan() {
    let c = card();
    let declared: Vec<&str> = c["capabilities"]["mcp_tools"]
        .as_array()
        .expect("mcp_tools")
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();

    assert!(
        declared.contains(&"solicit_agent_plan"),
        "cohere_and_coordinate must declare `solicit_agent_plan`. Without it \
         Stage 0 can only record what the strategist believes each member is \
         doing, and a map of beliefs checked against itself is not coordination."
    );

    let platform = fermi::agent_backend::tools::platform_tool_names();
    assert!(
        platform.contains(&"solicit_agent_plan"),
        "`solicit_agent_plan` must be a dispatchable platform tool, not a \
         phantom the model will confidently call"
    );
}

/// The six Stage 0 tools the prompt names must be on the card.
///
/// A different failure from the phantom-tool test, which checks
/// declared → dispatchable. This checks prompt → declared, and that direction
/// was unguarded: Stage 0 named all six intention tools and the card declared
/// none of them.
#[test]
fn every_stage_0_tool_the_prompt_names_is_declared() {
    let c = card();
    let prompt = c["system_prompt"].as_str().expect("system_prompt");
    let declared: Vec<&str> = c["capabilities"]["mcp_tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();

    for tool in [
        "get_intention_map",
        "declare_intention",
        "check_conflicts",
        "clear_intention",
        "suggest_differentiation",
        "emit_coherence_signal",
        "solicit_agent_plan",
    ] {
        assert!(
            prompt.contains(tool),
            "Stage 0 no longer names `{tool}`; if the stage changed, change this list"
        );
        assert!(
            declared.contains(&tool),
            "the prompt tells the agent to call `{tool}` and the card does not \
             declare it, so it is not in the tool list the model receives"
        );
    }
}

/// Stage 0 must lead with asking, and must say why inferring is weaker.
///
/// The prompt is the mechanism here — the tool exists either way, and what
/// decides whether Loop 3 propagates intentions or narrates them is which tool
/// the stage reaches for first.
#[test]
fn stage_0_leads_with_soliciting_rather_than_inferring() {
    let c = card();
    let prompt = c["system_prompt"].as_str().unwrap();
    let stage_0 = prompt
        .split("## Stage 1")
        .next()
        .expect("Stage 0 precedes Stage 1");

    let ask = stage_0
        .find("solicit_agent_plan")
        .expect("Stage 0 must mention solicit_agent_plan");
    let assume = stage_0
        .find("declare_intention")
        .expect("Stage 0 must still mention declare_intention as the fallback");
    assert!(
        ask < assume,
        "Stage 0 reaches for `declare_intention` before `solicit_agent_plan`. \
         Order is the instruction: the first tool named is the one that gets \
         called, and declaring on a member's behalf is the behaviour that made \
         every intention map on the platform one agent's guesswork."
    );

    assert!(
        stage_0.contains("inferred"),
        "Stage 0 must tell the agent that a plan it writes for someone else is \
         recorded as `inferred`, or it has no reason to prefer asking"
    );
}

/// Provenance is derived, never accepted from the caller.
///
/// A `source` argument on `declare_intention` would let the party with the most
/// reason to overstate it — a model told that first-hand rows are treated more
/// seriously — assert its own guess was a report. The platform knows who called
/// and about whom; it does not need to be told.
#[test]
fn intention_provenance_is_derived_from_the_caller() {
    let src = code_of("src/agent_backend/tools_legacy.rs");

    assert!(
        src.contains("Some(caller) if caller == agent_id => crate::intentions::IntentionSource::SelfDeclared"),
        "`declare_intention` must derive `source` by comparing the caller to the \
         subject, not read it from the input"
    );

    // The declared schema must not offer a `source` property. Checked on the
    // tool definition rather than the handler, because an accepted-and-ignored
    // argument still tells the model the claim is available to make.
    let defs = fermi::agent_backend::tools::platform_tools();
    for name in ["declare_intention", "solicit_agent_plan"] {
        let def = defs
            .iter()
            .find(|d| d.name == name)
            .unwrap_or_else(|| panic!("{name} must be a platform tool"));
        assert!(
            def.input_schema.pointer("/properties/source").is_none(),
            "`{name}` exposes a `source` property; provenance a caller can set \
             is not provenance"
        );
    }
}

/// `solicit_agent_plan` invokes another agent, so the anti-recursion tool set
/// must be able to strip it.
///
/// `with_workspace_no_delegation` exists so a delegated child cannot delegate
/// onward. A new agent-invoking tool that is not marked as one is a hole in
/// that: the child receives it and the recursion guard does not know.
#[test]
fn soliciting_is_marked_as_delegation() {
    let def = fermi::agent_backend::tools::platform_tools()
        .into_iter()
        .find(|d| d.name == "solicit_agent_plan")
        .expect("solicit_agent_plan must be a platform tool");
    assert!(
        def.is_delegation,
        "`solicit_agent_plan` runs another agent and spends against the \
         caller's credentials. Unmarked, it survives into the no-delegation \
         tool set and reopens the recursion `execute_agent` is stripped to close."
    );
    assert!(
        def.requires_workspace,
        "the intention map is workspace state; the tool must require one"
    );
}

// ─── the loop model must not overstate either fix ──────────────────────

/// Loop 3 counts asked-for plans separately from the whole map.
///
/// One combined count is what let the stage read as healthy: the map was full,
/// so coordination looked like it was happening, when one agent had written
/// every row about all the others. Two stages make `plans` ≪ `intentions`
/// visible as the finding it is.
#[test]
fn the_loop_model_distinguishes_asked_for_plans_from_inferred_ones() {
    let loop3 = fermi::loop_model::LOOPS
        .iter()
        .find(|l| l.id == "loop3")
        .expect("loop3 must exist");

    let plans = loop3
        .stages
        .iter()
        .find(|s| s.id == "plans")
        .expect("loop3 must declare a `plans` stage — the step where a member is asked");

    assert!(
        plans.sink_sql.contains("source = 'solicited'"),
        "the `plans` sink counts rows the strategist wrote for itself. It must \
         count only `solicited` rows, or it measures the defect as though it \
         were the fix. Got: {}",
        plans.sink_sql
    );

    let intentions = loop3
        .stages
        .iter()
        .find(|s| s.id == "intentions")
        .expect("the whole-map stage must remain, for the comparison");
    assert_ne!(
        plans.sink_sql, intentions.sink_sql,
        "the two stages measure the same thing, so the gap between asked and \
         assumed is invisible again"
    );
}

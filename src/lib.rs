/// Fermi - Forecasting Programming Language (FPL)
///
/// This library implements the FPL language processing engine,
/// including lexer, parser, semantic analyzer, and execution engine.
pub mod ast;
pub mod attribution;
pub mod distributions;
pub mod evaluator;
pub mod executor;
pub mod lexer;
pub mod parser;
pub mod report;
pub mod semantic;
pub mod sensitivity;
pub mod symbol_table;
pub mod types;

// Agent Backend (Phase 2)
pub mod agent_backend;

// REST API (Phase 2)
pub mod api;

// Gas fees and workflow engine
pub mod gas;
pub mod glasses_shell;
pub mod workflows;

// App primitive — registry + manifest builder
// (handlers live in handlers::apps; builder substrate is here so the CLI,
// the xamanEK app_design session, the fork-from-workspace flow, and the
// auto-seed-from-filesystem path all share the same validation rules.)
pub mod apps;

// Shared slug validation for every publishable artifact (agents, apps,
// workspaces, compositions, …). Re-exports the same rules App slugs
// already use so the platform has a single mental model.
pub mod slug;

// Polymarket integration (prediction market data)
pub mod polymarket;

// Voice synthesis
pub mod voice;

// Outbound transactional email (Resend). No-ops when unconfigured.
pub mod email;

// Schema trust contract (v0.11.0) — the hand-declared manifest of every
// schema object the Rust code assumes exists, plus the boot-time probe
// that verifies it against the live DB.
//
// Declared here, not `#[path]`-included into `api_server.rs`, so that
// `cargo test` can see it. It previously lived in the binary only, which
// is why an unsatisfiable contract survived eight releases unnoticed.
pub mod schema_trust;

// Rollup trust contract — the sibling that asks whether a column is
// telling the truth, not merely whether it exists. `schema_trust` would
// have caught `agents.total_executions` disappearing; it could not catch
// the column being present, correctly typed, and permanently zero because
// nothing ever wrote it. Content drift needs its own contract.
pub mod rollup_trust;

// Grounding trust contract — the third sibling. `schema_trust` asks whether
// a column exists; `rollup_trust` asks whether it is true; this asks
// whether a value could have come from anywhere at all.
//
// `genome_profiler` was asked for genome size, karyotype, divergence date
// and IUCN status with two GBIF tools that return taxonomy only, and filled
// all of it confidently for 56 episodes. Presence and content checks both
// pass on that: the field is there and internally consistent. Only a
// contract that maps output fields to the tools that could supply them
// catches it.
pub mod grounding_trust;

// The one implementation of `ProvenanceOracle`, which carries the grounding
// verdict across the crate boundary into the memory layer.
//
// Semantic rules are extracted from episodes during dream cycles and then
// injected into other agents' prompts — they become things the platform tells
// its own agents are true. Without a floor, a rule extracted from prose is
// stored and retrieved identically to one extracted from tool output, and the
// citation makes it worse than a bare hallucination: `source_episode_cluster`
// genuinely points at episodes that genuinely said that. The laundering runs
// outward, into the whole ecology.
//
// The oracle lives in `fermi` because the field contracts do, and the memory
// crate declares a trait instead of copying the arithmetic — a second copy of
// a trust calculation is a second answer to the same question, and the one
// that disagrees is the one nearest the writer.
pub mod provenance_oracle;

// Verification corpus — the domain layer for community expert determination.
//
// Hodgson et al. 2023 measured phone mushroom-ID apps at 49% best case against
// expert-confirmed specimens. That figure disqualifies an app that tells people
// what is safe to eat, and it is the curriculum for one that teaches how
// unreliable photographic identification is.
//
// An expert determination is also the second independent determiner that
// `grounding_trust`'s cross-check exemption for `forage_identify.taxonomy` calls
// for — recorded there as a capability decision rather than a missing query. It
// was never a structural limit; it was an absence of people.
pub mod verification;

// Image attachments — what may travel with a request, and the rule that an
// undeliverable frame is an error rather than a silent omission.
//
// A camera agent has a failure mode the text-only platform does not: the
// picture goes missing and the answer still arrives. "What is this?" with no
// image attached still gets a species name, correctly labelled
// `model_inference` by a boundary that cannot tell an inference-from-a-photo
// from an inference-from-nothing. Worse than genome_profiler rather than equal
// to it: there the gap was permanent and nameable, here the same field is
// well-sourced or evidence-free depending on whether a blob survived the trip.
pub mod attachments;

// HUD contract — the display-layer sibling of `grounding_trust`, for agents
// whose output is read on glass in half a second rather than parsed.
//
// `grounding_trust` nulls what nothing could supply and stamps
// `<block>_provenance`, which is enough for a consumer that reads the tag. A
// heads-up display is not that consumer: a correctly-tagged guess rendered as
// identical text to a verified retrieval reproduces the `genome_profiler` harm
// through the presentation layer. So this module derives a visible treatment
// per line, conditions every lookup on the provenance of the subject it was
// keyed on, and computes `confidence_display` rather than accepting it.
pub mod hud_contract;

// What an agent quantified, recorded whether or not there is a forecast to
// bind it to.
//
// `forecast_agent_claims.workspace_id` is NOT NULL, which is right for a claim
// — a claim is an adjustment to a driver and is neutralisable at 1.0, which is
// what makes exact Shapley credit cheap. But it was the ONLY home for an
// agent's quantified output, so a standalone evaluation lost it: 14 quantified
// judgements measured, 14 discarded, 0 claims. Standalone evaluation is how
// agents are mostly exercised, so no agent could build a track record.
//
// An assertion is what the agent said; a claim is that assertion bound to a
// driver, 0..n per assertion. The load-bearing rule is that a multiplier can
// never be tool_verified — no database contains "the multiplier for this
// driver" — so a multiplier is not a checkable proposition and verification
// routes to its basis instead.
pub mod assertions;

// Liveness trust contract — the fifth sibling, and the one that would have
// caught the other four. Every contract above examines data that EXISTS; none
// of them can see a table that is empty because nothing ever wrote to it.
//
// That blind spot produced five findings in an afternoon: a CHECK constraint
// declared by seventeen migrations and applied by none; a provenance oracle
// wired into one of three call sites; `forecast_agent_claims` coded, wired,
// exhaustively commented and holding zero rows; `anomaly_events` never fired;
// and `semantic_rules.application_count` declared in migration 010 and never
// incremented. Reading the code proves nothing — in every case the code looks
// right.
//
// Nobody writes this check because `count(*) = 0` is ambiguous. The
// disambiguator is the OPPORTUNITY count: zero claims beside fourteen
// multiplier-bearing episodes is broken; zero beside zero is merely unused.
pub mod liveness_trust;

// Port trust contract — whether the caller is sending what the agent said it
// takes. `negotiate::bind_input` in the console answered this correctly and
// was wired only into the desktop client, so every HTTP execute path went
// unchecked; worse, `stamp_invocation` recorded the CALLER's claim about the
// binding as if it were a finding. Canonical implementation lives here so
// the server can verify rather than transcribe.
pub mod port_trust;

// Card-declared contracts — the typing and field-to-tool rules an agent
// must state about itself before it may be published.
//
// `grounding_trust` holds a Rust const table, which works for curated
// agents someone hand-wrote an entry for and cannot work for anyone else:
// a third party publishing over the API has no way to add a line to a
// compiled const. So the map lives in the card and Rust keeps the checker.
pub mod card_contract;

// A minimal JSON Schema validator. Seven keywords, no new dependency, and
// an unsupported keyword is NOT a pass — a validator that silently ignores
// what it cannot interpret returns `valid` for a document it never checked.
pub mod schema_validate;

// Agent economics — one definition of "how much has this agent run, and
// what did it cost", measured from `episodes` rather than from the
// denormalised counters on `agents` that no code path maintains.
pub mod agent_economics;

// Agent taxonomy (SPEC_30) — the derived ranks. Lives in the lib so both
// the API (classifying agents at creation) and the parity test can reach
// it; `scripts/taxonomy.py` remains the editorial tool for on-disk cards.
pub mod taxonomy;

// Pipeline planning (SPEC_31 P2) — validate a declared workflow_template's
// seams before executing it, so a pipeline that would break at stage 4 never
// spends stages 1–3.
pub mod pipeline;

// Episode construction — one constructor, reachable from both sides. The
// HTTP handlers in the api-server binary and the in-library delegation
// tools (agent_backend::tools_legacy) must build episodes through the same
// function; while it lived in the binary the lib had to keep a duplicate,
// and duplicates silently diverge on cost basis, provider attribution and
// failure provenance.
pub mod calibration;
pub mod episodes;
pub mod intentions;

// Re-export main types
pub use ast::*;
pub use evaluator::{evaluate, EvalError, EvaluationContext};
pub use executor::{ExecutionError, ExecutionResults, Executor};
pub use lexer::{Lexer, LexerError, Token, TokenType};
pub use parser::{ParseError, Parser};
pub use report::generate_report;
pub use semantic::{SemanticAnalysis, SemanticAnalyzer, SemanticError};
pub use symbol_table::{Symbol, SymbolTable, SymbolTableBuilder, SymbolType};
pub use types::{BinaryOp, Type, TypeEnvironment, UnaryOp};

/// Convenience function to execute a program with default settings
pub fn execute_program(program: &Program) -> Result<ExecutionResults, ExecutionError> {
    let mut executor = Executor::new(10000);
    executor.execute(program)
}

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

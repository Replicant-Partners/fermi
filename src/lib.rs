/// Fermi - Forecasting Programming Language (FPL)
///
/// This library implements the FPL language processing engine,
/// including lexer, parser, semantic analyzer, and execution engine.
pub mod ast;
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

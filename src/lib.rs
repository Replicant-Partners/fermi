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

// Voice synthesis
pub mod voice;

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

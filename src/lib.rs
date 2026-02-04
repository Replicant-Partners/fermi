/// Fermi - Forecasting Programming Language (FPL)
///
/// This library implements the FPL language processing engine,
/// including lexer, parser, semantic analyzer, and execution engine.

pub mod lexer;
pub mod ast;
pub mod parser;
pub mod types;
pub mod symbol_table;
pub mod semantic;
pub mod distributions;
pub mod evaluator;
pub mod executor;

// Re-export main types
pub use lexer::{Lexer, Token, TokenType, LexerError};
pub use ast::*;
pub use parser::{Parser, ParseError};
pub use types::{Type, TypeEnvironment, BinaryOp, UnaryOp};
pub use symbol_table::{SymbolTable, SymbolTableBuilder, Symbol, SymbolType};
pub use semantic::{SemanticAnalyzer, SemanticAnalysis, SemanticError};
pub use evaluator::{EvaluationContext, evaluate, EvalError};
pub use executor::{Executor, ExecutionResult, ExecutionError, execute_program, execute_program_with_seed};

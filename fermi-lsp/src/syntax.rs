/// Rowan-based lossless syntax tree for FPL
///
/// This module defines the syntax tree structure using Rowan,
/// enabling incremental parsing and error recovery.

use rowan::{GreenNode, GreenNodeBuilder, Language};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum SyntaxKind {
    // Tokens
    Ident = 0,
    Number,
    String,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,

    // Keywords
    Forecast,
    Driver,
    Estimate,
    Triangular,
    Normal,
    Uniform,
    Lognormal,
    Beta,

    // Operators
    Plus,
    Minus,
    Star,
    Slash,

    // Nodes
    Root,
    ForecastStmt,
    DriverStmt,
    EstimateStmt,
    Expression,
    BinaryExpr,
    CallExpr,

    // Special
    Error,
    Whitespace,
    Comment,
    Eof,
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        Self(kind as u16)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FplLanguage;

impl Language for FplLanguage {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        assert!(raw.0 <= SyntaxKind::Eof as u16);
        unsafe { std::mem::transmute::<u16, SyntaxKind>(raw.0) }
    }

    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        kind.into()
    }
}

pub type SyntaxNode = rowan::SyntaxNode<FplLanguage>;
pub type SyntaxToken = rowan::SyntaxToken<FplLanguage>;
pub type SyntaxElement = rowan::NodeOrToken<SyntaxNode, SyntaxToken>;

/// Build a green tree from tokens
pub fn build_tree(tokens: &[fermi::Token]) -> GreenNode {
    let mut builder = GreenNodeBuilder::new();

    builder.start_node(SyntaxKind::Root.into());

    for token in tokens {
        let kind = match token.token_type {
            fermi::TokenType::Identifier => SyntaxKind::Ident,
            fermi::TokenType::Number => SyntaxKind::Number,
            fermi::TokenType::String => SyntaxKind::String,
            fermi::TokenType::LeftParen => SyntaxKind::LParen,
            fermi::TokenType::RightParen => SyntaxKind::RParen,
            fermi::TokenType::LeftBrace => SyntaxKind::LBrace,
            fermi::TokenType::RightBrace => SyntaxKind::RBrace,
            fermi::TokenType::Comma => SyntaxKind::Comma,
            fermi::TokenType::Plus => SyntaxKind::Plus,
            fermi::TokenType::Minus => SyntaxKind::Minus,
            fermi::TokenType::Star => SyntaxKind::Star,
            fermi::TokenType::Slash => SyntaxKind::Slash,
            fermi::TokenType::Keyword => {
                match token.lexeme.as_str() {
                    "forecast" => SyntaxKind::Forecast,
                    "driver" => SyntaxKind::Driver,
                    "estimate" => SyntaxKind::Estimate,
                    "triangular" => SyntaxKind::Triangular,
                    "normal" => SyntaxKind::Normal,
                    "uniform" => SyntaxKind::Uniform,
                    "lognormal" => SyntaxKind::Lognormal,
                    "beta" => SyntaxKind::Beta,
                    _ => SyntaxKind::Ident,
                }
            }
            fermi::TokenType::Eof => SyntaxKind::Eof,
            _ => SyntaxKind::Error,
        };

        builder.token(kind.into(), &token.lexeme);
    }

    builder.finish_node();
    builder.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_simple_tree() {
        use fermi::Lexer;

        let source = r#"
            forecast "Test" {
                driver x triangular(10, 20, 30)
                estimate x
            }
        "#;

        let lexer = Lexer::new(source);
        let tokens = lexer.tokenize();

        let green = build_tree(&tokens);
        let syntax = SyntaxNode::new_root(green);

        assert!(syntax.children().count() > 0);
    }
}

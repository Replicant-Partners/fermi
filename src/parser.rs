/// FPL Parser
///
/// Recursive descent parser that converts token stream into Abstract Syntax Tree (AST).
/// Uses operator precedence climbing for expressions.
use crate::ast::*;
use crate::lexer::{Token, TokenType};
use std::fmt;

/// Parser error types
#[derive(Debug, Clone)]
pub enum ParseError {
    UnexpectedToken {
        expected: String,
        found: TokenType,
        line: usize,
        column: usize,
    },
    UnexpectedEOF {
        expected: String,
    },
    InvalidExpression {
        message: String,
        line: usize,
        column: usize,
    },
    InvalidDistribution {
        message: String,
        line: usize,
        column: usize,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParseError::UnexpectedToken {
                expected,
                found,
                line,
                column,
            } => {
                write!(
                    f,
                    "Expected {} but found {:?} at {}:{}",
                    expected, found, line, column
                )
            }
            ParseError::UnexpectedEOF { expected } => {
                write!(f, "Unexpected end of file, expected {}", expected)
            }
            ParseError::InvalidExpression {
                message,
                line,
                column,
            } => {
                write!(f, "Invalid expression: {} at {}:{}", message, line, column)
            }
            ParseError::InvalidDistribution {
                message,
                line,
                column,
            } => {
                write!(
                    f,
                    "Invalid distribution: {} at {}:{}",
                    message, line, column
                )
            }
        }
    }
}

pub type ParseResult<T> = Result<T, ParseError>;

/// FPL Parser using recursive descent
pub struct Parser {
    tokens: Vec<Token>,
    current: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, current: 0 }
    }

    /// Parse the entire program
    pub fn parse(mut self) -> ParseResult<Program> {
        let mut statements = Vec::new();

        while !self.is_at_end() {
            statements.push(self.parse_statement()?);
        }

        Ok(Program { statements })
    }

    /// Parse a single statement
    fn parse_statement(&mut self) -> ParseResult<Statement> {
        let token = self.peek();

        match &token.token_type {
            TokenType::Question => Ok(Statement::Question(self.parse_question()?)),
            TokenType::Driver => Ok(Statement::Driver(self.parse_driver()?)),
            TokenType::Evidence => Ok(Statement::Evidence(self.parse_evidence()?)),
            TokenType::Agent => Ok(Statement::Agent(self.parse_agent()?)),
            TokenType::Model => Ok(Statement::Model(self.parse_model()?)),
            TokenType::Simulate => Ok(Statement::Simulate(self.parse_simulate()?)),
            _ => Err(ParseError::UnexpectedToken {
                expected: "statement keyword (question, driver, evidence, agent, model, simulate)"
                    .to_string(),
                found: token.token_type.clone(),
                line: token.line,
                column: token.column,
            }),
        }
    }

    /// Parse question statement
    fn parse_question(&mut self) -> ParseResult<QuestionStmt> {
        self.consume_keyword(TokenType::Question, "question")?;

        let text = self.consume_string()?;

        // Optional target date and resolution criteria can be added later
        Ok(QuestionStmt {
            text,
            target_date: None,
            resolution_criteria: None,
        })
    }

    /// Parse driver statement
    fn parse_driver(&mut self) -> ParseResult<DriverStmt> {
        self.consume_keyword(TokenType::Driver, "driver")?;

        let name = self.consume_identifier()?;

        // Parse driver type
        let driver_type = if self.match_token(&TokenType::Continuous) {
            DriverType::Continuous
        } else if self.match_token(&TokenType::Binary) {
            DriverType::Binary
        } else if self.match_token(&TokenType::Discrete) {
            DriverType::Discrete
        } else {
            return Err(ParseError::UnexpectedToken {
                expected: "driver type (continuous, binary, or discrete)".to_string(),
                found: self.peek().token_type.clone(),
                line: self.peek().line,
                column: self.peek().column,
            });
        };

        // Parse driver body
        self.consume_token(TokenType::LBrace, "{")?;

        let mut display_name = None;
        let mut description = None;
        let mut distribution = None;
        let mut probability = None;
        let mut impact_multiplier = None;
        let mut values = None;
        let mut weights = None;
        let mut unit = None;
        let mut rationale = None;
        let constraints = Vec::new();
        let evidence_refs = Vec::new();

        while !self.check(&TokenType::RBrace) && !self.is_at_end() {
            let field = self.consume_identifier()?;
            self.consume_token(TokenType::Colon, ":")?;

            match field.as_str() {
                "display_name" => {
                    display_name = Some(self.consume_string()?);
                }
                "description" => {
                    description = Some(self.consume_string()?);
                }
                "distribution" => {
                    distribution = Some(self.parse_distribution()?);
                }
                "probability" => {
                    probability = Some(self.parse_probability_value()?);
                }
                "impact_multiplier" => {
                    impact_multiplier = Some(self.parse_number()?);
                }
                "values" => {
                    values = Some(self.parse_number_array()?);
                }
                "weights" => {
                    weights = Some(self.parse_number_array()?);
                }
                "unit" => {
                    unit = Some(self.consume_string()?);
                }
                "rationale" => {
                    rationale = Some(self.consume_string()?);
                }
                _ => {
                    // Skip unknown fields for now
                    self.skip_until_newline_or_rbrace();
                }
            }
        }

        self.consume_token(TokenType::RBrace, "}")?;

        Ok(DriverStmt {
            name,
            display_name,
            description,
            driver_type,
            distribution,
            probability,
            impact_multiplier,
            values,
            weights,
            unit,
            rationale,
            constraints,
            evidence_refs,
        })
    }

    /// Parse distribution
    fn parse_distribution(&mut self) -> ParseResult<Distribution> {
        let dist_type = self.peek();

        match &dist_type.token_type {
            TokenType::Triangular => {
                self.advance();
                self.consume_token(TokenType::LParen, "(")?;

                let p5 = self.parse_expression()?;
                self.consume_token(TokenType::Comma, ",")?;

                let p50 = self.parse_expression()?;
                self.consume_token(TokenType::Comma, ",")?;

                let p95 = self.parse_expression()?;
                self.consume_token(TokenType::RParen, ")")?;

                Ok(Distribution::Triangular { p5, p50, p95 })
            }
            TokenType::Normal => {
                self.advance();
                self.consume_token(TokenType::LParen, "(")?;

                let mean = self.parse_expression()?;
                self.consume_token(TokenType::Comma, ",")?;

                let stddev = self.parse_expression()?;
                self.consume_token(TokenType::RParen, ")")?;

                Ok(Distribution::Normal { mean, stddev })
            }
            TokenType::Lognormal => {
                self.advance();
                self.consume_token(TokenType::LParen, "(")?;

                let median = self.parse_expression()?;
                self.consume_token(TokenType::Comma, ",")?;

                let sigma = self.parse_expression()?;
                self.consume_token(TokenType::RParen, ")")?;

                Ok(Distribution::Lognormal { median, sigma })
            }
            TokenType::Uniform => {
                self.advance();
                self.consume_token(TokenType::LParen, "(")?;

                let low = self.parse_expression()?;
                self.consume_token(TokenType::Comma, ",")?;

                let high = self.parse_expression()?;
                self.consume_token(TokenType::RParen, ")")?;

                Ok(Distribution::Uniform { low, high })
            }
            TokenType::Beta => {
                self.advance();
                self.consume_token(TokenType::LParen, "(")?;

                let alpha = self.parse_expression()?;
                self.consume_token(TokenType::Comma, ",")?;

                let beta = self.parse_expression()?;

                // Optional min/max
                let min = if self.match_token(&TokenType::Comma) {
                    Some(self.parse_expression()?)
                } else {
                    None
                };

                let max = if min.is_some() && self.match_token(&TokenType::Comma) {
                    Some(self.parse_expression()?)
                } else {
                    None
                };

                self.consume_token(TokenType::RParen, ")")?;

                Ok(Distribution::Beta {
                    alpha,
                    beta,
                    min,
                    max,
                })
            }
            _ => Err(ParseError::InvalidDistribution {
                message: format!(
                    "Expected distribution type, found {:?}",
                    dist_type.token_type
                ),
                line: dist_type.line,
                column: dist_type.column,
            }),
        }
    }

    /// Parse evidence statement
    fn parse_evidence(&mut self) -> ParseResult<EvidenceStmt> {
        self.consume_keyword(TokenType::Evidence, "evidence")?;

        let id = self.consume_identifier()?;

        self.consume_token(TokenType::LBrace, "{")?;

        let mut source = String::new();
        let mut summary = None;
        let mut url = None;
        let mut relevance = None;
        let mut date = None;
        let key_findings = Vec::new();

        while !self.check(&TokenType::RBrace) && !self.is_at_end() {
            let field = self.consume_identifier()?;
            self.consume_token(TokenType::Colon, ":")?;

            match field.as_str() {
                "source" => {
                    source = self.consume_string()?;
                }
                "summary" => {
                    summary = Some(self.consume_string()?);
                }
                "url" => {
                    url = Some(self.consume_string()?);
                }
                "relevance" => {
                    relevance = Some(self.parse_probability_value()?);
                }
                "date" => {
                    date = Some(self.consume_date()?);
                }
                _ => {
                    self.skip_until_newline_or_rbrace();
                }
            }
        }

        self.consume_token(TokenType::RBrace, "}")?;

        Ok(EvidenceStmt {
            id,
            source,
            summary,
            url,
            relevance,
            date,
            key_findings,
        })
    }

    /// Parse agent statement
    fn parse_agent(&mut self) -> ParseResult<AgentStmt> {
        self.consume_keyword(TokenType::Agent, "agent")?;

        let name = self.consume_identifier()?;

        self.consume_token(TokenType::LBrace, "{")?;

        let mut agent_type = None;
        let mut query = String::new();
        let mut schedule = None;
        let driver_refs = Vec::new();

        while !self.check(&TokenType::RBrace) && !self.is_at_end() {
            let field = self.consume_identifier()?;
            self.consume_token(TokenType::Colon, ":")?;

            match field.as_str() {
                "type" => {
                    agent_type = Some(self.consume_string()?);
                }
                "query" => {
                    query = self.consume_string()?;
                }
                "schedule" => {
                    schedule = Some(self.parse_schedule()?);
                }
                _ => {
                    self.skip_until_newline_or_rbrace();
                }
            }
        }

        self.consume_token(TokenType::RBrace, "}")?;

        Ok(AgentStmt {
            name,
            agent_type,
            query,
            schedule,
            driver_refs,
        })
    }

    /// Parse schedule
    fn parse_schedule(&mut self) -> ParseResult<Schedule> {
        if self.match_token(&TokenType::Every) {
            let interval = self.parse_number()? as u32;
            let unit_str = self.consume_identifier()?;

            let unit = match unit_str.as_str() {
                "minute" | "minutes" => TimeUnit::Minute,
                "hour" | "hours" => TimeUnit::Hour,
                "day" | "days" => TimeUnit::Day,
                "week" | "weeks" => TimeUnit::Week,
                "month" | "months" => TimeUnit::Month,
                _ => {
                    return Err(ParseError::InvalidExpression {
                        message: format!("Invalid time unit: {}", unit_str),
                        line: self.peek().line,
                        column: self.peek().column,
                    })
                }
            };

            Ok(Schedule::Every { interval, unit })
        } else {
            Ok(Schedule::Once)
        }
    }

    /// Parse model statement
    fn parse_model(&mut self) -> ParseResult<ModelStmt> {
        self.consume_keyword(TokenType::Model, "model")?;
        self.consume_token(TokenType::Colon, ":")?;

        let expression = self.parse_expression()?;

        Ok(ModelStmt { expression })
    }

    /// Parse simulate statement
    fn parse_simulate(&mut self) -> ParseResult<SimulateStmt> {
        self.consume_keyword(TokenType::Simulate, "simulate")?;

        let iterations = self.parse_number()? as u32;

        // Optional "iterations" keyword
        if let TokenType::Identifier(id) = &self.peek().token_type {
            if id == "iterations" {
                self.advance();
            }
        }

        Ok(SimulateStmt {
            iterations,
            target: None,
        })
    }

    /// Parse expression with operator precedence
    fn parse_expression(&mut self) -> ParseResult<Expression> {
        self.parse_conditional()
    }

    /// Parse conditional (if-then-else)
    fn parse_conditional(&mut self) -> ParseResult<Expression> {
        if self.match_token(&TokenType::If) {
            let condition = self.parse_logical_or()?;
            self.consume_keyword(TokenType::Then, "then")?;
            let then_expr = self.parse_logical_or()?;
            self.consume_keyword(TokenType::Else, "else")?;
            let else_expr = self.parse_logical_or()?;

            Ok(Expression::If {
                condition: Box::new(condition),
                then_expr: Box::new(then_expr),
                else_expr: Box::new(else_expr),
            })
        } else {
            self.parse_logical_or()
        }
    }

    /// Parse logical OR
    fn parse_logical_or(&mut self) -> ParseResult<Expression> {
        let mut left = self.parse_logical_and()?;

        while self.match_token(&TokenType::Or) {
            let right = self.parse_logical_and()?;
            left = Expression::Or(Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    /// Parse logical AND
    fn parse_logical_and(&mut self) -> ParseResult<Expression> {
        let mut left = self.parse_equality()?;

        while self.match_token(&TokenType::And) {
            let right = self.parse_equality()?;
            left = Expression::And(Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    /// Parse equality (==, !=)
    fn parse_equality(&mut self) -> ParseResult<Expression> {
        let mut left = self.parse_comparison()?;

        loop {
            if self.match_token(&TokenType::DoubleEquals) {
                let right = self.parse_comparison()?;
                left = Expression::Equal(Box::new(left), Box::new(right));
            } else if self.match_token(&TokenType::NotEquals) {
                let right = self.parse_comparison()?;
                left = Expression::NotEqual(Box::new(left), Box::new(right));
            } else {
                break;
            }
        }

        Ok(left)
    }

    /// Parse comparison (>, <, >=, <=)
    fn parse_comparison(&mut self) -> ParseResult<Expression> {
        let mut left = self.parse_addition()?;

        loop {
            if self.match_token(&TokenType::Greater) {
                let right = self.parse_addition()?;
                left = Expression::Greater(Box::new(left), Box::new(right));
            } else if self.match_token(&TokenType::Less) {
                let right = self.parse_addition()?;
                left = Expression::Less(Box::new(left), Box::new(right));
            } else if self.match_token(&TokenType::GreaterEqual) {
                let right = self.parse_addition()?;
                left = Expression::GreaterEqual(Box::new(left), Box::new(right));
            } else if self.match_token(&TokenType::LessEqual) {
                let right = self.parse_addition()?;
                left = Expression::LessEqual(Box::new(left), Box::new(right));
            } else {
                break;
            }
        }

        Ok(left)
    }

    /// Parse addition and subtraction
    fn parse_addition(&mut self) -> ParseResult<Expression> {
        let mut left = self.parse_multiplication()?;

        loop {
            if self.match_token(&TokenType::Plus) {
                let right = self.parse_multiplication()?;
                left = Expression::Add(Box::new(left), Box::new(right));
            } else if self.match_token(&TokenType::Minus) {
                let right = self.parse_multiplication()?;
                left = Expression::Subtract(Box::new(left), Box::new(right));
            } else {
                break;
            }
        }

        Ok(left)
    }

    /// Parse multiplication, division, modulo
    fn parse_multiplication(&mut self) -> ParseResult<Expression> {
        let mut left = self.parse_power()?;

        loop {
            if self.match_token(&TokenType::Star) {
                let right = self.parse_power()?;
                left = Expression::Multiply(Box::new(left), Box::new(right));
            } else if self.match_token(&TokenType::Slash) {
                let right = self.parse_power()?;
                left = Expression::Divide(Box::new(left), Box::new(right));
            } else if self.match_token(&TokenType::Percent) {
                let right = self.parse_power()?;
                left = Expression::Modulo(Box::new(left), Box::new(right));
            } else {
                break;
            }
        }

        Ok(left)
    }

    /// Parse power (^)
    fn parse_power(&mut self) -> ParseResult<Expression> {
        let mut left = self.parse_unary()?;

        if self.match_token(&TokenType::Caret) {
            let right = self.parse_power()?; // Right associative
            left = Expression::Power(Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    /// Parse unary (-, not)
    fn parse_unary(&mut self) -> ParseResult<Expression> {
        if self.match_token(&TokenType::Minus) {
            let expr = self.parse_unary()?;
            Ok(Expression::Subtract(
                Box::new(Expression::Number(0.0)),
                Box::new(expr),
            ))
        } else if self.match_token(&TokenType::Not) {
            let expr = self.parse_unary()?;
            Ok(Expression::Not(Box::new(expr)))
        } else {
            self.parse_primary()
        }
    }

    /// Parse primary expressions (literals, identifiers, function calls, parentheses)
    fn parse_primary(&mut self) -> ParseResult<Expression> {
        let token = self.peek();

        match &token.token_type {
            TokenType::Number(n) => {
                let val = *n;
                self.advance();
                Ok(Expression::Number(val))
            }
            TokenType::Probability(p) => {
                let val = *p;
                self.advance();
                Ok(Expression::Probability(val))
            }
            TokenType::String(s) => {
                let val = s.clone();
                self.advance();
                Ok(Expression::String(val))
            }
            TokenType::Boolean(b) => {
                let val = *b;
                self.advance();
                Ok(Expression::Boolean(val))
            }
            TokenType::Identifier(id) => {
                let name = id.clone();
                self.advance();

                // Check for function call
                if self.check(&TokenType::LParen) {
                    self.parse_function_call(name)
                } else {
                    Ok(Expression::Identifier(name))
                }
            }
            TokenType::LParen => {
                self.advance();
                let expr = self.parse_expression()?;
                self.consume_token(TokenType::RParen, ")")?;
                Ok(expr)
            }
            _ => Err(ParseError::InvalidExpression {
                message: format!("Unexpected token: {:?}", token.token_type),
                line: token.line,
                column: token.column,
            }),
        }
    }

    /// Parse function call
    fn parse_function_call(&mut self, name: String) -> ParseResult<Expression> {
        self.consume_token(TokenType::LParen, "(")?;

        let mut args = Vec::new();

        if !self.check(&TokenType::RParen) {
            loop {
                args.push(self.parse_expression()?);

                if !self.match_token(&TokenType::Comma) {
                    break;
                }
            }
        }

        self.consume_token(TokenType::RParen, ")")?;

        Ok(Expression::FunctionCall { name, args })
    }

    // Helper methods

    fn is_at_end(&self) -> bool {
        matches!(self.peek().token_type, TokenType::EOF)
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.current]
    }

    fn advance(&mut self) -> &Token {
        if !self.is_at_end() {
            self.current += 1;
        }
        &self.tokens[self.current - 1]
    }

    fn check(&self, token_type: &TokenType) -> bool {
        if self.is_at_end() {
            return false;
        }
        std::mem::discriminant(&self.peek().token_type) == std::mem::discriminant(token_type)
    }

    fn match_token(&mut self, token_type: &TokenType) -> bool {
        if self.check(token_type) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn consume_token(&mut self, token_type: TokenType, expected: &str) -> ParseResult<()> {
        if self.check(&token_type) {
            self.advance();
            Ok(())
        } else {
            Err(ParseError::UnexpectedToken {
                expected: expected.to_string(),
                found: self.peek().token_type.clone(),
                line: self.peek().line,
                column: self.peek().column,
            })
        }
    }

    fn consume_keyword(&mut self, token_type: TokenType, keyword: &str) -> ParseResult<()> {
        self.consume_token(token_type, keyword)
    }

    fn consume_identifier(&mut self) -> ParseResult<String> {
        if let TokenType::Identifier(id) = &self.peek().token_type {
            let name = id.clone();
            self.advance();
            Ok(name)
        } else {
            Err(ParseError::UnexpectedToken {
                expected: "identifier".to_string(),
                found: self.peek().token_type.clone(),
                line: self.peek().line,
                column: self.peek().column,
            })
        }
    }

    fn consume_string(&mut self) -> ParseResult<String> {
        if let TokenType::String(s) = &self.peek().token_type {
            let text = s.clone();
            self.advance();
            Ok(text)
        } else {
            Err(ParseError::UnexpectedToken {
                expected: "string".to_string(),
                found: self.peek().token_type.clone(),
                line: self.peek().line,
                column: self.peek().column,
            })
        }
    }

    fn consume_date(&mut self) -> ParseResult<String> {
        if let TokenType::Date(d) = &self.peek().token_type {
            let date = d.clone();
            self.advance();
            Ok(date)
        } else {
            Err(ParseError::UnexpectedToken {
                expected: "date".to_string(),
                found: self.peek().token_type.clone(),
                line: self.peek().line,
                column: self.peek().column,
            })
        }
    }

    fn parse_number(&mut self) -> ParseResult<f64> {
        if let TokenType::Number(n) = self.peek().token_type {
            self.advance();
            Ok(n)
        } else {
            Err(ParseError::UnexpectedToken {
                expected: "number".to_string(),
                found: self.peek().token_type.clone(),
                line: self.peek().line,
                column: self.peek().column,
            })
        }
    }

    fn parse_probability_value(&mut self) -> ParseResult<f64> {
        if let TokenType::Probability(p) = self.peek().token_type {
            self.advance();
            Ok(p)
        } else if let TokenType::Number(n) = self.peek().token_type {
            self.advance();
            Ok(n)
        } else {
            Err(ParseError::UnexpectedToken {
                expected: "probability".to_string(),
                found: self.peek().token_type.clone(),
                line: self.peek().line,
                column: self.peek().column,
            })
        }
    }

    fn parse_number_array(&mut self) -> ParseResult<Vec<f64>> {
        self.consume_token(TokenType::LBracket, "[")?;

        let mut numbers = Vec::new();

        while !self.check(&TokenType::RBracket) && !self.is_at_end() {
            numbers.push(self.parse_number()?);

            if !self.check(&TokenType::RBracket) {
                self.consume_token(TokenType::Comma, ",")?;
            }
        }

        self.consume_token(TokenType::RBracket, "]")?;
        Ok(numbers)
    }

    fn skip_until_newline_or_rbrace(&mut self) {
        while !self.is_at_end()
            && !matches!(
                self.peek().token_type,
                TokenType::Newline | TokenType::RBrace
            )
        {
            self.advance();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    fn parse_source(source: &str) -> ParseResult<Program> {
        let lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();
        let parser = Parser::new(tokens);
        parser.parse()
    }

    #[test]
    fn test_parse_question() {
        let source = r#"question "Will AMD reach $200?""#;
        let program = parse_source(source).unwrap();

        assert_eq!(program.statements.len(), 1);
        if let Statement::Question(q) = &program.statements[0] {
            assert_eq!(q.text, "Will AMD reach $200?");
        } else {
            panic!("Expected Question statement");
        }
    }

    #[test]
    fn test_parse_continuous_driver() {
        let source = r#"
driver market_size continuous {
    distribution: triangular(500, 1200, 2500)
    unit: "millions USD"
}
"#;
        let program = parse_source(source).unwrap();

        assert_eq!(program.statements.len(), 1);
        if let Statement::Driver(d) = &program.statements[0] {
            assert_eq!(d.name, "market_size");
            assert!(matches!(d.driver_type, DriverType::Continuous));
            assert!(d.distribution.is_some());
            assert_eq!(d.unit, Some("millions USD".to_string()));
        } else {
            panic!("Expected Driver statement");
        }
    }

    #[test]
    fn test_parse_binary_driver() {
        let source = r#"
driver major_contract binary {
    probability: 0.65p
    impact_multiplier: 1.3
}
"#;
        let program = parse_source(source).unwrap();

        assert_eq!(program.statements.len(), 1);
        if let Statement::Driver(d) = &program.statements[0] {
            assert_eq!(d.name, "major_contract");
            assert!(matches!(d.driver_type, DriverType::Binary));
            assert_eq!(d.probability, Some(0.65));
            assert_eq!(d.impact_multiplier, Some(1.3));
        } else {
            panic!("Expected Driver statement");
        }
    }

    #[test]
    fn test_parse_model() {
        let source = "model: market_size * growth_rate";
        let program = parse_source(source).unwrap();

        assert_eq!(program.statements.len(), 1);
        if let Statement::Model(m) = &program.statements[0] {
            assert!(matches!(m.expression, Expression::Multiply(_, _)));
        } else {
            panic!("Expected Model statement");
        }
    }

    #[test]
    fn test_parse_expression() {
        let source = "model: (a + b) * c / (d - e)";
        let program = parse_source(source).unwrap();

        assert_eq!(program.statements.len(), 1);
    }

    #[test]
    fn test_parse_if_expression() {
        let source = "model: if major_contract then 1.5 else 1.0";
        let program = parse_source(source).unwrap();

        assert_eq!(program.statements.len(), 1);
        if let Statement::Model(m) = &program.statements[0] {
            if let Expression::If { .. } = m.expression {
                // Success
            } else {
                panic!("Expected If expression");
            }
        }
    }

    #[test]
    fn test_parse_simulate() {
        let source = "simulate 10000 iterations";
        let program = parse_source(source).unwrap();

        assert_eq!(program.statements.len(), 1);
        if let Statement::Simulate(s) = &program.statements[0] {
            assert_eq!(s.iterations, 10000);
        } else {
            panic!("Expected Simulate statement");
        }
    }

    #[test]
    fn test_parse_complete_forecast() {
        let source = r#"
question "Will AMD reach $200?"

driver market_size continuous {
    distribution: triangular(500, 1200, 2500)
    unit: "millions USD"
}

driver growth_rate continuous {
    distribution: normal(0.25, 0.05)
}

model: market_size * (1 + growth_rate)

simulate 10000 iterations
"#;

        let program = parse_source(source).unwrap();
        assert_eq!(program.statements.len(), 5);
    }
}

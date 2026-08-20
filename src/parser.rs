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
    MissingField {
        field: String,
        context: String,
        line: usize,
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
            ParseError::MissingField {
                field,
                context,
                line,
            } => {
                write!(
                    f,
                    "Missing required field '{}' in {} at line {}",
                    field, context, line
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
            TokenType::Factor => Ok(Statement::Factor(self.parse_factor()?)),
            TokenType::Param => Ok(Statement::Param(self.parse_param()?)),
            TokenType::Import => Ok(Statement::Import(self.parse_import()?)),
            TokenType::Estimate => Ok(Statement::Estimate(self.parse_estimate()?)),
            TokenType::Output => Ok(Statement::Output(self.parse_output()?)),
            _ => Err(ParseError::UnexpectedToken {
                expected: "statement keyword (question, driver, evidence, agent, model, simulate, factor, param, import, estimate, output)"
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

        // Check if there's a block with base_rate (optional)
        let mut base_rate = None;
        let mut target_date = None;
        let mut resolution_criteria = None;

        if self.match_token(&TokenType::LBrace) {
            // Parse question block with optional fields
            while !self.check(&TokenType::RBrace) && !self.is_at_end() {
                // Check for base_rate keyword
                if self.match_token(&TokenType::BaseRate) {
                    base_rate = Some(self.parse_base_rate()?);
                } else if let TokenType::Identifier(field) = &self.peek().token_type.clone() {
                    // Handle other optional fields
                    let field_name = field.clone();
                    self.advance();
                    self.consume_token(TokenType::Colon, ":")?;

                    match field_name.as_str() {
                        "target_date" => {
                            target_date = Some(self.consume_string()?);
                        }
                        "resolution_criteria" => {
                            resolution_criteria = Some(self.consume_string()?);
                        }
                        _ => {
                            // Skip unknown fields
                            self.skip_until_newline_or_rbrace();
                        }
                    }
                } else {
                    // Skip unexpected tokens
                    self.advance();
                }
            }

            self.consume_token(TokenType::RBrace, "}")?;
        }

        Ok(QuestionStmt {
            text,
            base_rate,
            target_date,
            resolution_criteria,
        })
    }

    /// Parse base_rate block
    fn parse_base_rate(&mut self) -> ParseResult<BaseRate> {
        use crate::ast::{BaseRate, GeneratedBy};

        self.consume_token(TokenType::LBrace, "{")?;

        let mut reference_class = None;
        let mut historical_frequency = None;
        let mut sample_size = None;
        let mut source = None;
        let mut reasoning = None;
        let mut generated_by = None;

        while !self.check(&TokenType::RBrace) && !self.is_at_end() {
            // Match field tokens (can be keywords or identifiers)
            let field_token = self.peek().token_type.clone();

            let field_name: String = match &field_token {
                TokenType::ReferenceClass => {
                    self.advance();
                    "reference_class".to_string()
                }
                TokenType::HistoricalFrequency => {
                    self.advance();
                    "historical_frequency".to_string()
                }
                TokenType::SampleSize => {
                    self.advance();
                    "sample_size".to_string()
                }
                TokenType::Source => {
                    self.advance();
                    "source".to_string()
                }
                TokenType::Reasoning => {
                    self.advance();
                    "reasoning".to_string()
                }
                TokenType::GeneratedBy => {
                    self.advance();
                    "generated_by".to_string()
                }
                TokenType::Identifier(id) => {
                    let name = id.clone();
                    self.advance();
                    name
                }
                _ => {
                    return Err(ParseError::UnexpectedToken {
                        expected: "field name".to_string(),
                        found: field_token,
                        line: self.peek().line,
                        column: self.peek().column,
                    });
                }
            };

            self.consume_token(TokenType::Colon, ":")?;

            match field_name.as_str() {
                "reference_class" => {
                    reference_class = Some(self.consume_string()?);
                }
                "historical_frequency" => {
                    historical_frequency = Some(self.parse_probability_value()?);
                }
                "sample_size" => {
                    let size = self.parse_number()? as usize;
                    sample_size = Some(size);
                }
                "source" => {
                    source = Some(self.consume_string()?);
                }
                "reasoning" => {
                    reasoning = Some(self.consume_string()?);
                }
                "generated_by" => {
                    // Parse agent name or "human"
                    if self.match_token(&TokenType::Human) {
                        generated_by = Some(GeneratedBy::Human);
                    } else if self.match_token(&TokenType::Agent) {
                        // Expect agent name (identifier or string)
                        generated_by = Some(GeneratedBy::Agent("agent".to_string()));
                    } else if let TokenType::Identifier(agent_name) =
                        &self.peek().token_type.clone()
                    {
                        generated_by = Some(GeneratedBy::Agent(agent_name.clone()));
                        self.advance();
                    } else {
                        return Err(ParseError::UnexpectedToken {
                            expected: "'human' or agent name".to_string(),
                            found: self.peek().token_type.clone(),
                            line: self.peek().line,
                            column: self.peek().column,
                        });
                    }
                }
                _ => {
                    // Skip unknown fields
                    self.skip_until_newline_or_rbrace();
                }
            }
        }

        self.consume_token(TokenType::RBrace, "}")?;

        // Validate required fields
        let reference_class = reference_class.ok_or_else(|| ParseError::MissingField {
            field: "reference_class".to_string(),
            context: "base_rate".to_string(),
            line: self.peek().line,
        })?;

        let historical_frequency =
            historical_frequency.ok_or_else(|| ParseError::MissingField {
                field: "historical_frequency".to_string(),
                context: "base_rate".to_string(),
                line: self.peek().line,
            })?;

        let source = source.ok_or_else(|| ParseError::MissingField {
            field: "source".to_string(),
            context: "base_rate".to_string(),
            line: self.peek().line,
        })?;

        let generated_by = generated_by.ok_or_else(|| ParseError::MissingField {
            field: "generated_by".to_string(),
            context: "base_rate".to_string(),
            line: self.peek().line,
        })?;

        Ok(BaseRate {
            reference_class,
            historical_frequency,
            sample_size,
            source,
            reasoning,
            generated_by,
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
        let mut applies_to: Option<crate::ast::AppliesTo> = None;
        let mut rationale = None;
        let mut constraints: Vec<crate::ast::Constraint> = Vec::new();
        let mut evidence_refs = Vec::new();
        let mut learnable = false;
        let mut feeds_from: Option<crate::ast::FeedsFrom> = None;

        while !self.check(&TokenType::RBrace) && !self.is_at_end() {
            // Use consume_identifier_or_keyword so reserved tokens like
            // `learnable` can be used as field names without re-lexing.
            let field = self.consume_identifier_or_keyword()?;
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
                "evidence_refs" => {
                    evidence_refs = self.parse_string_array()?;
                }
                "constraint" => {
                    // A boolean expression the driver's value must satisfy, e.g.
                    // `constraint: synoptic_pattern >= 0.1`. Repeatable.
                    //
                    // `DriverStmt.constraints` has been in the AST since the
                    // language was written and `parse_driver` held
                    // `let constraints = Vec::new();` — not even `mut` — so the
                    // field was never populated, and nothing read it either. It was
                    // dead at both ends.
                    let condition = self.parse_expression()?;
                    constraints.push(crate::ast::Constraint {
                        condition,
                        message: None,
                    });
                }
                "applies_to" => {
                    // What a ratio-valued driver multiplies: `probability` or
                    // `quantity`. A bare word rather than a string, matching
                    // `generated_by` and the driver-type keywords, so a typo is a
                    // parse error rather than an unrecognised string that silently
                    // means nothing.
                    let word = self.consume_identifier_or_keyword()?;
                    applies_to = Some(match word.as_str() {
                        "probability" => crate::ast::AppliesTo::Probability,
                        "quantity" => crate::ast::AppliesTo::Quantity,
                        other => {
                            return Err(ParseError::UnexpectedToken {
                                expected: "applies_to value (probability or quantity)".to_string(),
                                found: TokenType::Identifier(other.to_string()),
                                line: self.peek().line,
                                column: self.peek().column,
                            })
                        }
                    });
                }
                "learnable" => {
                    // Accept `learnable: true` / `learnable: false`.
                    // This opts the driver into BayesOps-managed distribution
                    // fitting (see DriverStmt docs).
                    learnable = self.parse_bool_literal()?;
                }
                "feeds_from" => {
                    // Parse the feeds_from block: declares how upstream
                    // workspace resolutions translate into observations for
                    // fitting this driver. See ast::FeedsFrom and
                    // docs/specs/23_BAYESOPS_WORLD_CUP_DEMO.md §3.4.
                    feeds_from = Some(self.parse_feeds_from()?);
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
            applies_to,
            rationale,
            constraints,
            evidence_refs,
            learnable,
            feeds_from,
        })
    }

    /// Parse a `true` or `false` boolean literal.
    /// Used by driver fields (`learnable: true`) and any other place that
    /// accepts a boolean. The lexer currently emits booleans as
    /// `Identifier("true")` / `Identifier("false")` rather than dedicated
    /// tokens, so we accept either form.
    fn parse_bool_literal(&mut self) -> ParseResult<bool> {
        let token = self.peek().clone();
        match &token.token_type {
            TokenType::Boolean(b) => {
                self.advance();
                Ok(*b)
            }
            TokenType::Identifier(s) if s == "true" => {
                self.advance();
                Ok(true)
            }
            TokenType::Identifier(s) if s == "false" => {
                self.advance();
                Ok(false)
            }
            _ => Err(ParseError::UnexpectedToken {
                expected: "boolean (true/false)".to_string(),
                found: token.token_type.clone(),
                line: token.line,
                column: token.column,
            }),
        }
    }

    /// Parse a `feeds_from: { ... }` block on a learnable driver.
    /// See `ast::FeedsFrom` and docs/specs/23_BAYESOPS_WORLD_CUP_DEMO.md §3.4.
    ///
    /// Required fields: `source`, `extractor`, `config`.
    /// Optional field: `auto_accept_threshold_pp`.
    fn parse_feeds_from(&mut self) -> ParseResult<crate::ast::FeedsFrom> {
        self.consume_token(TokenType::LBrace, "{")?;

        let mut source: Option<String> = None;
        let mut extractor: Option<String> = None;
        let mut config: Option<serde_json::Value> = None;
        let mut auto_accept_threshold_pp: Option<f64> = None;

        while !self.check(&TokenType::RBrace) && !self.is_at_end() {
            let field = self.consume_identifier_or_keyword()?;
            self.consume_token(TokenType::Colon, ":")?;

            match field.as_str() {
                "source" => source = Some(self.consume_string()?),
                "extractor" => extractor = Some(self.consume_string()?),
                "config" => config = Some(self.parse_json_value()?),
                "auto_accept_threshold_pp" => {
                    auto_accept_threshold_pp = Some(self.parse_number()?);
                }
                _ => {
                    self.skip_until_newline_or_rbrace();
                }
            }

            // Allow but don't require trailing commas between fields.
            self.match_token(&TokenType::Comma);
        }

        self.consume_token(TokenType::RBrace, "}")?;

        let source = source.ok_or_else(|| ParseError::UnexpectedToken {
            expected: "feeds_from requires 'source' field".to_string(),
            found: self.peek().token_type.clone(),
            line: self.peek().line,
            column: self.peek().column,
        })?;
        let extractor = extractor.ok_or_else(|| ParseError::UnexpectedToken {
            expected: "feeds_from requires 'extractor' field".to_string(),
            found: self.peek().token_type.clone(),
            line: self.peek().line,
            column: self.peek().column,
        })?;
        let config = config.unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

        Ok(crate::ast::FeedsFrom {
            source,
            extractor,
            config,
            auto_accept_threshold_pp,
        })
    }

    /// Parse a JSON-ish value out of the FPL token stream. Supports:
    ///   - object literals  `{ key: value, ... }` (keys are identifiers or strings)
    ///   - array literals   `[ a, b, c ]`
    ///   - string literals  `"text"`
    ///   - numbers          `42`, `3.14`
    ///   - booleans         `true`, `false`
    ///   - null             `null` (as Identifier)
    ///
    /// Used by `feeds_from.config` and reusable for any future field that
    /// needs structured embedded JSON. Returns a `serde_json::Value` so the
    /// downstream consumer (the Extractor) can use existing serde tooling.
    fn parse_json_value(&mut self) -> ParseResult<serde_json::Value> {
        let token = self.peek().clone();
        match &token.token_type {
            TokenType::LBrace => {
                self.advance();
                let mut map = serde_json::Map::new();
                while !self.check(&TokenType::RBrace) && !self.is_at_end() {
                    // Key: identifier-or-keyword OR string literal
                    let key = if let TokenType::String(s) = &self.peek().token_type.clone() {
                        let key = s.clone();
                        self.advance();
                        key
                    } else {
                        self.consume_identifier_or_keyword()?
                    };
                    self.consume_token(TokenType::Colon, ":")?;
                    let value = self.parse_json_value()?;
                    map.insert(key, value);
                    self.match_token(&TokenType::Comma);
                }
                self.consume_token(TokenType::RBrace, "}")?;
                Ok(serde_json::Value::Object(map))
            }
            TokenType::LBracket => {
                self.advance();
                let mut items = Vec::new();
                while !self.check(&TokenType::RBracket) && !self.is_at_end() {
                    items.push(self.parse_json_value()?);
                    self.match_token(&TokenType::Comma);
                }
                self.consume_token(TokenType::RBracket, "]")?;
                Ok(serde_json::Value::Array(items))
            }
            TokenType::String(s) => {
                let v = serde_json::Value::String(s.clone());
                self.advance();
                Ok(v)
            }
            TokenType::Number(n) => {
                let v = serde_json::json!(n);
                self.advance();
                Ok(v)
            }
            TokenType::Boolean(b) => {
                let v = serde_json::Value::Bool(*b);
                self.advance();
                Ok(v)
            }
            TokenType::Identifier(s) if s == "true" => {
                self.advance();
                Ok(serde_json::Value::Bool(true))
            }
            TokenType::Identifier(s) if s == "false" => {
                self.advance();
                Ok(serde_json::Value::Bool(false))
            }
            TokenType::Identifier(s) if s == "null" => {
                self.advance();
                Ok(serde_json::Value::Null)
            }
            _ => Err(ParseError::UnexpectedToken {
                expected: "JSON value (object, array, string, number, bool, null)".to_string(),
                found: token.token_type.clone(),
                line: token.line,
                column: token.column,
            }),
        }
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
        let mut strength = None;
        let mut key_findings = Vec::new();

        while !self.check(&TokenType::RBrace) && !self.is_at_end() {
            // Match field tokens (can be keywords or identifiers)
            let field_token = self.peek().token_type.clone();

            let field: String = match &field_token {
                TokenType::Source => {
                    self.advance();
                    "source".to_string()
                }
                TokenType::Reasoning => {
                    self.advance();
                    "reasoning".to_string()
                }
                TokenType::Identifier(id) => {
                    let name = id.clone();
                    self.advance();
                    name
                }
                _ => {
                    return Err(ParseError::UnexpectedToken {
                        expected: "field name".to_string(),
                        found: field_token,
                        line: self.peek().line,
                        column: self.peek().column,
                    });
                }
            };

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
                    // Accept either a Date token or a String token
                    if let TokenType::Date(_) = &self.peek().token_type {
                        date = Some(self.consume_date()?);
                    } else {
                        date = Some(self.consume_string()?);
                    }
                }
                "strength" => {
                    strength = Some(self.parse_probability_value()?);
                }
                "key_findings" => {
                    key_findings = self.parse_string_array()?;
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
            strength,
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
        let mut executor = None;
        let mut schedule = None;
        let mut driver_refs = Vec::new();
        let mut depends_on = Vec::new();
        let mut confidence_threshold = None;

        while !self.check(&TokenType::RBrace) && !self.is_at_end() {
            // Match field tokens (can be keywords or identifiers)
            let field_token = self.peek().token_type.clone();

            let field: String = match &field_token {
                TokenType::Schedule => {
                    self.advance();
                    "schedule".to_string()
                }
                TokenType::Identifier(id) => {
                    let name = id.clone();
                    self.advance();
                    name
                }
                _ => {
                    return Err(ParseError::UnexpectedToken {
                        expected: "field name".to_string(),
                        found: field_token,
                        line: self.peek().line,
                        column: self.peek().column,
                    });
                }
            };

            self.consume_token(TokenType::Colon, ":")?;

            match field.as_str() {
                "type" => {
                    agent_type = Some(self.consume_string()?);
                }
                "query" => {
                    query = self.consume_string()?;
                }
                "executor" => {
                    executor = Some(self.parse_executor_type()?);
                }
                "schedule" => {
                    schedule = Some(self.parse_schedule()?);
                }
                "driver_refs" => {
                    driver_refs = self.parse_string_array()?;
                }
                "depends_on" => {
                    depends_on = self.parse_string_array()?;
                }
                "confidence_threshold" => {
                    let value = self.parse_probability_value()?;
                    // Validate range [0.0, 1.0]
                    if value < 0.0 || value > 1.0 {
                        return Err(ParseError::InvalidExpression {
                            message: format!(
                                "confidence_threshold must be between 0.0 and 1.0, got {}",
                                value
                            ),
                            line: self.peek().line,
                            column: self.peek().column,
                        });
                    }
                    confidence_threshold = Some(value);
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
            executor,
            schedule,
            driver_refs,
            depends_on,
            confidence_threshold,
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

    /// Parse executor type
    fn parse_executor_type(&mut self) -> ParseResult<ExecutorType> {
        let executor_str = self.consume_string()?;

        match executor_str.as_str() {
            "llm" => Ok(ExecutorType::LLM),
            "mcp" => Ok(ExecutorType::MCP),
            "manual" => Ok(ExecutorType::Manual),
            "skill" => Ok(ExecutorType::Skill),
            _ => Err(ParseError::InvalidExpression {
                message: format!(
                    "Invalid executor type: '{}'. Valid values: llm, mcp, manual, skill",
                    executor_str
                ),
                line: self.peek().line,
                column: self.peek().column,
            }),
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

                // Check for param.field reference (e.g., param.elo_current)
                if name == "param" && self.check(&TokenType::Colon) {
                    // Handle param:field_name (colon syntax)
                    self.advance();
                    let field = self.consume_identifier()?;
                    return Ok(Expression::ParamRef(field));
                }

                // Check for function call
                if self.check(&TokenType::LParen) {
                    self.parse_function_call(name)
                } else {
                    Ok(Expression::Identifier(name))
                }
            }
            // Factor model expression keywords
            TokenType::Learnable => {
                self.advance();
                self.consume_token(TokenType::LParen, "(")?;
                let initial = match self.parse_expression()? {
                    Expression::Number(n) => n,
                    _ => 1.0,
                };
                self.consume_token(TokenType::Comma, ",")?;
                let sigma = match self.parse_expression()? {
                    Expression::Number(n) => n,
                    _ => 0.1,
                };
                self.consume_token(TokenType::RParen, ")")?;
                // name is assigned later by Executor::assign_learnable_names
                Ok(Expression::LearnablePrior {
                    initial,
                    sigma,
                    name: None,
                })
            }
            TokenType::Residual => {
                self.advance();
                self.consume_token(TokenType::LParen, "(")?;
                let raw = self.parse_expression()?;
                let mut upstream = Vec::new();
                while self.check(&TokenType::Comma) {
                    self.advance();
                    let factor_name = self.consume_identifier()?;
                    upstream.push(factor_name);
                }
                self.consume_token(TokenType::RParen, ")")?;
                Ok(Expression::Residual {
                    raw: Box::new(raw),
                    upstream,
                })
            }
            TokenType::Exp => {
                self.advance();
                self.consume_token(TokenType::LParen, "(")?;
                let inner = self.parse_expression()?;
                self.consume_token(TokenType::RParen, ")")?;
                Ok(Expression::Exp(Box::new(inner)))
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

    fn parse_string_array(&mut self) -> ParseResult<Vec<String>> {
        self.consume_token(TokenType::LBracket, "[")?;

        let mut strings = Vec::new();

        while !self.check(&TokenType::RBracket) && !self.is_at_end() {
            strings.push(self.consume_string()?);

            if !self.check(&TokenType::RBracket) {
                self.consume_token(TokenType::Comma, ",")?;
            }
        }

        self.consume_token(TokenType::RBracket, "]")?;
        Ok(strings)
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

    // ═══════════════════════════════════════════════════════════════
    // Factor Model Parsing
    // ═══════════════════════════════════════════════════════════════

    /// Parse: factor X1 "Socioeconomic Capital" { inputs: ..., formulation: ..., variance_share: 0.25, update: static }
    fn parse_factor(&mut self) -> ParseResult<FactorStmt> {
        self.consume_keyword(TokenType::Factor, "factor")?;
        let name = self.consume_identifier()?;
        let label = if self.check_string() {
            self.consume_string()?
        } else {
            name.clone()
        };
        self.skip_newlines();

        let mut inputs = Vec::new();
        let mut formulation = None;
        let mut variance_share = 0.0;
        let mut update_frequency = UpdateFreq::Static;

        if self.check(&TokenType::LBrace) {
            self.advance(); // consume {
            self.skip_newlines();

            while !self.check(&TokenType::RBrace) && !self.is_at_end() {
                self.skip_newlines();
                if self.check(&TokenType::RBrace) {
                    break;
                }

                let field = self.consume_identifier_or_keyword()?;
                self.consume_token(TokenType::Colon, ":")?;

                match field.as_str() {
                    "inputs" => {
                        // inputs: name1, name2, name3
                        loop {
                            let input_name = self.consume_identifier()?;
                            inputs.push(FactorInput {
                                name: input_name,
                                input_type: ParamType::Real,
                            });
                            if self.check(&TokenType::Comma) {
                                self.advance();
                            } else {
                                break;
                            }
                        }
                    }
                    "formulation" => {
                        formulation = Some(self.parse_expression()?);
                    }
                    "variance_share" => {
                        let expr = self.parse_expression()?;
                        variance_share = match expr {
                            Expression::Number(n) => n,
                            Expression::Probability(p) => p,
                            _ => 0.0,
                        };
                    }
                    "update" => {
                        let val = self.consume_identifier_or_keyword()?;
                        update_frequency = match val.as_str() {
                            "static" => UpdateFreq::Static,
                            "per_match" => UpdateFreq::PerMatch,
                            "tournament_start" => UpdateFreq::TournamentStart,
                            "per_fixture" => UpdateFreq::PerFixture,
                            _ => UpdateFreq::Static,
                        };
                    }
                    _ => {
                        // Skip unknown fields
                        let _ = self.parse_expression();
                    }
                }
                self.skip_newlines();
            }
            self.consume_token(TokenType::RBrace, "}")?;
        }

        Ok(FactorStmt {
            name,
            label,
            inputs,
            formulation,
            variance_share,
            update_frequency,
        })
    }

    /// Parse: param team_id: string
    fn parse_param(&mut self) -> ParseResult<ParamDecl> {
        self.consume_keyword(TokenType::Param, "param")?;
        let name = self.consume_identifier()?;
        self.consume_token(TokenType::Colon, ":")?;
        let type_name = self.consume_identifier_or_keyword()?;
        let param_type = match type_name.as_str() {
            "real" | "float" | "f64" => ParamType::Real,
            "int" | "integer" | "i64" => ParamType::Int,
            "string" | "str" | "text" => ParamType::Str,
            "bool" | "boolean" => ParamType::Bool,
            _ => ParamType::Str,
        };

        // Optional default value
        let default_value = if self.check(&TokenType::Equals) {
            self.advance();
            Some(self.parse_expression()?)
        } else {
            None
        };

        Ok(ParamDecl {
            name,
            param_type,
            default_value,
        })
    }

    /// Parse: import factor X1 with ( input1 = expr, input2 = expr )
    fn parse_import(&mut self) -> ParseResult<ImportStmt> {
        self.consume_keyword(TokenType::Import, "import")?;
        // Optional "factor" keyword
        if self.check(&TokenType::Factor) {
            self.advance();
        }
        let factor_name = self.consume_identifier()?;

        let mut bindings = Vec::new();
        // "with" keyword + parenthesized bindings
        if self.check_identifier("with") {
            self.advance();
            self.consume_token(TokenType::LParen, "(")?;
            self.skip_newlines();

            while !self.check(&TokenType::RParen) && !self.is_at_end() {
                self.skip_newlines();
                if self.check(&TokenType::RParen) {
                    break;
                }
                let input_name = self.consume_identifier()?;
                self.consume_token(TokenType::Equals, "=")?;
                let value = self.parse_expression()?;
                bindings.push((input_name, value));
                if self.check(&TokenType::Comma) {
                    self.advance();
                }
                self.skip_newlines();
            }
            self.consume_token(TokenType::RParen, ")")?;
        }

        Ok(ImportStmt {
            factor_name,
            bindings,
        })
    }

    /// Parse: estimate tournament_strength as: expression
    fn parse_estimate(&mut self) -> ParseResult<EstimateStmt> {
        self.consume_keyword(TokenType::Estimate, "estimate")?;
        let name = self.consume_identifier()?;
        // Optional "as" keyword
        if self.check_identifier("as") {
            self.advance();
        }
        if self.check(&TokenType::Colon) {
            self.advance();
        }
        let expression = self.parse_expression()?;

        Ok(EstimateStmt { name, expression })
    }

    /// Parse: output p_win: expression  OR  output p_win: derived
    fn parse_output(&mut self) -> ParseResult<OutputStmt> {
        self.consume_keyword(TokenType::Output, "output")?;
        let name = self.consume_identifier()?;

        let mut expression = None;
        let mut is_derived = false;

        if self.check(&TokenType::Colon) {
            self.advance();
            if self.check_identifier("derived") {
                self.advance();
                is_derived = true;
            } else {
                expression = Some(self.parse_expression()?);
            }
        }

        Ok(OutputStmt {
            name,
            expression,
            is_derived,
        })
    }

    /// Skip over newline tokens.
    fn skip_newlines(&mut self) {
        while self.check(&TokenType::Newline) && !self.is_at_end() {
            self.advance();
        }
    }

    /// Check if the current token is an identifier with a specific value
    fn check_identifier(&self, expected: &str) -> bool {
        matches!(&self.peek().token_type, TokenType::Identifier(s) if s == expected)
    }

    /// Check if current token is a string literal
    fn check_string(&self) -> bool {
        matches!(&self.peek().token_type, TokenType::String(_))
    }

    /// Consume an identifier or keyword token — returns the text.
    /// Keywords are valid as field names inside blocks.
    fn consume_identifier_or_keyword(&mut self) -> ParseResult<String> {
        let token = self.peek().clone();
        match &token.token_type {
            TokenType::Identifier(s) => {
                let s = s.clone();
                self.advance();
                Ok(s)
            }
            // Allow keywords as identifiers in field position
            TokenType::Inputs => {
                self.advance();
                Ok("inputs".to_string())
            }
            TokenType::Formulation => {
                self.advance();
                Ok("formulation".to_string())
            }
            TokenType::VarianceShare => {
                self.advance();
                Ok("variance_share".to_string())
            }
            TokenType::Update => {
                self.advance();
                Ok("update".to_string())
            }
            TokenType::Static => {
                self.advance();
                Ok("static".to_string())
            }
            TokenType::PerMatch => {
                self.advance();
                Ok("per_match".to_string())
            }
            TokenType::PerFixture => {
                self.advance();
                Ok("per_fixture".to_string())
            }
            TokenType::Output => {
                self.advance();
                Ok("output".to_string())
            }
            TokenType::Source => {
                self.advance();
                Ok("source".to_string())
            }
            TokenType::Factor => {
                self.advance();
                Ok("factor".to_string())
            }
            TokenType::Learnable => {
                self.advance();
                Ok("learnable".to_string())
            }
            _ => Err(ParseError::UnexpectedToken {
                expected: "identifier".to_string(),
                found: token.token_type.clone(),
                line: token.line,
                column: token.column,
            }),
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
    fn test_parse_learnable_driver_with_feeds_from() {
        // Realistic World Cup team-prior driver: opts into BayesOps, declares
        // how upstream H2H resolutions translate into observations.
        let source = r#"
driver won_in_group_stage continuous {
    distribution: triangular(0.3, 0.5, 0.7)
    learnable: true
    feeds_from: {
        source: "upstream_resolutions",
        extractor: "binary_winner_id_match",
        config: {
            winner_field: "winner_team_id",
            match_value: "${workspace.entity_id}"
        },
        auto_accept_threshold_pp: 3.0
    }
}
"#;
        let program = parse_source(source).unwrap();
        assert_eq!(program.statements.len(), 1);
        if let Statement::Driver(d) = &program.statements[0] {
            assert_eq!(d.name, "won_in_group_stage");
            assert!(d.learnable);
            let ff = d.feeds_from.as_ref().expect("feeds_from should be Some");
            assert_eq!(ff.source, "upstream_resolutions");
            assert_eq!(ff.extractor, "binary_winner_id_match");
            assert_eq!(ff.auto_accept_threshold_pp, Some(3.0));
            // Config is a serde_json::Value object; spot-check keys
            assert_eq!(
                ff.config.get("winner_field").and_then(|v| v.as_str()),
                Some("winner_team_id")
            );
            assert_eq!(
                ff.config.get("match_value").and_then(|v| v.as_str()),
                Some("${workspace.entity_id}")
            );
        } else {
            panic!("Expected Driver statement");
        }
    }

    #[test]
    fn test_parse_feeds_from_without_optional_threshold() {
        let source = r#"
driver won continuous {
    distribution: triangular(0.0, 0.5, 1.0)
    learnable: true
    feeds_from: {
        source: "upstream_resolutions",
        extractor: "binary_field_value",
        config: { path: "advanced", value: true }
    }
}
"#;
        let program = parse_source(source).unwrap();
        if let Statement::Driver(d) = &program.statements[0] {
            let ff = d.feeds_from.as_ref().unwrap();
            assert_eq!(ff.auto_accept_threshold_pp, None);
            assert_eq!(ff.config.get("value").and_then(|v| v.as_bool()), Some(true));
        } else {
            panic!("Expected Driver statement");
        }
    }

    #[test]
    fn test_parse_driver_without_feeds_from_is_fine() {
        // Backward compatibility: existing learnable drivers without
        // feeds_from must still parse — the refit hook will treat them as
        // fittable only from explicit observations arrays.
        let source = r#"
driver legacy continuous {
    distribution: normal(5.0, 1.0)
    learnable: true
}
"#;
        let program = parse_source(source).unwrap();
        if let Statement::Driver(d) = &program.statements[0] {
            assert!(d.learnable);
            assert!(d.feeds_from.is_none());
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

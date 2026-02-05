/// Semantic Analyzer
///
/// Performs semantic analysis on the AST:
/// - Type checking
/// - Symbol resolution
/// - Validation rules
/// - Constraint checking
use crate::ast::*;
use crate::symbol_table::*;
use crate::types::*;
use std::fmt;

/// Semantic error types
#[derive(Debug, Clone)]
pub enum SemanticError {
    UndefinedSymbol {
        name: String,
        message: String,
    },
    TypeMismatch {
        expected: Type,
        found: Type,
        message: String,
    },
    ValidationError {
        rule: String,
        message: String,
    },
    DuplicateDefinition {
        name: String,
        message: String,
    },
}

impl fmt::Display for SemanticError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SemanticError::UndefinedSymbol { name, message } => {
                write!(f, "Undefined symbol '{}': {}", name, message)
            }
            SemanticError::TypeMismatch {
                expected,
                found,
                message,
            } => {
                write!(
                    f,
                    "Type mismatch: expected {}, found {}. {}",
                    expected, found, message
                )
            }
            SemanticError::ValidationError { rule, message } => {
                write!(f, "Validation error ({}): {}", rule, message)
            }
            SemanticError::DuplicateDefinition { name, message } => {
                write!(f, "Duplicate definition of '{}': {}", name, message)
            }
        }
    }
}

/// Semantic analysis result
pub struct SemanticAnalysis {
    pub symbol_table: SymbolTable,
    pub errors: Vec<SemanticError>,
    pub warnings: Vec<String>,
}

impl SemanticAnalysis {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Semantic analyzer
pub struct SemanticAnalyzer {
    symbol_table: SymbolTable,
    type_env: TypeEnvironment,
    errors: Vec<SemanticError>,
    warnings: Vec<String>,
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        SemanticAnalyzer {
            symbol_table: SymbolTable::new(),
            type_env: TypeEnvironment::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Analyze a program
    pub fn analyze(mut self, program: &Program) -> SemanticAnalysis {
        // Phase 1: Build symbol table
        match SymbolTableBuilder::new().build(program) {
            Ok(table) => {
                self.symbol_table = table;
            }
            Err(errors) => {
                for error in errors {
                    self.errors.push(SemanticError::DuplicateDefinition {
                        name: "unknown".to_string(),
                        message: error,
                    });
                }
                // Return early if symbol table construction failed
                return SemanticAnalysis {
                    symbol_table: self.symbol_table,
                    errors: self.errors,
                    warnings: self.warnings,
                };
            }
        }

        // Phase 2: Type checking and validation
        for stmt in &program.statements {
            self.analyze_statement(stmt);
        }

        // Phase 3: Check validation rules
        self.check_validation_rules(program);

        SemanticAnalysis {
            symbol_table: self.symbol_table,
            errors: self.errors,
            warnings: self.warnings,
        }
    }

    /// Analyze a statement
    fn analyze_statement(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Driver(driver) => self.analyze_driver(driver),
            Statement::Model(model) => self.analyze_model(model),
            Statement::Simulate(simulate) => self.analyze_simulate(simulate),
            Statement::Evidence(_) => {
                // Evidence is just metadata, no type checking needed
            }
            Statement::Agent(_) => {
                // Agents are validated separately
            }
            Statement::Question(_) => {
                // Question is just metadata
            }
        }
    }

    /// Analyze a driver
    fn analyze_driver(&mut self, driver: &DriverStmt) {
        match driver.driver_type {
            DriverType::Continuous => {
                // Must have distribution
                if driver.distribution.is_none() {
                    self.errors.push(SemanticError::ValidationError {
                        rule: "continuous_driver_requires_distribution".to_string(),
                        message: format!(
                            "Continuous driver '{}' must have a distribution",
                            driver.name
                        ),
                    });
                } else if let Some(dist) = &driver.distribution {
                    self.analyze_distribution(dist, &driver.name);
                }
            }
            DriverType::Binary => {
                // Must have probability
                if driver.probability.is_none() {
                    self.errors.push(SemanticError::ValidationError {
                        rule: "binary_driver_requires_probability".to_string(),
                        message: format!("Binary driver '{}' must have a probability", driver.name),
                    });
                } else if let Some(prob) = driver.probability {
                    // Check probability range
                    if !(0.0..=1.0).contains(&prob) {
                        self.errors.push(SemanticError::ValidationError {
                            rule: "probability_range".to_string(),
                            message: format!(
                                "Probability must be between 0 and 1, got {} for driver '{}'",
                                prob, driver.name
                            ),
                        });
                    }
                }
            }
            DriverType::Discrete => {
                // Must have values and weights
                match (&driver.values, &driver.weights) {
                    (None, _) => {
                        self.errors.push(SemanticError::ValidationError {
                            rule: "discrete_driver_requires_values".to_string(),
                            message: format!("Discrete driver '{}' must have values", driver.name),
                        });
                    }
                    (_, None) => {
                        self.errors.push(SemanticError::ValidationError {
                            rule: "discrete_driver_requires_weights".to_string(),
                            message: format!("Discrete driver '{}' must have weights", driver.name),
                        });
                    }
                    (Some(values), Some(weights)) => {
                        // Check that values and weights have same length
                        if values.len() != weights.len() {
                            self.errors.push(SemanticError::ValidationError {
                                rule: "discrete_values_weights_mismatch".to_string(),
                                message: format!(
                                    "Discrete driver '{}' has {} values but {} weights",
                                    driver.name,
                                    values.len(),
                                    weights.len()
                                ),
                            });
                        }

                        // Check that weights sum to approximately 1.0
                        let sum: f64 = weights.iter().sum();
                        if (sum - 1.0).abs() > 0.001 {
                            self.warnings.push(format!(
                                "Discrete driver '{}' weights sum to {:.3}, should sum to 1.0",
                                driver.name, sum
                            ));
                        }

                        // Check that all weights are non-negative
                        for (i, &weight) in weights.iter().enumerate() {
                            if weight < 0.0 {
                                self.errors.push(SemanticError::ValidationError {
                                    rule: "discrete_negative_weight".to_string(),
                                    message: format!(
                                        "Discrete driver '{}' has negative weight at index {}: {}",
                                        driver.name, i, weight
                                    ),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    /// Analyze a distribution
    fn analyze_distribution(&mut self, dist: &Distribution, driver_name: &str) {
        match dist {
            Distribution::Triangular { p5, p50, p95 } => {
                // Type check parameters
                let t5 = self.infer_type(p5);
                let t50 = self.infer_type(p50);
                let t95 = self.infer_type(p95);

                if !t5.is_numeric() || !t50.is_numeric() || !t95.is_numeric() {
                    self.errors.push(SemanticError::TypeMismatch {
                        expected: Type::Number,
                        found: Type::Error,
                        message: format!(
                            "Triangular distribution parameters for '{}' must be numeric",
                            driver_name
                        ),
                    });
                }

                // Validate ordering (if all are constant numbers)
                if let (Expression::Number(v5), Expression::Number(v50), Expression::Number(v95)) =
                    (p5, p50, p95)
                {
                    if !(*v5 <= *v50 && *v50 <= *v95) {
                        self.errors.push(SemanticError::ValidationError {
                            rule: "triangular_ordering".to_string(),
                            message: format!(
                                "Triangular distribution for '{}' must have p5 <= p50 <= p95, got {} <= {} <= {}",
                                driver_name, v5, v50, v95
                            ),
                        });
                    }

                    // Check if range is too narrow (potential overconfidence)
                    let range = (v95 - v5) / v50;
                    if range < 0.2 {
                        self.warnings.push(format!(
                            "Driver '{}' has a narrow range (±{}%). Consider if this reflects true uncertainty.",
                            driver_name, (range * 100.0) as i32
                        ));
                    }
                }
            }
            Distribution::Normal { mean, stddev } => {
                let t_mean = self.infer_type(mean);
                let t_std = self.infer_type(stddev);

                if !t_mean.is_numeric() || !t_std.is_numeric() {
                    self.errors.push(SemanticError::TypeMismatch {
                        expected: Type::Number,
                        found: Type::Error,
                        message: format!(
                            "Normal distribution parameters for '{}' must be numeric",
                            driver_name
                        ),
                    });
                }

                // Check stddev is positive
                if let Expression::Number(std) = stddev {
                    if *std <= 0.0 {
                        self.errors.push(SemanticError::ValidationError {
                            rule: "positive_stddev".to_string(),
                            message: format!(
                                "Standard deviation for '{}' must be positive, got {}",
                                driver_name, std
                            ),
                        });
                    }
                }
            }
            Distribution::Lognormal { median, sigma } => {
                let t_med = self.infer_type(median);
                let t_sig = self.infer_type(sigma);

                if !t_med.is_numeric() || !t_sig.is_numeric() {
                    self.errors.push(SemanticError::TypeMismatch {
                        expected: Type::Number,
                        found: Type::Error,
                        message: format!(
                            "Lognormal distribution parameters for '{}' must be numeric",
                            driver_name
                        ),
                    });
                }

                // Check sigma is positive
                if let Expression::Number(sig) = sigma {
                    if *sig <= 0.0 {
                        self.errors.push(SemanticError::ValidationError {
                            rule: "positive_sigma".to_string(),
                            message: format!(
                                "Sigma for '{}' must be positive, got {}",
                                driver_name, sig
                            ),
                        });
                    }
                }
            }
            Distribution::Uniform { low, high } => {
                let t_low = self.infer_type(low);
                let t_high = self.infer_type(high);

                if !t_low.is_numeric() || !t_high.is_numeric() {
                    self.errors.push(SemanticError::TypeMismatch {
                        expected: Type::Number,
                        found: Type::Error,
                        message: format!(
                            "Uniform distribution parameters for '{}' must be numeric",
                            driver_name
                        ),
                    });
                }

                // Check ordering
                if let (Expression::Number(l), Expression::Number(h)) = (low, high) {
                    if l >= h {
                        self.errors.push(SemanticError::ValidationError {
                            rule: "uniform_ordering".to_string(),
                            message: format!(
                                "Uniform distribution for '{}' must have low < high, got {} < {}",
                                driver_name, l, h
                            ),
                        });
                    }
                }
            }
            Distribution::Beta { alpha, beta, .. } => {
                let t_alpha = self.infer_type(alpha);
                let t_beta = self.infer_type(beta);

                if !t_alpha.is_numeric() || !t_beta.is_numeric() {
                    self.errors.push(SemanticError::TypeMismatch {
                        expected: Type::Number,
                        found: Type::Error,
                        message: format!(
                            "Beta distribution parameters for '{}' must be numeric",
                            driver_name
                        ),
                    });
                }

                // Check both are positive
                if let Expression::Number(a) = alpha {
                    if *a <= 0.0 {
                        self.errors.push(SemanticError::ValidationError {
                            rule: "positive_alpha".to_string(),
                            message: format!(
                                "Alpha for '{}' must be positive, got {}",
                                driver_name, a
                            ),
                        });
                    }
                }
                if let Expression::Number(b) = beta {
                    if *b <= 0.0 {
                        self.errors.push(SemanticError::ValidationError {
                            rule: "positive_beta".to_string(),
                            message: format!(
                                "Beta for '{}' must be positive, got {}",
                                driver_name, b
                            ),
                        });
                    }
                }
            }
        }
    }

    /// Analyze a model
    fn analyze_model(&mut self, model: &ModelStmt) {
        let ty = self.infer_type(&model.expression);

        if !ty.is_numeric() && ty != Type::Boolean {
            self.errors.push(SemanticError::TypeMismatch {
                expected: Type::Number,
                found: ty,
                message: "Model expression must evaluate to a number or boolean".to_string(),
            });
        }
    }

    /// Analyze a simulate statement
    fn analyze_simulate(&mut self, simulate: &SimulateStmt) {
        if simulate.iterations == 0 {
            self.errors.push(SemanticError::ValidationError {
                rule: "positive_iterations".to_string(),
                message: "Simulation must have at least 1 iteration".to_string(),
            });
        }

        if simulate.iterations < 1000 {
            self.warnings.push(format!(
                "Simulation has only {} iterations. Consider using at least 10,000 for stable results.",
                simulate.iterations
            ));
        }
    }

    /// Infer the type of an expression
    fn infer_type(&mut self, expr: &Expression) -> Type {
        match expr {
            Expression::Number(_) => Type::Number,
            Expression::Probability(_) => Type::Probability,
            Expression::String(_) => Type::String,
            Expression::Boolean(_) => Type::Boolean,
            Expression::Identifier(name) => {
                if let Some(symbol) = self.symbol_table.lookup(name) {
                    symbol.ty.clone()
                } else {
                    self.errors.push(SemanticError::UndefinedSymbol {
                        name: name.clone(),
                        message: format!("Identifier '{}' is not defined", name),
                    });
                    Type::Error
                }
            }
            Expression::Add(l, r) => self.infer_binary_type(l, r, BinaryOp::Add),
            Expression::Subtract(l, r) => self.infer_binary_type(l, r, BinaryOp::Subtract),
            Expression::Multiply(l, r) => self.infer_binary_type(l, r, BinaryOp::Multiply),
            Expression::Divide(l, r) => self.infer_binary_type(l, r, BinaryOp::Divide),
            Expression::Modulo(l, r) => self.infer_binary_type(l, r, BinaryOp::Modulo),
            Expression::Power(l, r) => self.infer_binary_type(l, r, BinaryOp::Power),
            Expression::Greater(l, r) => self.infer_binary_type(l, r, BinaryOp::Greater),
            Expression::Less(l, r) => self.infer_binary_type(l, r, BinaryOp::Less),
            Expression::GreaterEqual(l, r) => self.infer_binary_type(l, r, BinaryOp::GreaterEqual),
            Expression::LessEqual(l, r) => self.infer_binary_type(l, r, BinaryOp::LessEqual),
            Expression::Equal(l, r) => self.infer_binary_type(l, r, BinaryOp::Equal),
            Expression::NotEqual(l, r) => self.infer_binary_type(l, r, BinaryOp::NotEqual),
            Expression::And(l, r) => self.infer_binary_type(l, r, BinaryOp::And),
            Expression::Or(l, r) => self.infer_binary_type(l, r, BinaryOp::Or),
            Expression::Not(e) => {
                let t = self.infer_type(e);
                Type::unary_op_result(&t, UnaryOp::Not)
            }
            Expression::If {
                condition,
                then_expr,
                else_expr,
            } => {
                let cond_ty = self.infer_type(condition);
                if !cond_ty.is_boolean() && cond_ty != Type::Error {
                    self.errors.push(SemanticError::TypeMismatch {
                        expected: Type::Boolean,
                        found: cond_ty,
                        message: "If condition must be boolean".to_string(),
                    });
                }

                let then_ty = self.infer_type(then_expr);
                let else_ty = self.infer_type(else_expr);

                // Both branches should have compatible types
                if !then_ty.can_coerce_to(&else_ty) && !else_ty.can_coerce_to(&then_ty) {
                    self.warnings.push(format!(
                        "If-then-else branches have different types: {} and {}",
                        then_ty, else_ty
                    ));
                }

                then_ty
            }
            Expression::FunctionCall { .. } => {
                // Function calls in distributions are handled in distribution validation
                Type::Number
            }
        }
    }

    /// Infer type of binary operation
    fn infer_binary_type(&mut self, left: &Expression, right: &Expression, op: BinaryOp) -> Type {
        let left_ty = self.infer_type(left);
        let right_ty = self.infer_type(right);

        let result_ty = Type::binary_op_result(&left_ty, &right_ty, op);

        if result_ty == Type::Error {
            self.errors.push(SemanticError::TypeMismatch {
                expected: Type::Number,
                found: Type::Error,
                message: format!(
                    "Cannot apply operator {:?} to types {} and {}",
                    op, left_ty, right_ty
                ),
            });
        }

        result_ty
    }

    /// Check validation rules
    fn check_validation_rules(&mut self, program: &Program) {
        // Rule: All drivers should be used in model
        if !self.symbol_table.all_drivers_used() {
            let unused = self.symbol_table.unused_drivers();
            for driver in unused {
                self.warnings.push(format!(
                    "Driver '{}' is defined but not used in the model",
                    driver.name
                ));
            }
        }

        // Rule: Should have at least one driver
        if self.symbol_table.drivers().is_empty() {
            self.errors.push(SemanticError::ValidationError {
                rule: "minimum_drivers".to_string(),
                message: "Forecast should have at least one driver".to_string(),
            });
        }

        // Rule: Should have a model if there are drivers
        if !self.symbol_table.drivers().is_empty() {
            let has_model = program
                .statements
                .iter()
                .any(|s| matches!(s, Statement::Model(_)));
            if !has_model {
                self.errors.push(SemanticError::ValidationError {
                    rule: "model_required".to_string(),
                    message: "Forecast with drivers must have a model".to_string(),
                });
            }
        }

        // Rule: Should have a question
        let has_question = program
            .statements
            .iter()
            .any(|s| matches!(s, Statement::Question(_)));
        if !has_question {
            self.warnings
                .push("Forecast should have a question statement".to_string());
        }

        // Rule: Recommend having evidence
        if self.symbol_table.evidence().is_empty() {
            self.warnings.push(
                "Consider adding evidence to support your forecast. Use 'evidence' statements or research agents.".to_string()
            );
        }
    }
}

impl Default for SemanticAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn analyze_source(source: &str) -> SemanticAnalysis {
        let tokens = Lexer::new(source).tokenize().unwrap();
        let program = Parser::new(tokens).parse().unwrap();
        SemanticAnalyzer::new().analyze(&program)
    }

    #[test]
    fn test_valid_forecast() {
        let source = r#"
question "Will X happen?"
driver market_size continuous {
    distribution: triangular(500, 1200, 2500)
}
model: market_size
simulate 10000 iterations
"#;

        let analysis = analyze_source(source);
        assert!(analysis.is_valid(), "Errors: {:?}", analysis.errors);
    }

    #[test]
    fn test_triangular_ordering_error() {
        let source = r#"
driver market_size continuous {
    distribution: triangular(2500, 1200, 500)
}
"#;

        let analysis = analyze_source(source);
        assert!(!analysis.is_valid());
        assert!(analysis.errors.iter().any(|e| matches!(e, SemanticError::ValidationError { rule, .. } if rule == "triangular_ordering")));
    }

    #[test]
    fn test_undefined_variable() {
        let source = r#"
driver market_size continuous {
    distribution: triangular(500, 1200, 2500)
}
model: unknown_variable
"#;

        let analysis = analyze_source(source);
        assert!(!analysis.is_valid());
        assert!(analysis
            .errors
            .iter()
            .any(|e| matches!(e, SemanticError::UndefinedSymbol { .. })));
    }

    #[test]
    fn test_unused_driver_warning() {
        let source = r#"
driver market_size continuous {
    distribution: triangular(500, 1200, 2500)
}
driver unused_driver continuous {
    distribution: normal(100, 20)
}
model: market_size
"#;

        let analysis = analyze_source(source);
        assert!(analysis.is_valid());
        assert!(!analysis.warnings.is_empty());
    }
}

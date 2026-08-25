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

        // Phase 3: Check agent circular dependencies (needs all agents)
        self.check_agent_circular_dependencies(program);

        // Phase 4: Check validation rules
        self.check_validation_rules(program);
        self.check_driver_spaces(program);
        self.check_driver_constraints(program);

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
            Statement::Agent(agent) => {
                self.analyze_agent(agent);
            }
            Statement::Question(question) => {
                self.analyze_question(question);
            }
            // Factor model statements — validated at a higher level
            Statement::Factor(_)
            | Statement::Param(_)
            | Statement::Import(_)
            | Statement::Estimate(_)
            | Statement::Output(_) => {}
        }
    }

    /// Analyze a question
    fn analyze_question(&mut self, question: &QuestionStmt) {
        // Validate base_rate if present
        if let Some(base_rate) = &question.base_rate {
            // Check historical_frequency range
            if !(0.0..=1.0).contains(&base_rate.historical_frequency) {
                self.errors.push(SemanticError::ValidationError {
                    rule: "historical_frequency_range".to_string(),
                    message: format!(
                        "historical_frequency must be between 0.0 and 1.0, got {}",
                        base_rate.historical_frequency
                    ),
                });
            }

            // Check that reference_class is not empty
            if base_rate.reference_class.trim().is_empty() {
                self.errors.push(SemanticError::ValidationError {
                    rule: "empty_reference_class".to_string(),
                    message: "reference_class cannot be empty".to_string(),
                });
            }

            // Check that source is not empty
            if base_rate.source.trim().is_empty() {
                self.errors.push(SemanticError::ValidationError {
                    rule: "empty_source".to_string(),
                    message: "base_rate source cannot be empty".to_string(),
                });
            }
        }
    }

    /// Analyze an agent statement
    fn analyze_agent(&mut self, agent: &AgentStmt) {
        // Validate driver_refs point to defined drivers
        for driver_ref in &agent.driver_refs {
            if !self.symbol_table.contains(driver_ref) {
                self.errors.push(SemanticError::UndefinedSymbol {
                    name: driver_ref.clone(),
                    message: format!(
                        "Agent '{}' references undefined driver '{}'",
                        agent.name, driver_ref
                    ),
                });
            }
        }

        // Validate depends_on references exist
        for dep in &agent.depends_on {
            if !self.symbol_table.contains(dep) {
                self.errors.push(SemanticError::UndefinedSymbol {
                    name: dep.clone(),
                    message: format!(
                        "Agent '{}' depends on undefined agent '{}'",
                        agent.name, dep
                    ),
                });
            }
        }

        // Validate confidence_threshold range
        if let Some(threshold) = agent.confidence_threshold {
            if !(0.0..=1.0).contains(&threshold) {
                self.errors.push(SemanticError::ValidationError {
                    rule: "confidence_threshold_range".to_string(),
                    message: format!(
                        "Agent '{}' confidence_threshold must be between 0.0 and 1.0, got {}",
                        agent.name, threshold
                    ),
                });
            }
        }
    }

    /// Check for circular dependencies among all agents
    fn check_agent_circular_dependencies(&mut self, program: &Program) {
        use std::collections::{HashMap, HashSet};

        // Build a map of agent name -> dependencies
        let mut agent_deps: HashMap<String, Vec<String>> = HashMap::new();

        for stmt in &program.statements {
            if let Statement::Agent(agent) = stmt {
                agent_deps.insert(agent.name.clone(), agent.depends_on.clone());
            }
        }

        // Check each agent for cycles using DFS
        for stmt in &program.statements {
            if let Statement::Agent(agent) = stmt {
                let mut visited = HashSet::new();
                let mut rec_stack = vec![agent.name.clone()];

                if let Some(cycle) =
                    self.dfs_detect_cycle(&agent.name, &agent_deps, &mut visited, &mut rec_stack)
                {
                    self.errors.push(SemanticError::ValidationError {
                        rule: "circular_agent_dependency".to_string(),
                        message: format!(
                            "Circular agent dependency detected: {}",
                            cycle.join(" -> ")
                        ),
                    });
                    // Only report one cycle to avoid duplicate errors
                    return;
                }
            }
        }
    }

    /// DFS helper to detect cycles in agent dependency graph
    fn dfs_detect_cycle(
        &self,
        current: &str,
        agent_deps: &std::collections::HashMap<String, Vec<String>>,
        visited: &mut std::collections::HashSet<String>,
        rec_stack: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        // Mark current node as visited
        visited.insert(current.to_string());

        // Get dependencies of current agent
        if let Some(deps) = agent_deps.get(current) {
            for dep in deps {
                // If this dependency is in the recursion stack, we found a cycle
                if let Some(pos) = rec_stack.iter().position(|s| s == dep) {
                    let mut cycle = rec_stack[pos..].to_vec();
                    cycle.push(dep.to_string());
                    return Some(cycle);
                }

                // If not visited, recurse
                if !visited.contains(dep) {
                    rec_stack.push(dep.clone());
                    if let Some(cycle) = self.dfs_detect_cycle(dep, agent_deps, visited, rec_stack)
                    {
                        return Some(cycle);
                    }
                    rec_stack.pop();
                }
            }
        }

        None
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

        // Validate evidence_refs
        for evidence_ref in &driver.evidence_refs {
            if !self.symbol_table.contains(evidence_ref) {
                self.errors.push(SemanticError::UndefinedSymbol {
                    name: evidence_ref.clone(),
                    message: format!(
                        "Driver '{}' references undefined evidence '{}'",
                        driver.name, evidence_ref
                    ),
                });
            }
        }

        // Suggest adding evidence if none provided
        if driver.evidence_refs.is_empty() && driver.rationale.is_none() {
            self.warnings.push(format!(
                "Driver '{}' has no evidence_refs or rationale. Consider adding supporting evidence",
                driver.name
            ));
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
            // Factor model expressions — all produce numbers
            Expression::Residual { .. } => Type::Number,
            Expression::LearnablePrior { .. } => Type::Number,
            Expression::ParamRef(_) => Type::Number,
            Expression::FactorRef(_) => Type::Number,
            Expression::Exp(_) => Type::Number,
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
    /// Every ratio-valued driver in one multiplication chain must act on the same
    /// space.
    ///
    /// ## The defect
    ///
    /// A live Chicago weather forecast read
    ///
    /// ```text
    /// model: 0.067 * seasonal_climatology * enso_phase * synoptic_pattern
    ///        * urban_heat_island * climate_trend
    /// ```
    ///
    /// and, quoting the agents' own rationales, `seasonal_climatology` and
    /// `climate_trend` multiply a bucket PROBABILITY while `enso_phase` and
    /// `synoptic_pattern` multiply a TEMPERATURE. All five were multiplied together
    /// into a probability. The forecast came out at 5.9% — its own base rate — while
    /// the agent that consulted the ensemble had concluded 35%, and the console
    /// reported the 40-point gap to the market as a possible edge.
    ///
    /// ## Why a same-chain rule rather than a target-type rule
    ///
    /// Checking "the model is probability-valued, so every factor must be a
    /// probability ratio" requires inferring the model's target type, and it is
    /// wrong for the correct form of a bucket question:
    ///
    /// ```text
    /// model: high_temp_f >= 77.5 && high_temp_f < 79.5
    /// ```
    ///
    /// That model IS probability-valued — the mean of an indicator over iterations —
    /// and its driver is legitimately a temperature. A target-type rule would reject
    /// the shape we want people to move TO.
    ///
    /// Mixing spaces inside one product, by contrast, is incoherent whatever the
    /// product feeds, and it is decidable locally without inferring anything. It
    /// catches Chicago exactly and cannot fire on an indicator model, because there
    /// the drivers are not multiplied by each other.
    ///
    /// ## Severity
    ///
    /// A declared mix is an ERROR. An undeclared driver is a WARNING, because the
    /// stored corpus predates the field and silence is honest ignorance; treating
    /// absence as `Probability` would reinstate the guess that caused this.
    /// A driver's declared distribution must satisfy the driver's own constraints.
    ///
    /// ## Why this is the enforcement point
    ///
    /// `constraint:` states the legal domain of a driver's value. The obvious place
    /// to enforce it is wherever a value is proposed — but a *prior* that can
    /// produce illegal values is a defect that exists before any agent runs, and it
    /// is decidable now, from the file, with no data and no plumbing.
    ///
    /// It also replaces something worse. `assertions.rs` enforces one hardcoded
    /// `[0.1, 3.0]` on every multiplier on the platform, while twelve cards declare
    /// twelve different ranges — `biotech_analyst` is invited to 0.05 and rejected
    /// at it, `sentiment_analyzer` is capped at 0.3 by its card and accepted at 0.15
    /// by the runtime. A per-driver declaration is the honest home for that bound.
    ///
    /// ## What it cannot do
    ///
    /// Drivers whose distribution parameters are `param` references
    /// (`triangular(socio_p5, socio_p50, socio_p95)`, which is 48 of the stored
    /// corpus) are skipped: the bounds are not known until instantiation. Skipping
    /// is stated here rather than silently passing, because "no error" on those
    /// drivers means "not checked", not "checked and clean".
    fn check_driver_constraints(&mut self, program: &Program) {
        for driver in program.drivers() {
            if driver.constraints.is_empty() {
                continue;
            }
            let Some(dist) = &driver.distribution else {
                continue;
            };
            let Some((low, high)) = distribution_bounds(dist) else {
                continue;
            };

            for c in &driver.constraints {
                for (edge, value) in [("low", low), ("high", high)] {
                    let mut ctx = crate::evaluator::EvaluationContext::new();
                    ctx.set(driver.name.clone(), value);
                    // A constraint that cannot be evaluated (references another
                    // driver, say) is not a violation — it is out of scope for a
                    // static check and must not be reported as a failure.
                    if let Ok(result) = crate::evaluator::evaluate(&c.condition, &ctx) {
                        if result == 0.0 {
                            self.errors.push(SemanticError::ValidationError {
                                rule: "driver_constraint_violated_by_own_prior".to_string(),
                                message: format!(
                                    "driver '{}' declares a constraint its own \
                                     distribution can violate: at the {edge} end of \
                                     its range ({value}) the constraint is false. The \
                                     prior can therefore produce values the driver \
                                     declares illegal.",
                                    driver.name
                                ),
                            });
                        }
                    }
                }
            }
        }
    }

    fn check_driver_spaces(&mut self, program: &Program) {
        use std::collections::BTreeMap;

        let spaces: BTreeMap<&str, Option<crate::ast::AppliesTo>> = program
            .statements
            .iter()
            .filter_map(|s| match s {
                Statement::Driver(d) => Some((d.name.as_str(), d.applies_to)),
                _ => None,
            })
            .collect();

        let Some(model) = program.statements.iter().find_map(|s| match s {
            Statement::Model(m) => Some(m),
            _ => None,
        }) else {
            return;
        };

        // Every maximal `a * b * c` chain in the expression, as driver names.
        let mut chains: Vec<Vec<String>> = Vec::new();
        collect_product_chains(&model.expression, &mut chains);

        let mut undeclared: Vec<String> = Vec::new();

        for chain in &chains {
            let mut seen: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
            for name in chain {
                match spaces.get(name.as_str()) {
                    Some(Some(space)) => seen.entry(space.as_str()).or_default().push(name.clone()),
                    // A driver with no declaration, in a chain. Collected once
                    // below rather than per chain, so a driver used twice is
                    // reported once.
                    Some(None) => {
                        if !undeclared.contains(name) {
                            undeclared.push(name.clone());
                        }
                    }
                    // Not a driver — a param, a literal, a function result.
                    None => {}
                }
            }
            if seen.len() > 1 {
                let detail = seen
                    .iter()
                    .map(|(space, names)| format!("{space}: {}", names.join(", ")))
                    .collect::<Vec<_>>()
                    .join(" | ");
                self.errors.push(SemanticError::ValidationError {
                    rule: "driver_space_consistency".to_string(),
                    message: format!(
                        "one product multiplies ratios that act on different things \
                         ({detail}). A temperature ratio and a probability ratio are \
                         not commensurable, so their product is not a quantity. \
                         Compose the quantity drivers into the quantity, then take an \
                         indicator over it."
                    ),
                });
            }
        }

        // ONE warning per program, not one per driver.
        //
        // The per-driver version was measured against the stored corpus first: 78
        // of 78 programs warned, 48 of them about `dynamic_performance` alone and
        // 21 about `conditions`. Six lines of identical advice per forecast is how
        // a diagnostics panel becomes wallpaper, and the panel had existed for one
        // commit at that point. Naming the drivers once, in a single line, says the
        // same thing and can be read.
        if !undeclared.is_empty() {
            let n = undeclared.len();
            self.warnings.push(format!(
                "{n} multiplied driver(s) declare no `applies_to` \
                 (probability or quantity), so nothing can check that this product \
                 is dimensionally coherent: {}",
                undeclared.join(", ")
            ));
        }
    }

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

        // Factor-model programs (TEAM_PRIOR, TOURNAMENT_PATH, H2H_MATCH) replace
        // driver+model with factor+estimate. Skip driver/model rules in that case.
        let has_factors = program
            .statements
            .iter()
            .any(|s| matches!(s, Statement::Factor(_)));
        let has_estimate = program
            .statements
            .iter()
            .any(|s| matches!(s, Statement::Estimate(_)));
        let is_factor_model = has_factors && has_estimate;

        // Rule: Should have at least one driver — UNLESS this is a factor model.
        if self.symbol_table.drivers().is_empty() && !is_factor_model {
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

        // Factor-model integrity: variance shares should sum to ~1.0
        if is_factor_model {
            let total: f64 = program
                .statements
                .iter()
                .filter_map(|s| match s {
                    Statement::Factor(f) => Some(f.variance_share),
                    _ => None,
                })
                .sum();
            if (total - 1.0).abs() > 0.05 {
                self.warnings.push(format!(
                    "Factor variance shares sum to {:.3} (expected ~1.0). Check variance budget.",
                    total
                ));
            }
        }

        // Rule: Should have a question
        let question = program.statements.iter().find_map(|s| {
            if let Statement::Question(q) = s {
                Some(q)
            } else {
                None
            }
        });

        if question.is_none() {
            self.warnings
                .push("Forecast should have a question statement".to_string());
        }

        // Rule: Question should have base_rate (Tetlock methodology)
        if let Some(q) = question {
            if q.base_rate.is_none() {
                self.warnings.push(
                    "⚠️  Missing base_rate: Start with outside view (base rate from reference class) before inside analysis. This is essential for proper forecasting methodology.".to_string()
                );
            }
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

/// Statically evaluate a distribution parameter, when it is a plain number.
///
/// `triangular(socio_p5, socio_p50, socio_p95)` binds params at instantiation and
/// cannot be checked here, so those drivers are skipped rather than guessed at.
fn static_number(expr: &Expression) -> Option<f64> {
    match expr {
        Expression::Number(n) => Some(*n),
        Expression::Probability(p) => Some(*p),
        _ => None,
    }
}

/// The values a declared distribution can actually produce, when they are known.
///
/// Returns `(low, high)`. For bounded families these are exact; for Normal it is
/// mean +/- 3 sigma, which covers 99.7% of draws — a constraint violated there is
/// violated often enough to matter, and treating Normal as uncheckable would exempt
/// the family the reference forecast uses.
fn distribution_bounds(dist: &Distribution) -> Option<(f64, f64)> {
    match dist {
        Distribution::Triangular { p5, p95, .. } => Some((static_number(p5)?, static_number(p95)?)),
        Distribution::Uniform { low, high } => Some((static_number(low)?, static_number(high)?)),
        Distribution::Normal { mean, stddev } => {
            let (m, s) = (static_number(mean)?, static_number(stddev)?);
            Some((m - 3.0 * s, m + 3.0 * s))
        }
        Distribution::Beta { min, max, .. } => Some((
            min.as_ref().and_then(static_number).unwrap_or(0.0),
            max.as_ref().and_then(static_number).unwrap_or(1.0),
        )),
        // Lognormal is unbounded above and its low tail approaches zero; there is no
        // honest finite bound to test, so it is skipped rather than approximated.
        _ => None,
    }
}

/// Collect every maximal multiplication chain in an expression, as identifier names.
///
/// `0.067 * a * b` yields one chain `[a, b]`; the numeric literal is not a driver
/// and is skipped. Division is deliberately NOT treated as part of a chain: `a / b`
/// is a different dimensional relationship and lumping it in would report a false
/// mix for a legitimate ratio-of-ratios.
fn collect_product_chains(expr: &Expression, out: &mut Vec<Vec<String>>) {
    match expr {
        Expression::Multiply(_, _) => {
            let mut names = Vec::new();
            flatten_product(expr, &mut names, out);
            if names.len() > 1 {
                out.push(names);
            }
        }
        // Recurse through everything else so a product nested inside a comparison,
        // a sum or a call is still examined.
        Expression::Add(l, r)
        | Expression::Subtract(l, r)
        | Expression::Divide(l, r)
        | Expression::Modulo(l, r)
        | Expression::Power(l, r)
        | Expression::Greater(l, r)
        | Expression::Less(l, r)
        | Expression::GreaterEqual(l, r)
        | Expression::LessEqual(l, r)
        | Expression::Equal(l, r)
        | Expression::NotEqual(l, r)
        | Expression::And(l, r)
        | Expression::Or(l, r) => {
            collect_product_chains(l, out);
            collect_product_chains(r, out);
        }
        _ => {}
    }
}

/// Flatten a left-nested `Multiply` tree into the identifier names it multiplies,
/// recursing into non-identifier operands so nested products are not lost.
fn flatten_product(expr: &Expression, names: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
    match expr {
        Expression::Multiply(l, r) => {
            flatten_product(l, names, out);
            flatten_product(r, names, out);
        }
        Expression::Identifier(n) => names.push(n.clone()),
        // `(driver ^ 1.8)` is still that driver's contribution to the product: the
        // World Cup models raise every factor to a weight, and ignoring the base
        // would make this rule blind to exactly the family that uses exponents.
        Expression::Power(base, _) => flatten_product(base, names, out),
        other => collect_product_chains(other, out),
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

    #[test]
    fn test_agent_valid_driver_refs() {
        let source = r#"
driver market_share continuous {
    distribution: triangular(0.15, 0.20, 0.25)
}

agent market_research {
    type: "research"
    query: "AMD market share trends"
    driver_refs: ["market_share"]
}

model: market_share
simulate 10000 iterations
"#;

        let analysis = analyze_source(source);
        assert!(analysis.is_valid(), "Errors: {:?}", analysis.errors);
    }

    #[test]
    fn test_agent_undefined_driver_ref() {
        let source = r#"
driver market_share continuous {
    distribution: triangular(0.15, 0.20, 0.25)
}

agent market_research {
    type: "research"
    query: "AMD market share trends"
    driver_refs: ["market_share", "nonexistent_driver"]
}
"#;

        let analysis = analyze_source(source);
        assert!(!analysis.is_valid());
        assert!(analysis.errors.iter().any(|e| {
            matches!(e, SemanticError::UndefinedSymbol { name, .. } if name == "nonexistent_driver")
        }));
    }

    #[test]
    fn test_agent_undefined_dependency() {
        let source = r#"
agent base_research {
    type: "research"
    query: "Base research"
}

agent competitive_analysis {
    type: "competitive"
    query: "Competitive dynamics"
    depends_on: ["base_research", "nonexistent_agent"]
}
"#;

        let analysis = analyze_source(source);
        assert!(!analysis.is_valid());
        assert!(analysis.errors.iter().any(|e| {
            matches!(e, SemanticError::UndefinedSymbol { name, .. } if name == "nonexistent_agent")
        }));
    }

    #[test]
    fn test_agent_confidence_threshold_valid() {
        let source = r#"
driver test_driver continuous {
    distribution: normal(100, 10)
}

agent market_research {
    type: "research"
    query: "Market research"
    confidence_threshold: 0.75
}

model: test_driver
simulate 10000 iterations
"#;

        let analysis = analyze_source(source);
        assert!(analysis.is_valid(), "Errors: {:?}", analysis.errors);
    }

    #[test]
    fn test_agent_confidence_threshold_invalid() {
        // Note: Parser already validates confidence_threshold range,
        // so we can't test values outside 0.0-1.0 without parser error.
        // This test verifies that the semantic analyzer would catch it
        // if a value somehow bypassed the parser.
        // The parser validation at src/parser.rs:684 prevents this case.

        // Instead, test that valid edge cases (0.0 and 1.0) pass
        let source = r#"
driver test_driver continuous {
    distribution: normal(100, 10)
}

agent market_research {
    type: "research"
    query: "Market research"
    confidence_threshold: 1.0
}

model: test_driver
simulate 10000 iterations
"#;

        let analysis = analyze_source(source);
        assert!(
            analysis.is_valid(),
            "Edge case 1.0 should be valid. Errors: {:?}",
            analysis.errors
        );

        let source2 = r#"
driver test_driver continuous {
    distribution: normal(100, 10)
}

agent market_research {
    type: "research"
    query: "Market research"
    confidence_threshold: 0.0
}

model: test_driver
simulate 10000 iterations
"#;

        let analysis2 = analyze_source(source2);
        assert!(
            analysis2.is_valid(),
            "Edge case 0.0 should be valid. Errors: {:?}",
            analysis2.errors
        );
    }

    #[test]
    fn test_agent_circular_dependency_simple() {
        let source = r#"
agent agent_a {
    type: "research"
    query: "Agent A"
    depends_on: ["agent_b"]
}

agent agent_b {
    type: "research"
    query: "Agent B"
    depends_on: ["agent_a"]
}
"#;

        let analysis = analyze_source(source);
        assert!(!analysis.is_valid());
        assert!(analysis.errors.iter().any(|e| {
            matches!(e, SemanticError::ValidationError { rule, .. } if rule == "circular_agent_dependency")
        }));
    }

    #[test]
    fn test_agent_circular_dependency_complex() {
        let source = r#"
agent agent_a {
    type: "research"
    query: "Agent A"
    depends_on: ["agent_b"]
}

agent agent_b {
    type: "research"
    query: "Agent B"
    depends_on: ["agent_c"]
}

agent agent_c {
    type: "research"
    query: "Agent C"
    depends_on: ["agent_a"]
}
"#;

        let analysis = analyze_source(source);
        assert!(!analysis.is_valid());
        assert!(analysis.errors.iter().any(|e| {
            matches!(e, SemanticError::ValidationError { rule, .. } if rule == "circular_agent_dependency")
        }));
    }

    #[test]
    fn test_agent_valid_dependency_chain() {
        let source = r#"
driver test_driver continuous {
    distribution: normal(100, 10)
}

agent base_research {
    type: "research"
    query: "Base research"
}

agent sentiment {
    type: "sentiment"
    query: "Sentiment analysis"
}

agent competitive {
    type: "competitive"
    query: "Competitive analysis"
    depends_on: ["base_research", "sentiment"]
}

model: test_driver
simulate 10000 iterations
"#;

        let analysis = analyze_source(source);
        assert!(analysis.is_valid(), "Errors: {:?}", analysis.errors);
    }

    /// The Chicago shape, with the spaces its agents actually described.
    ///
    /// A rule that cannot fire is worse than no rule, so this is the falsifiability
    /// probe: two probability ratios and two quantity ratios in one product, which
    /// is what the live forecast contained, and it must be an ERROR.
    #[test]
    fn a_product_mixing_probability_and_quantity_ratios_is_an_error() {
        let src = r#"
question "Will the high be 78-79F?" {
    base_rate {
        reference_class: "August days"
        historical_frequency: 6.7%
        sample_size: 930
        source: "climatology"
        generated_by: macro_forecaster
    }
}

driver climate_trend continuous {
    distribution: triangular(0.8, 0.87, 0.95)
    unit: "multiplier"
    applies_to: probability
}

driver synoptic_pattern continuous {
    distribution: triangular(0.55, 1.0, 1.75)
    unit: "multiplier"
    applies_to: quantity
}

model: 0.067 * climate_trend * synoptic_pattern

simulate 1000 iterations
"#;
        let analysis = analyze_source(src);
        let msgs: Vec<String> = analysis.errors.iter().map(|e| e.to_string()).collect();
        assert!(
            msgs.iter().any(|m| m.contains("different things")),
            "the mixed-space product must be an error; got {msgs:?}"
        );
        // Both offenders are named, because "something is wrong somewhere" is not
        // actionable in a five-driver model.
        assert!(
            msgs.iter()
                .any(|m| m.contains("climate_trend") && m.contains("synoptic_pattern")),
            "both sides of the mix must be named; got {msgs:?}"
        );
    }

    /// One space throughout is fine, which is what stops this rule being a blanket
    /// objection to multiplication.
    #[test]
    fn a_product_of_one_space_is_accepted() {
        let src = r#"
question "Will the high be 78-79F?" {
    base_rate {
        reference_class: "August days"
        historical_frequency: 6.7%
        sample_size: 930
        source: "climatology"
        generated_by: macro_forecaster
    }
}

driver climate_trend continuous {
    distribution: triangular(0.8, 0.87, 0.95)
    unit: "multiplier"
    applies_to: probability
}

driver seasonal_climatology continuous {
    distribution: triangular(0.77, 0.92, 1.07)
    unit: "multiplier"
    applies_to: probability
}

model: 0.067 * climate_trend * seasonal_climatology

simulate 1000 iterations
"#;
        let analysis = analyze_source(src);
        assert!(
            analysis.is_valid(),
            "a single-space product must pass: {:?}",
            analysis
                .errors
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
        );
    }

    /// The shape we want people to move TO must not be rejected.
    ///
    /// For a bucket question the correct model is an indicator over a quantity —
    /// `high_temp_f >= 77.5 && high_temp_f < 79.5` — whose Monte Carlo mean IS the
    /// bucket probability. Its driver is legitimately a temperature. A rule keyed on
    /// "the model is probability-valued so every factor must be a probability ratio"
    /// would reject exactly this, which is why the rule is same-chain instead.
    #[test]
    fn an_indicator_over_a_quantity_driver_is_not_flagged() {
        let src = r#"
question "Will the high be 78-79F?" {
    base_rate {
        reference_class: "August days"
        historical_frequency: 6.7%
        sample_size: 930
        source: "climatology"
        generated_by: macro_forecaster
    }
}

driver high_temp_f continuous {
    distribution: normal(79.3, 3.2)
    unit: "degF"
    applies_to: quantity
}

model: high_temp_f >= 77.5

simulate 1000 iterations
"#;
        let analysis = analyze_source(src);
        assert!(
            analysis.is_valid(),
            "an indicator over a quantity must pass: {:?}",
            analysis
                .errors
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
        );
        assert!(
            !analysis.warnings.iter().any(|w| w.contains("applies_to")),
            "a declared driver must not warn about being undeclared: {:?}",
            analysis.warnings
        );
    }

    /// Exponents do not hide a driver from the rule.
    ///
    /// The World Cup family writes `(dynamic_performance ^ 1.8)`, so a walker that
    /// only recognised bare identifiers would be blind to the 48-forecast family
    /// that uses weights.
    #[test]
    fn a_weighted_factor_is_still_part_of_the_product() {
        let src = r#"
question "Will they win?" {
    base_rate {
        reference_class: "48-team field"
        historical_frequency: 2.08%
        sample_size: 48
        source: "field size"
        generated_by: macro_forecaster
    }
}

driver squad_quality continuous {
    distribution: triangular(0.9, 1.1, 1.3)
    unit: "multiplier"
    applies_to: probability
}

driver expected_goals continuous {
    distribution: triangular(1.0, 1.5, 2.0)
    unit: "goals"
    applies_to: quantity
}

model: 0.0208 * (squad_quality ^ 1.5) * (expected_goals ^ 0.5)

simulate 1000 iterations
"#;
        let analysis = analyze_source(src);
        let msgs: Vec<String> = analysis.errors.iter().map(|e| e.to_string()).collect();
        assert!(
            msgs.iter().any(|m| m.contains("different things")),
            "a mix under exponents must still be caught; got {msgs:?}"
        );
    }

    /// Undeclared drivers produce ONE warning naming them, not one warning each.
    ///
    /// Measured before this was aggregated: 78 of 78 stored programs warned, 48 of
    /// them about a single driver name. A panel that prints six lines of identical
    /// advice per forecast stops being read.
    #[test]
    fn undeclared_drivers_are_reported_once_together() {
        let src = r#"
question "Will they win?" {
    base_rate {
        reference_class: "field"
        historical_frequency: 2.08%
        sample_size: 48
        source: "field size"
        generated_by: macro_forecaster
    }
}

driver first_factor continuous {
    distribution: triangular(0.9, 1.0, 1.1)
    unit: "multiplier"
}

driver second_factor continuous {
    distribution: triangular(0.9, 1.0, 1.1)
    unit: "multiplier"
}

model: 0.0208 * first_factor * second_factor

simulate 1000 iterations
"#;
        let analysis = analyze_source(src);
        let about: Vec<&String> = analysis
            .warnings
            .iter()
            .filter(|w| w.contains("applies_to"))
            .collect();
        assert_eq!(
            about.len(),
            1,
            "expected exactly one aggregated warning, got {about:?}"
        );
        assert!(about[0].contains("first_factor") && about[0].contains("second_factor"));
        // Undeclared is a warning, never an error: the stored corpus predates the
        // field and absence is honest ignorance rather than a mistake.
        assert!(
            analysis.is_valid(),
            "{:?}",
            analysis
                .errors
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
        );
    }

    /// A prior that can produce values its own constraint forbids.
    ///
    /// The falsifiability probe. `constraint:` was dead at both ends — the parser
    /// held `let constraints = Vec::new();` (not even `mut`) and nothing read the
    /// field — so a rule that could not fire would be indistinguishable from the
    /// state it replaced.
    #[test]
    fn a_prior_that_violates_its_own_constraint_is_an_error() {
        let src = r#"
question "Will it?" {
    base_rate {
        reference_class: "days"
        historical_frequency: 10%
        sample_size: 100
        source: "fixture"
        generated_by: macro_forecaster
    }
}

driver adjustment continuous {
    distribution: triangular(0.05, 1.0, 2.0)
    unit: "multiplier"
    applies_to: probability
    constraint: adjustment >= 0.1
}

model: 0.1 * adjustment

simulate 1000 iterations
"#;
        let analysis = analyze_source(src);
        let msgs: Vec<String> = analysis.errors.iter().map(|e| e.to_string()).collect();
        assert!(
            msgs.iter().any(|m| m.contains("can violate")),
            "a prior reaching 0.05 under a `>= 0.1` constraint must be an error; \
             got {msgs:?}"
        );
        // The driver and the offending end are named: "a constraint is violated"
        // is not actionable in a six-driver model.
        assert!(
            msgs.iter()
                .any(|m| m.contains("adjustment") && m.contains("low")),
            "the driver and the end of its range must be named; got {msgs:?}"
        );
    }

    /// A prior that respects its constraint passes, so the rule is not a blanket
    /// objection to declaring one.
    #[test]
    fn a_prior_inside_its_constraint_is_accepted() {
        let src = r#"
question "Will it?" {
    base_rate {
        reference_class: "days"
        historical_frequency: 10%
        sample_size: 100
        source: "fixture"
        generated_by: macro_forecaster
    }
}

driver adjustment continuous {
    distribution: triangular(0.2, 1.0, 2.0)
    unit: "multiplier"
    applies_to: probability
    constraint: adjustment >= 0.1
}

model: 0.1 * adjustment

simulate 1000 iterations
"#;
        let analysis = analyze_source(src);
        assert!(
            analysis.is_valid(),
            "{:?}",
            analysis
                .errors
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
        );
    }

    /// Both ends are tested, not only the low one.
    ///
    /// An upper bound is the case that matters for the multiplier ceiling — a
    /// driver declared `[0.55, 1.75]` while the honest value needed 2.67 — so a
    /// check that only looked downward would miss the defect that started this.
    #[test]
    fn the_upper_end_of_a_prior_is_checked_too() {
        let src = r#"
question "Will it?" {
    base_rate {
        reference_class: "days"
        historical_frequency: 10%
        sample_size: 100
        source: "fixture"
        generated_by: macro_forecaster
    }
}

driver adjustment continuous {
    distribution: triangular(0.5, 1.0, 4.0)
    unit: "multiplier"
    applies_to: probability
    constraint: adjustment <= 3.0
}

model: 0.1 * adjustment

simulate 1000 iterations
"#;
        let analysis = analyze_source(src);
        let msgs: Vec<String> = analysis.errors.iter().map(|e| e.to_string()).collect();
        assert!(
            msgs.iter().any(|m| m.contains("high")),
            "the high end must be checked; got {msgs:?}"
        );
    }

    /// A driver whose bounds are params is SKIPPED, not silently passed.
    ///
    /// 48 of the stored corpus write `triangular(socio_p5, socio_p50, socio_p95)`,
    /// whose bounds are unknown until instantiation. The test records that no error
    /// is raised AND that this means "not checked" rather than "checked and clean".
    #[test]
    fn a_prior_built_from_params_is_not_checkable_here() {
        let src = r#"
question "Will it?" {
    base_rate {
        reference_class: "days"
        historical_frequency: 10%
        sample_size: 100
        source: "fixture"
        generated_by: macro_forecaster
    }
}

param lo: real
param mid: real
param hi: real

driver adjustment continuous {
    distribution: triangular(lo, mid, hi)
    unit: "multiplier"
    applies_to: probability
    constraint: adjustment >= 0.1
}

model: 0.1 * adjustment

simulate 1000 iterations
"#;
        let analysis = analyze_source(src);
        assert!(
            analysis.is_valid(),
            "param-bounded priors cannot be statically checked and must not error: {:?}",
            analysis
                .errors
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
        );
    }

    /// Normal is checked at +/- 3 sigma rather than exempted.
    ///
    /// The reference forecast uses `normal(0.0, 2.796)`, so treating unbounded
    /// families as uncheckable would exempt the shape we are moving toward.
    #[test]
    fn a_normal_prior_is_checked_at_three_sigma() {
        let src = r#"
question "Will it?" {
    base_rate {
        reference_class: "days"
        historical_frequency: 10%
        sample_size: 100
        source: "fixture"
        generated_by: macro_forecaster
    }
}

driver error_f continuous {
    distribution: normal(0.0, 2.0)
    unit: "degF"
    applies_to: quantity
    constraint: error_f >= -3.0
}

model: error_f >= 0.0

simulate 1000 iterations
"#;
        let analysis = analyze_source(src);
        let msgs: Vec<String> = analysis.errors.iter().map(|e| e.to_string()).collect();
        assert!(
            msgs.iter().any(|m| m.contains("error_f")),
            "-3 sigma is -6.0, which violates `>= -3.0`; got {msgs:?}"
        );
    }
}

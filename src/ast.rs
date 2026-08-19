/// Abstract Syntax Tree (AST) Node Definitions
///
/// The AST represents the hierarchical structure of an FPL program.
/// Each node corresponds to a language construct (question, driver, etc.)
use std::fmt;

/// Root node - represents an entire FPL program
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub statements: Vec<Statement>,
}

/// Top-level statements in FPL
#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Question(QuestionStmt),
    Driver(DriverStmt),
    Evidence(EvidenceStmt),
    Agent(AgentStmt),
    Model(ModelStmt),
    Simulate(SimulateStmt),
    // Factor model extensions
    Factor(FactorStmt),
    Param(ParamDecl),
    Import(ImportStmt),
    Estimate(EstimateStmt),
    Output(OutputStmt),
}

/// Question statement: defines the forecast question
#[derive(Debug, Clone, PartialEq)]
pub struct QuestionStmt {
    pub text: String,
    pub base_rate: Option<BaseRate>,
    pub target_date: Option<String>,
    pub resolution_criteria: Option<String>,
}

/// Base Rate (Outside View) - Tetlock methodology
#[derive(Debug, Clone, PartialEq)]
pub struct BaseRate {
    pub reference_class: String,
    pub historical_frequency: f64, // 0.0 to 1.0
    pub sample_size: Option<usize>,
    pub source: String,
    pub reasoning: Option<String>,
    pub generated_by: GeneratedBy,
}

/// Who generated the base rate
#[derive(Debug, Clone, PartialEq)]
pub enum GeneratedBy {
    Agent(String), // Agent name
    Human,
}

/// Driver statement: defines a forecasting driver.
///
/// A driver is "learnable" when the user opts into BayesOps managing its
/// distribution from historical observations. The static `distribution`
/// field then acts as the prior: it's used when no fit is available (cold
/// start) and as the conjugate prior when there's data. At sim time the
/// executor looks for a `<driver_name>_fitted` JSON value in its parameter
/// context; if present it overrides the static distribution. This is how
/// BayesOps' `FittedDistribution` flows back into FPL without rewriting the
/// source — same pattern as the learnable elasticities contract documented
/// in docs/fermi/BAYESOPS_CONTRACT.md.
#[derive(Debug, Clone, PartialEq)]
pub struct DriverStmt {
    pub name: String,
    pub display_name: Option<String>, // Human-readable name
    pub description: Option<String>,  // Natural language description
    pub driver_type: DriverType,
    pub distribution: Option<Distribution>, // For continuous drivers
    pub probability: Option<f64>,           // For binary drivers
    pub impact_multiplier: Option<f64>,     // For binary drivers
    pub values: Option<Vec<f64>>,           // For discrete drivers
    pub weights: Option<Vec<f64>>,          // For discrete drivers (must sum to 1)
    pub unit: Option<String>,
    /// For a RATIO-valued driver: what the ratio multiplies.
    ///
    /// `unit: "multiplier"` says a driver is a ratio and never says a ratio of
    /// WHAT, and two agents filling the same slot read it two different ways.
    /// Measured on one live Chicago weather forecast, quoting the agents' own
    /// rationales:
    ///
    /// | driver | value | what it says it multiplies |
    /// | --- | --- | --- |
    /// | `seasonal_climatology` | 0.92 | "the climatological FREQUENCY of the bucket" |
    /// | `climate_trend` | 0.87 | "the bucket PROBABILITY, 13.1% to 11.4%" |
    /// | `enso_phase` | 1.00 | "the TEMPERATURE MEAN driver, not the bucket" |
    /// | `synoptic_pattern` | 1.00 | "the orchestra's broad TEMPERATURE prior" |
    ///
    /// `model: 0.067 * seasonal_climatology * enso_phase * synoptic_pattern * ...`
    /// multiplies all five into a probability as though commensurable. Two of them
    /// are not probabilities. The declared range is incoherent in both readings
    /// too: as a probability ratio `synoptic_pattern` needed 2.67x against a
    /// ceiling of 1.75; as a temperature ratio, 0.55 x 79F is 43F.
    ///
    /// `None` means undeclared, which is a warning rather than an error — the
    /// corpus predates this field and a missing declaration is honest ignorance.
    /// Defaulting it to `Probability` would silently reinstate exactly the guess
    /// that caused the defect.
    pub applies_to: Option<AppliesTo>,
    pub rationale: Option<String>,
    pub constraints: Vec<Constraint>,
    pub evidence_refs: Vec<String>,
    /// When true, BayesOps owns this driver's distribution. The `distribution`
    /// field above is the prior; the live posterior is read at sim time from
    /// `params.<name>_fitted` (FittedDistribution JSON). Default: false.
    pub learnable: bool,
    /// How upstream workspace resolutions translate into observations for
    /// fitting this driver. Only meaningful when `learnable = true`. The
    /// refit hook (`src/handlers/workspace/refit.rs`) walks
    /// `workspace_dependencies`, applies the named extractor to each
    /// upstream's resolution outcome, and folds the result into the
    /// observation vector before calling `fit_marginal()`.
    ///
    /// `None` means the refit hook can still fit this driver from an
    /// explicit `workspace_outputs[ws].observations.<driver_name>` array,
    /// but cannot derive observations from upstream resolutions itself.
    ///
    /// See `docs/specs/23_BAYESOPS_WORLD_CUP_DEMO.md` §3.4.
    pub feeds_from: Option<FeedsFrom>,
}

/// Declaration of how a learnable driver derives observations from upstream
/// workspace resolutions. See `DriverStmt::feeds_from`.
#[derive(Debug, Clone, PartialEq)]
pub struct FeedsFrom {
    /// Source of observations. Only `"upstream_resolutions"` is supported
    /// today; the field is a string so future sources (e.g.
    /// `"downstream_outputs"`, `"by_tag"`) can be added without an AST
    /// rewrite.
    pub source: String,
    /// Name of an [`Extractor`](posterior::Extractor) registered in the
    /// server's `ExtractorRegistry`. Examples: `binary_winner_id_match`,
    /// `scalar_field_value`.
    pub extractor: String,
    /// Extractor-specific config. Free-form key→value map; the FPL parser
    /// preserves it as a JSON object. The extractor validates it at refit
    /// time.
    pub config: serde_json::Value,
    /// Optional per-driver override for the auto-accept threshold (in
    /// percentage points of forecast rate). When `None`, the refit hook
    /// uses its global default (currently 2.0 pp).
    pub auto_accept_threshold_pp: Option<f64>,
}

/// Type of driver
#[derive(Debug, Clone, PartialEq)]
pub enum DriverType {
    Continuous,
    Binary,
    Discrete,
}

/// Distribution types for continuous drivers
#[derive(Debug, Clone, PartialEq)]
pub enum Distribution {
    Triangular {
        p5: Expression,
        p50: Expression,
        p95: Expression,
    },
    Normal {
        mean: Expression,
        stddev: Expression,
    },
    Lognormal {
        median: Expression,
        sigma: Expression,
    },
    Uniform {
        low: Expression,
        high: Expression,
    },
    Beta {
        alpha: Expression,
        beta: Expression,
        min: Option<Expression>,
        max: Option<Expression>,
    },
}

/// Constraint on a driver
#[derive(Debug, Clone, PartialEq)]
pub struct Constraint {
    pub condition: Expression,
    pub message: Option<String>,
}

/// The space a ratio-valued driver acts on. See [`DriverStmt::applies_to`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppliesTo {
    /// Multiplies a probability. Composes with a base rate.
    Probability,
    /// Multiplies a physical quantity — a temperature, a goal count, a revenue.
    /// Cannot be multiplied into a probability without a link function.
    Quantity,
}

impl AppliesTo {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Probability => "probability",
            Self::Quantity => "quantity",
        }
    }
}

/// Evidence statement: defines supporting evidence
#[derive(Debug, Clone, PartialEq)]
pub struct EvidenceStmt {
    pub id: String,
    pub source: String,
    pub summary: Option<String>,
    pub url: Option<String>,
    pub relevance: Option<f64>,
    pub date: Option<String>,
    pub strength: Option<f64>,
    pub key_findings: Vec<String>,
}

/// Executor type for agent execution
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecutorType {
    LLM,
    MCP,
    Manual,
    Skill,
}

impl fmt::Display for ExecutorType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ExecutorType::LLM => write!(f, "llm"),
            ExecutorType::MCP => write!(f, "mcp"),
            ExecutorType::Manual => write!(f, "manual"),
            ExecutorType::Skill => write!(f, "skill"),
        }
    }
}

/// Agent statement: defines a research agent
#[derive(Debug, Clone, PartialEq)]
pub struct AgentStmt {
    pub name: String,
    pub agent_type: Option<String>, // research, sentiment, competitive, etc.
    pub query: String,
    pub executor: Option<ExecutorType>, // How agent executes (llm, mcp, manual, skill)
    pub schedule: Option<Schedule>,
    pub driver_refs: Vec<String>,
    pub depends_on: Vec<String>, // Other agents this agent depends on
    pub confidence_threshold: Option<f64>, // Minimum confidence (0.0-1.0) to accept agent output
}

/// Schedule for agent execution
#[derive(Debug, Clone, PartialEq)]
pub enum Schedule {
    Once,
    Every { interval: u32, unit: TimeUnit },
    Cron(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum TimeUnit {
    Minute,
    Hour,
    Day,
    Week,
    Month,
}

/// Model statement: defines the forecast model
#[derive(Debug, Clone, PartialEq)]
pub struct ModelStmt {
    pub expression: Expression,
}

/// Simulate statement: runs the Monte Carlo simulation
#[derive(Debug, Clone, PartialEq)]
pub struct SimulateStmt {
    pub iterations: u32,
    pub target: Option<Expression>,
}

// ═══════════════════════════════════════════════════════════════════
// Factor Model Extensions — 6-factor orthogonal decomposition
// ═══════════════════════════════════════════════════════════════════

/// Factor declaration: defines an orthogonal factor with inputs, formulation, and variance share.
#[derive(Debug, Clone, PartialEq)]
pub struct FactorStmt {
    pub name: String,
    pub label: String,
    pub inputs: Vec<FactorInput>,
    pub formulation: Option<Expression>,
    pub variance_share: f64,
    pub update_frequency: UpdateFreq,
}

/// A single input to a factor.
#[derive(Debug, Clone, PartialEq)]
pub struct FactorInput {
    pub name: String,
    pub input_type: ParamType,
}

/// Update frequency for a factor.
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateFreq {
    Static,
    PerMatch,
    TournamentStart,
    PerFixture,
}

/// Parameter declaration: typed parameter bound at instantiation.
#[derive(Debug, Clone, PartialEq)]
pub struct ParamDecl {
    pub name: String,
    pub param_type: ParamType,
    pub default_value: Option<Expression>,
}

/// Parameter types.
#[derive(Debug, Clone, PartialEq)]
pub enum ParamType {
    Real,
    Int,
    Str,
    Bool,
}

/// Import statement: import a factor with bound values.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportStmt {
    pub factor_name: String,
    pub bindings: Vec<(String, Expression)>,
}

/// Estimate statement: defines a response function (e.g., Cobb-Douglas).
#[derive(Debug, Clone, PartialEq)]
pub struct EstimateStmt {
    pub name: String,
    pub expression: Expression,
}

/// Output declaration: a named output derived from the model.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputStmt {
    pub name: String,
    pub expression: Option<Expression>,
    pub is_derived: bool,
}

/// Expressions - used in models, distributions, constraints
#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    // Literals
    Number(f64),
    Probability(f64),
    String(String),
    Boolean(bool),
    Identifier(String),

    // Binary operations
    Add(Box<Expression>, Box<Expression>),
    Subtract(Box<Expression>, Box<Expression>),
    Multiply(Box<Expression>, Box<Expression>),
    Divide(Box<Expression>, Box<Expression>),
    Modulo(Box<Expression>, Box<Expression>),
    Power(Box<Expression>, Box<Expression>),

    // Comparison operations
    Equal(Box<Expression>, Box<Expression>),
    NotEqual(Box<Expression>, Box<Expression>),
    Greater(Box<Expression>, Box<Expression>),
    Less(Box<Expression>, Box<Expression>),
    GreaterEqual(Box<Expression>, Box<Expression>),
    LessEqual(Box<Expression>, Box<Expression>),

    // Logical operations
    And(Box<Expression>, Box<Expression>),
    Or(Box<Expression>, Box<Expression>),
    Not(Box<Expression>),

    // Conditional
    If {
        condition: Box<Expression>,
        then_expr: Box<Expression>,
        else_expr: Box<Expression>,
    },

    // Function call
    FunctionCall {
        name: String,
        args: Vec<Expression>,
    },

    // Factor model expressions
    /// Residualize a raw expression against upstream factors.
    /// residual(raw_expr, X1, X2, ...) → orthogonalized value
    Residual {
        raw: Box<Expression>,
        upstream: Vec<String>,
    },

    /// Learnable parameter with Gaussian prior.
    /// learnable(initial_value, sigma)
    ///
    /// The optional `name` field is assigned by `Executor::assign_learnable_names`
    /// post-parse — a deterministic, position-based identifier (e.g.
    /// `tournament_strength_l0`) used as the key BayesOps reads/writes via
    /// the workspace_outputs API. When `name` is Some, the evaluator looks
    /// up `params.<name>` before falling back to `initial`.
    ///
    /// The parser always emits `name: None`; auto-naming is a pre-execution
    /// pass so that `learnable(...)` literals in different statements get
    /// stable identifiers without changing FPL syntax. Later phases may add
    /// an explicit `learnable[name](initial, sigma)` syntax.
    LearnablePrior {
        initial: f64,
        sigma: f64,
        name: Option<String>,
    },

    /// Parameter reference: param.field_name
    ParamRef(String),

    /// Factor reference: X1, X2, etc. — resolved at runtime
    FactorRef(String),

    /// Exponential function: exp(expr)
    Exp(Box<Expression>),
}

// ═══════════════════════════════════════════════════════════════════
// Program helpers — accessors, mutators, builders for the console
// ═══════════════════════════════════════════════════════════════════

impl Program {
    /// Create an empty program.
    pub fn empty() -> Self {
        Self {
            statements: Vec::new(),
        }
    }

    /// Create a program with just a question.
    pub fn with_question(text: &str) -> Self {
        Self {
            statements: vec![Statement::Question(QuestionStmt {
                text: text.to_string(),
                base_rate: None,
                target_date: None,
                resolution_criteria: None,
            })],
        }
    }

    // ── Read accessors ────────────────────────────────────────────

    pub fn question(&self) -> Option<&QuestionStmt> {
        self.statements.iter().find_map(|s| match s {
            Statement::Question(q) => Some(q),
            _ => None,
        })
    }

    pub fn drivers(&self) -> Vec<&DriverStmt> {
        self.statements
            .iter()
            .filter_map(|s| match s {
                Statement::Driver(d) => Some(d),
                _ => None,
            })
            .collect()
    }

    pub fn driver(&self, name: &str) -> Option<&DriverStmt> {
        self.drivers().into_iter().find(|d| d.name == name)
    }

    pub fn evidence_items(&self) -> Vec<&EvidenceStmt> {
        self.statements
            .iter()
            .filter_map(|s| match s {
                Statement::Evidence(e) => Some(e),
                _ => None,
            })
            .collect()
    }

    pub fn agents(&self) -> Vec<&AgentStmt> {
        self.statements
            .iter()
            .filter_map(|s| match s {
                Statement::Agent(a) => Some(a),
                _ => None,
            })
            .collect()
    }

    pub fn agent(&self, name: &str) -> Option<&AgentStmt> {
        self.agents().into_iter().find(|a| a.name == name)
    }

    pub fn model(&self) -> Option<&ModelStmt> {
        self.statements.iter().find_map(|s| match s {
            Statement::Model(m) => Some(m),
            _ => None,
        })
    }

    pub fn simulate(&self) -> Option<&SimulateStmt> {
        self.statements.iter().find_map(|s| match s {
            Statement::Simulate(s) => Some(s),
            _ => None,
        })
    }

    pub fn factors(&self) -> Vec<&FactorStmt> {
        self.statements
            .iter()
            .filter_map(|s| match s {
                Statement::Factor(f) => Some(f),
                _ => None,
            })
            .collect()
    }

    pub fn params(&self) -> Vec<&ParamDecl> {
        self.statements
            .iter()
            .filter_map(|s| match s {
                Statement::Param(p) => Some(p),
                _ => None,
            })
            .collect()
    }

    pub fn imports(&self) -> Vec<&ImportStmt> {
        self.statements
            .iter()
            .filter_map(|s| match s {
                Statement::Import(i) => Some(i),
                _ => None,
            })
            .collect()
    }

    pub fn estimates(&self) -> Vec<&EstimateStmt> {
        self.statements
            .iter()
            .filter_map(|s| match s {
                Statement::Estimate(e) => Some(e),
                _ => None,
            })
            .collect()
    }

    pub fn add_factor(&mut self, factor: FactorStmt) {
        self.statements.push(Statement::Factor(factor));
    }

    pub fn add_param(&mut self, param: ParamDecl) {
        self.statements.push(Statement::Param(param));
    }

    // ── Mutable accessors ─────────────────────────────────────────

    pub fn question_mut(&mut self) -> Option<&mut QuestionStmt> {
        self.statements.iter_mut().find_map(|s| match s {
            Statement::Question(q) => Some(q),
            _ => None,
        })
    }

    pub fn driver_mut(&mut self, name: &str) -> Option<&mut DriverStmt> {
        self.statements.iter_mut().find_map(|s| match s {
            Statement::Driver(d) if d.name == name => Some(d),
            _ => None,
        })
    }

    pub fn agent_mut(&mut self, name: &str) -> Option<&mut AgentStmt> {
        self.statements.iter_mut().find_map(|s| match s {
            Statement::Agent(a) if a.name == name => Some(a),
            _ => None,
        })
    }

    // ── Builders / mutators ───────────────────────────────────────

    pub fn set_question(&mut self, question: QuestionStmt) {
        if let Some(pos) = self
            .statements
            .iter()
            .position(|s| matches!(s, Statement::Question(_)))
        {
            self.statements[pos] = Statement::Question(question);
        } else {
            self.statements.insert(0, Statement::Question(question));
        }
    }

    pub fn add_driver(&mut self, driver: DriverStmt) {
        if let Some(pos) = self
            .statements
            .iter()
            .position(|s| matches!(s, Statement::Driver(d) if d.name == driver.name))
        {
            self.statements[pos] = Statement::Driver(driver);
        } else {
            let insert_pos = self
                .statements
                .iter()
                .rposition(|s| matches!(s, Statement::Driver(_)))
                .map(|p| p + 1)
                .or_else(|| {
                    self.statements
                        .iter()
                        .rposition(|s| matches!(s, Statement::Question(_)))
                        .map(|p| p + 1)
                })
                .unwrap_or(self.statements.len());
            self.statements
                .insert(insert_pos, Statement::Driver(driver));
        }
    }

    pub fn remove_driver(&mut self, name: &str) -> bool {
        let before = self.statements.len();
        self.statements
            .retain(|s| !matches!(s, Statement::Driver(d) if d.name == name));
        self.statements.len() < before
    }

    pub fn add_evidence(&mut self, evidence: EvidenceStmt) {
        if let Some(pos) = self
            .statements
            .iter()
            .position(|s| matches!(s, Statement::Evidence(e) if e.id == evidence.id))
        {
            self.statements[pos] = Statement::Evidence(evidence);
        } else {
            let insert_pos = self
                .statements
                .iter()
                .rposition(|s| matches!(s, Statement::Evidence(_) | Statement::Driver(_)))
                .map(|p| p + 1)
                .unwrap_or(self.statements.len());
            self.statements
                .insert(insert_pos, Statement::Evidence(evidence));
        }
    }

    pub fn add_agent(&mut self, agent: AgentStmt) {
        if let Some(pos) = self
            .statements
            .iter()
            .position(|s| matches!(s, Statement::Agent(a) if a.name == agent.name))
        {
            self.statements[pos] = Statement::Agent(agent);
        } else {
            let insert_pos = self
                .statements
                .iter()
                .rposition(|s| {
                    matches!(
                        s,
                        Statement::Agent(_) | Statement::Evidence(_) | Statement::Driver(_)
                    )
                })
                .map(|p| p + 1)
                .unwrap_or(self.statements.len());
            self.statements.insert(insert_pos, Statement::Agent(agent));
        }
    }

    pub fn set_model(&mut self, model: ModelStmt) {
        if let Some(pos) = self
            .statements
            .iter()
            .position(|s| matches!(s, Statement::Model(_)))
        {
            self.statements[pos] = Statement::Model(model);
        } else {
            let insert_pos = self
                .statements
                .iter()
                .rposition(|s| !matches!(s, Statement::Simulate(_)))
                .map(|p| p + 1)
                .unwrap_or(self.statements.len());
            self.statements.insert(insert_pos, Statement::Model(model));
        }
    }

    pub fn set_simulate(&mut self, sim: SimulateStmt) {
        if let Some(pos) = self
            .statements
            .iter()
            .position(|s| matches!(s, Statement::Simulate(_)))
        {
            self.statements[pos] = Statement::Simulate(sim);
        } else {
            self.statements.push(Statement::Simulate(sim));
        }
    }
}

impl fmt::Display for Program {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Program({} statements)", self.statements.len())
    }
}

impl fmt::Display for Statement {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Statement::Question(q) => write!(f, "Question(\"{}\")", q.text),
            Statement::Driver(d) => write!(f, "Driver({})", d.name),
            Statement::Evidence(e) => write!(f, "Evidence({})", e.id),
            Statement::Agent(a) => write!(f, "Agent({})", a.name),
            Statement::Model(_m) => write!(f, "Model"),
            Statement::Simulate(s) => write!(f, "Simulate({} iterations)", s.iterations),
            Statement::Factor(fac) => write!(f, "Factor({} \"{}\")", fac.name, fac.label),
            Statement::Param(p) => write!(f, "Param({})", p.name),
            Statement::Import(i) => write!(f, "Import({})", i.factor_name),
            Statement::Estimate(e) => write!(f, "Estimate({})", e.name),
            Statement::Output(o) => write!(f, "Output({})", o.name),
        }
    }
}

impl fmt::Display for Distribution {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Distribution::Triangular { .. } => write!(f, "Triangular"),
            Distribution::Normal { .. } => write!(f, "Normal"),
            Distribution::Lognormal { .. } => write!(f, "Lognormal"),
            Distribution::Uniform { .. } => write!(f, "Uniform"),
            Distribution::Beta { .. } => write!(f, "Beta"),
        }
    }
}

impl fmt::Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Expression::Number(n) => write!(f, "{}", n),
            Expression::Probability(p) => write!(f, "{}p", p),
            Expression::String(s) => write!(f, "\"{}\"", s),
            Expression::Boolean(b) => write!(f, "{}", b),
            Expression::Identifier(id) => write!(f, "{}", id),
            Expression::Add(l, r) => write!(f, "({} + {})", l, r),
            Expression::Subtract(l, r) => write!(f, "({} - {})", l, r),
            Expression::Multiply(l, r) => write!(f, "({} * {})", l, r),
            Expression::Divide(l, r) => write!(f, "({} / {})", l, r),
            Expression::Modulo(l, r) => write!(f, "({} % {})", l, r),
            Expression::Power(l, r) => write!(f, "({} ^ {})", l, r),
            Expression::Equal(l, r) => write!(f, "({} == {})", l, r),
            Expression::NotEqual(l, r) => write!(f, "({} != {})", l, r),
            Expression::Greater(l, r) => write!(f, "({} > {})", l, r),
            Expression::Less(l, r) => write!(f, "({} < {})", l, r),
            Expression::GreaterEqual(l, r) => write!(f, "({} >= {})", l, r),
            Expression::LessEqual(l, r) => write!(f, "({} <= {})", l, r),
            Expression::And(l, r) => write!(f, "({} and {})", l, r),
            Expression::Or(l, r) => write!(f, "({} or {})", l, r),
            Expression::Not(e) => write!(f, "(not {})", e),
            Expression::If {
                condition,
                then_expr,
                else_expr,
            } => {
                write!(
                    f,
                    "(if {} then {} else {})",
                    condition, then_expr, else_expr
                )
            }
            Expression::FunctionCall { name, args } => {
                write!(f, "{}(", name)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg)?;
                }
                write!(f, ")")
            }
            Expression::Residual { raw, upstream } => {
                write!(f, "residual({}, {})", raw, upstream.join(", "))
            }
            Expression::LearnablePrior {
                initial,
                sigma,
                name,
            } => match name {
                Some(n) => write!(f, "learnable[{}]({}, {})", n, initial, sigma),
                None => write!(f, "learnable({}, {})", initial, sigma),
            },
            Expression::ParamRef(field) => write!(f, "param:{}", field),
            Expression::FactorRef(name) => write!(f, "{}", name),
            Expression::Exp(inner) => write!(f, "exp({})", inner),
        }
    }
}

// Helper methods for building expressions
impl Expression {
    pub fn add(left: Expression, right: Expression) -> Expression {
        Expression::Add(Box::new(left), Box::new(right))
    }

    pub fn subtract(left: Expression, right: Expression) -> Expression {
        Expression::Subtract(Box::new(left), Box::new(right))
    }

    pub fn multiply(left: Expression, right: Expression) -> Expression {
        Expression::Multiply(Box::new(left), Box::new(right))
    }

    pub fn divide(left: Expression, right: Expression) -> Expression {
        Expression::Divide(Box::new(left), Box::new(right))
    }

    pub fn power(left: Expression, right: Expression) -> Expression {
        Expression::Power(Box::new(left), Box::new(right))
    }

    pub fn equal(left: Expression, right: Expression) -> Expression {
        Expression::Equal(Box::new(left), Box::new(right))
    }

    pub fn greater(left: Expression, right: Expression) -> Expression {
        Expression::Greater(Box::new(left), Box::new(right))
    }

    pub fn if_then_else(
        condition: Expression,
        then_expr: Expression,
        else_expr: Expression,
    ) -> Expression {
        Expression::If {
            condition: Box::new(condition),
            then_expr: Box::new(then_expr),
            else_expr: Box::new(else_expr),
        }
    }

    pub fn call(name: String, args: Vec<Expression>) -> Expression {
        Expression::FunctionCall { name, args }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expression_builders() {
        let expr = Expression::add(Expression::Number(5.0), Expression::Number(3.0));
        assert_eq!(expr.to_string(), "(5 + 3)");

        let expr2 = Expression::multiply(
            Expression::Identifier("x".to_string()),
            Expression::Number(2.0),
        );
        assert_eq!(expr2.to_string(), "(x * 2)");
    }

    #[test]
    fn test_if_expression() {
        let expr = Expression::if_then_else(
            Expression::Boolean(true),
            Expression::Number(1.0),
            Expression::Number(0.0),
        );
        assert_eq!(expr.to_string(), "(if true then 1 else 0)");
    }

    #[test]
    fn test_function_call() {
        let expr = Expression::call(
            "triangular".to_string(),
            vec![
                Expression::Number(500.0),
                Expression::Number(1200.0),
                Expression::Number(2500.0),
            ],
        );
        assert_eq!(expr.to_string(), "triangular(500, 1200, 2500)");
    }
}

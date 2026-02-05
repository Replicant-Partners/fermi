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

/// Driver statement: defines a forecasting driver
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
    pub rationale: Option<String>,
    pub constraints: Vec<Constraint>,
    pub evidence_refs: Vec<String>,
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
#[derive(Debug, Clone, PartialEq)]
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
            Statement::Model(m) => write!(f, "Model"),
            Statement::Simulate(s) => write!(f, "Simulate({} iterations)", s.iterations),
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

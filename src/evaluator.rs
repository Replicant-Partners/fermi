/// Expression evaluator for Monte Carlo simulation
///
/// This module evaluates FPL expressions during simulation, looking up
/// driver values from the current simulation context and computing results.
use crate::ast::Expression;
use std::collections::HashMap;

/// Evaluation context holding driver values for one simulation iteration
#[derive(Debug, Clone)]
pub struct EvaluationContext {
    /// Driver name -> sampled value for this iteration
    values: HashMap<String, f64>,
}

impl EvaluationContext {
    /// Create a new empty evaluation context
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
        }
    }

    /// Set a driver value for this iteration
    pub fn set(&mut self, name: String, value: f64) {
        self.values.insert(name, value);
    }

    /// Get a driver value by name
    pub fn get(&self, name: &str) -> Option<f64> {
        self.values.get(name).copied()
    }

    /// Check if a driver exists in the context
    pub fn has(&self, name: &str) -> bool {
        self.values.contains_key(name)
    }
}

/// Evaluation error
#[derive(Debug, Clone)]
pub enum EvalError {
    /// Variable not found in context
    UndefinedVariable(String),

    /// Division by zero
    DivisionByZero,

    /// Invalid operation (e.g., negative number to non-integer power)
    InvalidOperation(String),

    /// Type error (shouldn't happen after semantic analysis)
    TypeError(String),
}

impl std::fmt::Display for EvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            EvalError::UndefinedVariable(name) => {
                write!(f, "Undefined variable: {}", name)
            }
            EvalError::DivisionByZero => {
                write!(f, "Division by zero")
            }
            EvalError::InvalidOperation(msg) => {
                write!(f, "Invalid operation: {}", msg)
            }
            EvalError::TypeError(msg) => {
                write!(f, "Type error: {}", msg)
            }
        }
    }
}

impl std::error::Error for EvalError {}

pub type EvalResult<T> = Result<T, EvalError>;

/// Evaluate an expression in the given context
///
/// This function recursively evaluates an FPL expression, looking up
/// driver values from the context and computing the result.
///
/// # Examples
///
/// ```
/// use fermi::ast::Expression;
/// use fermi::evaluator::{evaluate, EvaluationContext};
///
/// let mut ctx = EvaluationContext::new();
/// ctx.set("market_size".to_string(), 1200.0);
/// ctx.set("growth_rate".to_string(), 0.25);
///
/// // Evaluate: market_size * (1 + growth_rate)
/// let expr = Expression::Multiply(
///     Box::new(Expression::Identifier("market_size".to_string())),
///     Box::new(Expression::Add(
///         Box::new(Expression::Number(1.0)),
///         Box::new(Expression::Identifier("growth_rate".to_string())),
///     )),
/// );
///
/// let result = evaluate(&expr, &ctx).unwrap();
/// assert_eq!(result, 1500.0); // 1200 * 1.25
/// ```
pub fn evaluate(expr: &Expression, ctx: &EvaluationContext) -> EvalResult<f64> {
    match expr {
        // Literals
        Expression::Number(n) => Ok(*n),
        Expression::Probability(p) => Ok(*p),
        Expression::Boolean(b) => Ok(if *b { 1.0 } else { 0.0 }),

        // Variable lookup
        Expression::Identifier(name) => ctx
            .get(name)
            .ok_or_else(|| EvalError::UndefinedVariable(name.clone())),

        // Arithmetic operators
        Expression::Add(left, right) => {
            let l = evaluate(left, ctx)?;
            let r = evaluate(right, ctx)?;
            Ok(l + r)
        }

        Expression::Subtract(left, right) => {
            let l = evaluate(left, ctx)?;
            let r = evaluate(right, ctx)?;
            Ok(l - r)
        }

        Expression::Multiply(left, right) => {
            let l = evaluate(left, ctx)?;
            let r = evaluate(right, ctx)?;
            Ok(l * r)
        }

        Expression::Divide(left, right) => {
            let l = evaluate(left, ctx)?;
            let r = evaluate(right, ctx)?;
            if r == 0.0 {
                Err(EvalError::DivisionByZero)
            } else {
                Ok(l / r)
            }
        }

        Expression::Modulo(left, right) => {
            let l = evaluate(left, ctx)?;
            let r = evaluate(right, ctx)?;
            if r == 0.0 {
                Err(EvalError::DivisionByZero)
            } else {
                Ok(l % r)
            }
        }

        Expression::Power(base, exp) => {
            let b = evaluate(base, ctx)?;
            let e = evaluate(exp, ctx)?;

            // Check for invalid operations
            if b < 0.0 && e.fract() != 0.0 {
                Err(EvalError::InvalidOperation(format!(
                    "Cannot raise negative number {} to non-integer power {}",
                    b, e
                )))
            } else {
                Ok(b.powf(e))
            }
        }

        // Unary operators
        Expression::Not(operand) => {
            let val = evaluate(operand, ctx)?;
            Ok(if val == 0.0 { 1.0 } else { 0.0 })
        }

        // Comparison operators (return 1.0 for true, 0.0 for false)
        Expression::Greater(left, right) => {
            let l = evaluate(left, ctx)?;
            let r = evaluate(right, ctx)?;
            Ok(if l > r { 1.0 } else { 0.0 })
        }

        Expression::GreaterEqual(left, right) => {
            let l = evaluate(left, ctx)?;
            let r = evaluate(right, ctx)?;
            Ok(if l >= r { 1.0 } else { 0.0 })
        }

        Expression::Less(left, right) => {
            let l = evaluate(left, ctx)?;
            let r = evaluate(right, ctx)?;
            Ok(if l < r { 1.0 } else { 0.0 })
        }

        Expression::LessEqual(left, right) => {
            let l = evaluate(left, ctx)?;
            let r = evaluate(right, ctx)?;
            Ok(if l <= r { 1.0 } else { 0.0 })
        }

        Expression::Equal(left, right) => {
            let l = evaluate(left, ctx)?;
            let r = evaluate(right, ctx)?;
            Ok(if (l - r).abs() < f64::EPSILON {
                1.0
            } else {
                0.0
            })
        }

        Expression::NotEqual(left, right) => {
            let l = evaluate(left, ctx)?;
            let r = evaluate(right, ctx)?;
            Ok(if (l - r).abs() >= f64::EPSILON {
                1.0
            } else {
                0.0
            })
        }

        // Logical operators
        Expression::And(left, right) => {
            let l = evaluate(left, ctx)?;
            let r = evaluate(right, ctx)?;
            Ok(if l != 0.0 && r != 0.0 { 1.0 } else { 0.0 })
        }

        Expression::Or(left, right) => {
            let l = evaluate(left, ctx)?;
            let r = evaluate(right, ctx)?;
            Ok(if l != 0.0 || r != 0.0 { 1.0 } else { 0.0 })
        }

        // Conditional expression
        Expression::If {
            condition,
            then_expr,
            else_expr,
        } => {
            let cond = evaluate(condition, ctx)?;
            if cond != 0.0 {
                evaluate(then_expr, ctx)
            } else {
                evaluate(else_expr, ctx)
            }
        }

        // Function calls (built-in functions)
        Expression::FunctionCall { name, args } => evaluate_function(name, args, ctx),

        // Factor model expressions
        //
        // LearnablePrior: if the prior has been assigned a name (by
        // Executor::assign_learnable_names) AND the evaluation context binds
        // a value for that name, use the bound value. Otherwise fall back to
        // the prior's `initial`. This is the read-side of the BayesOps
        // contract: it writes updated values into the workspace's
        // .app/params.json under the learnable's auto-assigned name, and
        // the executor binds those into the context at sim time.
        Expression::LearnablePrior { initial, name, .. } => {
            if let Some(n) = name {
                if let Some(v) = ctx.get(n) {
                    return Ok(v);
                }
            }
            Ok(*initial)
        }
        Expression::ParamRef(field) => ctx
            .get(field)
            .ok_or_else(|| EvalError::UndefinedVariable(format!("param:{}", field))),
        Expression::FactorRef(name) => ctx
            .get(name)
            .ok_or_else(|| EvalError::UndefinedVariable(format!("factor:{}", name))),
        Expression::Exp(inner) => {
            let val = evaluate(inner, ctx)?;
            Ok(val.exp())
        }
        Expression::Residual { raw, .. } => {
            // During Monte Carlo evaluation, residualization has already been applied
            // as a pre-processing step. Here we just evaluate the raw expression.
            evaluate(raw, ctx)
        }

        // These should not appear in model expressions
        Expression::String(_) => Err(EvalError::TypeError(
            "String and Date literals cannot be used in model expressions".to_string(),
        )),
    }
}

/// Evaluate a built-in function call
fn evaluate_function(name: &str, args: &[Expression], ctx: &EvaluationContext) -> EvalResult<f64> {
    match name {
        "min" => {
            if args.is_empty() {
                return Err(EvalError::InvalidOperation(
                    "min() requires at least 1 argument".to_string(),
                ));
            }
            let values: Result<Vec<_>, _> = args.iter().map(|arg| evaluate(arg, ctx)).collect();
            let values = values?;
            Ok(values.into_iter().fold(f64::INFINITY, f64::min))
        }

        "max" => {
            if args.is_empty() {
                return Err(EvalError::InvalidOperation(
                    "max() requires at least 1 argument".to_string(),
                ));
            }
            let values: Result<Vec<_>, _> = args.iter().map(|arg| evaluate(arg, ctx)).collect();
            let values = values?;
            Ok(values.into_iter().fold(f64::NEG_INFINITY, f64::max))
        }

        "abs" => {
            if args.len() != 1 {
                return Err(EvalError::InvalidOperation(
                    "abs() requires exactly 1 argument".to_string(),
                ));
            }
            let val = evaluate(&args[0], ctx)?;
            Ok(val.abs())
        }

        "sqrt" => {
            if args.len() != 1 {
                return Err(EvalError::InvalidOperation(
                    "sqrt() requires exactly 1 argument".to_string(),
                ));
            }
            let val = evaluate(&args[0], ctx)?;
            if val < 0.0 {
                Err(EvalError::InvalidOperation(
                    "sqrt() of negative number".to_string(),
                ))
            } else {
                Ok(val.sqrt())
            }
        }

        "log" => {
            if args.len() != 1 {
                return Err(EvalError::InvalidOperation(
                    "log() requires exactly 1 argument".to_string(),
                ));
            }
            let val = evaluate(&args[0], ctx)?;
            if val <= 0.0 {
                Err(EvalError::InvalidOperation(
                    "log() of non-positive number".to_string(),
                ))
            } else {
                Ok(val.ln())
            }
        }

        "exp" => {
            if args.len() != 1 {
                return Err(EvalError::InvalidOperation(
                    "exp() requires exactly 1 argument".to_string(),
                ));
            }
            let val = evaluate(&args[0], ctx)?;
            Ok(val.exp())
        }

        "round" => {
            if args.len() != 1 {
                return Err(EvalError::InvalidOperation(
                    "round() requires exactly 1 argument".to_string(),
                ));
            }
            let val = evaluate(&args[0], ctx)?;
            Ok(val.round())
        }

        "floor" => {
            if args.len() != 1 {
                return Err(EvalError::InvalidOperation(
                    "floor() requires exactly 1 argument".to_string(),
                ));
            }
            let val = evaluate(&args[0], ctx)?;
            Ok(val.floor())
        }

        "ceil" => {
            if args.len() != 1 {
                return Err(EvalError::InvalidOperation(
                    "ceil() requires exactly 1 argument".to_string(),
                ));
            }
            let val = evaluate(&args[0], ctx)?;
            Ok(val.ceil())
        }

        _ => Err(EvalError::InvalidOperation(format!(
            "Unknown function: {}",
            name
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_literals() {
        let ctx = EvaluationContext::new();

        assert_eq!(evaluate(&Expression::Number(42.5), &ctx).unwrap(), 42.5);
        assert_eq!(
            evaluate(&Expression::Probability(0.75), &ctx).unwrap(),
            0.75
        );
        assert_eq!(evaluate(&Expression::Boolean(true), &ctx).unwrap(), 1.0);
        assert_eq!(evaluate(&Expression::Boolean(false), &ctx).unwrap(), 0.0);
    }

    #[test]
    fn test_identifier() {
        let mut ctx = EvaluationContext::new();
        ctx.set("x".to_string(), 100.0);

        assert_eq!(
            evaluate(&Expression::Identifier("x".to_string()), &ctx).unwrap(),
            100.0
        );

        let result = evaluate(&Expression::Identifier("y".to_string()), &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_arithmetic() {
        let mut ctx = EvaluationContext::new();
        ctx.set("a".to_string(), 10.0);
        ctx.set("b".to_string(), 3.0);

        // a + b = 13
        let expr = Expression::Add(
            Box::new(Expression::Identifier("a".to_string())),
            Box::new(Expression::Identifier("b".to_string())),
        );
        assert_eq!(evaluate(&expr, &ctx).unwrap(), 13.0);

        // a - b = 7
        let expr = Expression::Subtract(
            Box::new(Expression::Identifier("a".to_string())),
            Box::new(Expression::Identifier("b".to_string())),
        );
        assert_eq!(evaluate(&expr, &ctx).unwrap(), 7.0);

        // a * b = 30
        let expr = Expression::Multiply(
            Box::new(Expression::Identifier("a".to_string())),
            Box::new(Expression::Identifier("b".to_string())),
        );
        assert_eq!(evaluate(&expr, &ctx).unwrap(), 30.0);

        // a / b = 3.333...
        let expr = Expression::Divide(
            Box::new(Expression::Identifier("a".to_string())),
            Box::new(Expression::Identifier("b".to_string())),
        );
        assert!((evaluate(&expr, &ctx).unwrap() - 3.333).abs() < 0.001);

        // a % b = 1
        let expr = Expression::Modulo(
            Box::new(Expression::Identifier("a".to_string())),
            Box::new(Expression::Identifier("b".to_string())),
        );
        assert_eq!(evaluate(&expr, &ctx).unwrap(), 1.0);

        // a ^ b = 1000
        let expr = Expression::Power(
            Box::new(Expression::Identifier("a".to_string())),
            Box::new(Expression::Identifier("b".to_string())),
        );
        assert_eq!(evaluate(&expr, &ctx).unwrap(), 1000.0);
    }

    #[test]
    fn test_division_by_zero() {
        let ctx = EvaluationContext::new();

        let expr = Expression::Divide(
            Box::new(Expression::Number(10.0)),
            Box::new(Expression::Number(0.0)),
        );

        assert!(matches!(
            evaluate(&expr, &ctx),
            Err(EvalError::DivisionByZero)
        ));
    }

    #[test]
    fn test_unary() {
        let ctx = EvaluationContext::new();

        // -5 = -5 (using subtraction: 0 - 5)
        let expr = Expression::Subtract(
            Box::new(Expression::Number(0.0)),
            Box::new(Expression::Number(5.0)),
        );
        assert_eq!(evaluate(&expr, &ctx).unwrap(), -5.0);

        // not true = false
        let expr = Expression::Not(Box::new(Expression::Boolean(true)));
        assert_eq!(evaluate(&expr, &ctx).unwrap(), 0.0);

        // not false = true
        let expr = Expression::Not(Box::new(Expression::Boolean(false)));
        assert_eq!(evaluate(&expr, &ctx).unwrap(), 1.0);
    }

    #[test]
    fn test_comparison() {
        let ctx = EvaluationContext::new();

        // 5 > 3 = true
        let expr = Expression::Greater(
            Box::new(Expression::Number(5.0)),
            Box::new(Expression::Number(3.0)),
        );
        assert_eq!(evaluate(&expr, &ctx).unwrap(), 1.0);

        // 3 > 5 = false
        let expr = Expression::Greater(
            Box::new(Expression::Number(3.0)),
            Box::new(Expression::Number(5.0)),
        );
        assert_eq!(evaluate(&expr, &ctx).unwrap(), 0.0);

        // 5 >= 5 = true
        let expr = Expression::GreaterEqual(
            Box::new(Expression::Number(5.0)),
            Box::new(Expression::Number(5.0)),
        );
        assert_eq!(evaluate(&expr, &ctx).unwrap(), 1.0);

        // 5 == 5 = true
        let expr = Expression::Equal(
            Box::new(Expression::Number(5.0)),
            Box::new(Expression::Number(5.0)),
        );
        assert_eq!(evaluate(&expr, &ctx).unwrap(), 1.0);

        // 5 != 3 = true
        let expr = Expression::NotEqual(
            Box::new(Expression::Number(5.0)),
            Box::new(Expression::Number(3.0)),
        );
        assert_eq!(evaluate(&expr, &ctx).unwrap(), 1.0);
    }

    #[test]
    fn test_logical() {
        let ctx = EvaluationContext::new();

        // true and true = true
        let expr = Expression::And(
            Box::new(Expression::Boolean(true)),
            Box::new(Expression::Boolean(true)),
        );
        assert_eq!(evaluate(&expr, &ctx).unwrap(), 1.0);

        // true and false = false
        let expr = Expression::And(
            Box::new(Expression::Boolean(true)),
            Box::new(Expression::Boolean(false)),
        );
        assert_eq!(evaluate(&expr, &ctx).unwrap(), 0.0);

        // true or false = true
        let expr = Expression::Or(
            Box::new(Expression::Boolean(true)),
            Box::new(Expression::Boolean(false)),
        );
        assert_eq!(evaluate(&expr, &ctx).unwrap(), 1.0);

        // false or false = false
        let expr = Expression::Or(
            Box::new(Expression::Boolean(false)),
            Box::new(Expression::Boolean(false)),
        );
        assert_eq!(evaluate(&expr, &ctx).unwrap(), 0.0);
    }

    #[test]
    fn test_conditional() {
        let mut ctx = EvaluationContext::new();
        ctx.set("x".to_string(), 10.0);

        // if x > 5 then 100 else 200 = 100
        let expr = Expression::If {
            condition: Box::new(Expression::Greater(
                Box::new(Expression::Identifier("x".to_string())),
                Box::new(Expression::Number(5.0)),
            )),
            then_expr: Box::new(Expression::Number(100.0)),
            else_expr: Box::new(Expression::Number(200.0)),
        };
        assert_eq!(evaluate(&expr, &ctx).unwrap(), 100.0);

        // if x > 20 then 100 else 200 = 200
        let expr = Expression::If {
            condition: Box::new(Expression::Greater(
                Box::new(Expression::Identifier("x".to_string())),
                Box::new(Expression::Number(20.0)),
            )),
            then_expr: Box::new(Expression::Number(100.0)),
            else_expr: Box::new(Expression::Number(200.0)),
        };
        assert_eq!(evaluate(&expr, &ctx).unwrap(), 200.0);
    }

    #[test]
    fn test_complex_expression() {
        let mut ctx = EvaluationContext::new();
        ctx.set("market_size".to_string(), 1200.0);
        ctx.set("growth_rate".to_string(), 0.25);
        ctx.set("major_contract".to_string(), 1.0); // true

        // market_size * (1 + growth_rate) * (if major_contract then 1.5 else 1.0)
        let expr = Expression::Multiply(
            Box::new(Expression::Multiply(
                Box::new(Expression::Identifier("market_size".to_string())),
                Box::new(Expression::Add(
                    Box::new(Expression::Number(1.0)),
                    Box::new(Expression::Identifier("growth_rate".to_string())),
                )),
            )),
            Box::new(Expression::If {
                condition: Box::new(Expression::Identifier("major_contract".to_string())),
                then_expr: Box::new(Expression::Number(1.5)),
                else_expr: Box::new(Expression::Number(1.0)),
            }),
        );

        // Expected: 1200 * 1.25 * 1.5 = 2250
        assert_eq!(evaluate(&expr, &ctx).unwrap(), 2250.0);
    }

    #[test]
    fn test_builtin_functions() {
        let ctx = EvaluationContext::new();

        // min(5, 3, 8) = 3
        let expr = Expression::FunctionCall {
            name: "min".to_string(),
            args: vec![
                Expression::Number(5.0),
                Expression::Number(3.0),
                Expression::Number(8.0),
            ],
        };
        assert_eq!(evaluate(&expr, &ctx).unwrap(), 3.0);

        // max(5, 3, 8) = 8
        let expr = Expression::FunctionCall {
            name: "max".to_string(),
            args: vec![
                Expression::Number(5.0),
                Expression::Number(3.0),
                Expression::Number(8.0),
            ],
        };
        assert_eq!(evaluate(&expr, &ctx).unwrap(), 8.0);

        // abs(-5) = 5
        let expr = Expression::FunctionCall {
            name: "abs".to_string(),
            args: vec![Expression::Number(-5.0)],
        };
        assert_eq!(evaluate(&expr, &ctx).unwrap(), 5.0);

        // sqrt(16) = 4
        let expr = Expression::FunctionCall {
            name: "sqrt".to_string(),
            args: vec![Expression::Number(16.0)],
        };
        assert_eq!(evaluate(&expr, &ctx).unwrap(), 4.0);

        // round(3.7) = 4
        let expr = Expression::FunctionCall {
            name: "round".to_string(),
            args: vec![Expression::Number(3.7)],
        };
        assert_eq!(evaluate(&expr, &ctx).unwrap(), 4.0);
    }
}

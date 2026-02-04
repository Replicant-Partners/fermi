/// FPL Type System
///
/// Defines the type system for FPL expressions and provides type checking utilities.

use std::fmt;

/// Types in the FPL language
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    /// Numeric type (f64)
    Number,

    /// Probability type (0.0 to 1.0)
    Probability,

    /// String type
    String,

    /// Boolean type
    Boolean,

    /// Date type (YYYY-MM-DD)
    Date,

    /// Distribution type (triangular, normal, etc.)
    Distribution,

    /// Driver type (continuous or binary)
    Driver,

    /// Unknown type (used during inference)
    Unknown,

    /// Error type (used when type checking fails)
    Error,
}

impl Type {
    /// Check if this type can be coerced to another type
    pub fn can_coerce_to(&self, other: &Type) -> bool {
        match (self, other) {
            // Same types
            (a, b) if a == b => true,

            // Number <-> Probability coercion
            (Type::Number, Type::Probability) => true,
            (Type::Probability, Type::Number) => true,

            // Unknown can coerce to anything
            (Type::Unknown, _) => true,
            (_, Type::Unknown) => true,

            // Error can coerce to anything (error propagation)
            (Type::Error, _) => true,
            (_, Type::Error) => true,

            _ => false,
        }
    }

    /// Check if this type is numeric (Number or Probability)
    pub fn is_numeric(&self) -> bool {
        matches!(self, Type::Number | Type::Probability | Type::Unknown)
    }

    /// Check if this type is comparable
    pub fn is_comparable(&self) -> bool {
        matches!(
            self,
            Type::Number | Type::Probability | Type::Date | Type::Unknown
        )
    }

    /// Check if this type supports arithmetic operations
    pub fn supports_arithmetic(&self) -> bool {
        matches!(self, Type::Number | Type::Probability | Type::Unknown)
    }

    /// Check if this type is boolean or can be used in boolean context
    pub fn is_boolean(&self) -> bool {
        matches!(self, Type::Boolean | Type::Unknown)
    }

    /// Get the result type of a binary operation
    pub fn binary_op_result(left: &Type, right: &Type, op: BinaryOp) -> Type {
        match op {
            // Arithmetic operations
            BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply | BinaryOp::Divide | BinaryOp::Modulo => {
                if left.supports_arithmetic() && right.supports_arithmetic() {
                    // If either is Probability, result is Number
                    if matches!(left, Type::Probability) || matches!(right, Type::Probability) {
                        Type::Number
                    } else {
                        Type::Number
                    }
                } else {
                    Type::Error
                }
            }

            // Power operation
            BinaryOp::Power => {
                if left.supports_arithmetic() && right.supports_arithmetic() {
                    Type::Number
                } else {
                    Type::Error
                }
            }

            // Comparison operations
            BinaryOp::Greater | BinaryOp::Less | BinaryOp::GreaterEqual | BinaryOp::LessEqual => {
                if left.is_comparable() && right.is_comparable() {
                    Type::Boolean
                } else {
                    Type::Error
                }
            }

            // Equality operations
            BinaryOp::Equal | BinaryOp::NotEqual => {
                // Can compare anything
                Type::Boolean
            }

            // Logical operations
            BinaryOp::And | BinaryOp::Or => {
                if left.is_boolean() && right.is_boolean() {
                    Type::Boolean
                } else {
                    Type::Error
                }
            }
        }
    }

    /// Get the result type of a unary operation
    pub fn unary_op_result(operand: &Type, op: UnaryOp) -> Type {
        match op {
            UnaryOp::Negate => {
                if operand.supports_arithmetic() {
                    operand.clone()
                } else {
                    Type::Error
                }
            }
            UnaryOp::Not => {
                if operand.is_boolean() {
                    Type::Boolean
                } else {
                    Type::Error
                }
            }
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Type::Number => write!(f, "Number"),
            Type::Probability => write!(f, "Probability"),
            Type::String => write!(f, "String"),
            Type::Boolean => write!(f, "Boolean"),
            Type::Date => write!(f, "Date"),
            Type::Distribution => write!(f, "Distribution"),
            Type::Driver => write!(f, "Driver"),
            Type::Unknown => write!(f, "Unknown"),
            Type::Error => write!(f, "Error"),
        }
    }
}

/// Binary operators
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Power,
    Greater,
    Less,
    GreaterEqual,
    LessEqual,
    Equal,
    NotEqual,
    And,
    Or,
}

/// Unary operators
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnaryOp {
    Negate,
    Not,
}

/// Type environment for tracking variable types
#[derive(Debug, Clone)]
pub struct TypeEnvironment {
    /// Map of variable names to types
    bindings: std::collections::HashMap<String, Type>,
}

impl TypeEnvironment {
    pub fn new() -> Self {
        TypeEnvironment {
            bindings: std::collections::HashMap::new(),
        }
    }

    /// Bind a name to a type
    pub fn bind(&mut self, name: String, ty: Type) {
        self.bindings.insert(name, ty);
    }

    /// Lookup a name's type
    pub fn lookup(&self, name: &str) -> Option<&Type> {
        self.bindings.get(name)
    }

    /// Check if a name is bound
    pub fn contains(&self, name: &str) -> bool {
        self.bindings.contains_key(name)
    }

    /// Get all bindings
    pub fn bindings(&self) -> &std::collections::HashMap<String, Type> {
        &self.bindings
    }
}

impl Default for TypeEnvironment {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_coercion() {
        assert!(Type::Number.can_coerce_to(&Type::Probability));
        assert!(Type::Probability.can_coerce_to(&Type::Number));
        assert!(!Type::String.can_coerce_to(&Type::Number));
    }

    #[test]
    fn test_numeric_types() {
        assert!(Type::Number.is_numeric());
        assert!(Type::Probability.is_numeric());
        assert!(!Type::String.is_numeric());
    }

    #[test]
    fn test_arithmetic_operations() {
        let result = Type::binary_op_result(&Type::Number, &Type::Number, BinaryOp::Add);
        assert_eq!(result, Type::Number);

        let result = Type::binary_op_result(&Type::String, &Type::Number, BinaryOp::Add);
        assert_eq!(result, Type::Error);
    }

    #[test]
    fn test_comparison_operations() {
        let result = Type::binary_op_result(&Type::Number, &Type::Number, BinaryOp::Greater);
        assert_eq!(result, Type::Boolean);
    }

    #[test]
    fn test_type_environment() {
        let mut env = TypeEnvironment::new();
        env.bind("x".to_string(), Type::Number);
        env.bind("y".to_string(), Type::Probability);

        assert_eq!(env.lookup("x"), Some(&Type::Number));
        assert_eq!(env.lookup("y"), Some(&Type::Probability));
        assert_eq!(env.lookup("z"), None);
    }
}

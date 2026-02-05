/// Symbol Table
///
/// Tracks all defined symbols (drivers, evidence, agents) in an FPL program.
use crate::ast::*;
use crate::types::Type;
use std::collections::HashMap;

/// Symbol information
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub symbol_type: SymbolType,
    pub ty: Type,
    pub defined_at: Option<usize>, // Line number
}

/// Type of symbol
#[derive(Debug, Clone, PartialEq)]
pub enum SymbolType {
    Driver,
    Evidence,
    Agent,
    Function,
}

/// Symbol table for tracking all symbols in a program
#[derive(Debug, Clone)]
pub struct SymbolTable {
    symbols: HashMap<String, Symbol>,

    // Track which drivers are used in the model
    drivers_in_model: Vec<String>,
}

impl SymbolTable {
    pub fn new() -> Self {
        SymbolTable {
            symbols: HashMap::new(),
            drivers_in_model: Vec::new(),
        }
    }

    /// Define a new symbol
    pub fn define(
        &mut self,
        name: String,
        symbol_type: SymbolType,
        ty: Type,
        line: Option<usize>,
    ) -> Result<(), String> {
        if self.symbols.contains_key(&name) {
            return Err(format!("Symbol '{}' is already defined", name));
        }

        self.symbols.insert(
            name.clone(),
            Symbol {
                name,
                symbol_type,
                ty,
                defined_at: line,
            },
        );

        Ok(())
    }

    /// Lookup a symbol
    pub fn lookup(&self, name: &str) -> Option<&Symbol> {
        self.symbols.get(name)
    }

    /// Check if a symbol exists
    pub fn contains(&self, name: &str) -> bool {
        self.symbols.contains_key(name)
    }

    /// Get all symbols
    pub fn symbols(&self) -> &HashMap<String, Symbol> {
        &self.symbols
    }

    /// Get all driver symbols
    pub fn drivers(&self) -> Vec<&Symbol> {
        self.symbols
            .values()
            .filter(|s| s.symbol_type == SymbolType::Driver)
            .collect()
    }

    /// Get all evidence symbols
    pub fn evidence(&self) -> Vec<&Symbol> {
        self.symbols
            .values()
            .filter(|s| s.symbol_type == SymbolType::Evidence)
            .collect()
    }

    /// Record that a driver is used in the model
    pub fn mark_driver_used(&mut self, driver_name: String) {
        if !self.drivers_in_model.contains(&driver_name) {
            self.drivers_in_model.push(driver_name);
        }
    }

    /// Get all drivers used in the model
    pub fn drivers_used_in_model(&self) -> &[String] {
        &self.drivers_in_model
    }

    /// Get all unused drivers
    pub fn unused_drivers(&self) -> Vec<&Symbol> {
        self.drivers()
            .into_iter()
            .filter(|s| !self.drivers_in_model.contains(&s.name))
            .collect()
    }

    /// Check if all drivers are used
    pub fn all_drivers_used(&self) -> bool {
        self.unused_drivers().is_empty()
    }
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Symbol table builder - constructs symbol table from AST
pub struct SymbolTableBuilder {
    table: SymbolTable,
    errors: Vec<String>,
}

impl SymbolTableBuilder {
    pub fn new() -> Self {
        SymbolTableBuilder {
            table: SymbolTable::new(),
            errors: Vec::new(),
        }
    }

    /// Build symbol table from a program
    pub fn build(mut self, program: &Program) -> Result<SymbolTable, Vec<String>> {
        // First pass: collect all definitions
        for stmt in &program.statements {
            match stmt {
                Statement::Driver(driver) => {
                    let ty = match driver.driver_type {
                        DriverType::Continuous => Type::Number,
                        DriverType::Binary => Type::Boolean,
                        DriverType::Discrete => Type::Number,
                    };

                    if let Err(e) =
                        self.table
                            .define(driver.name.clone(), SymbolType::Driver, ty, None)
                    {
                        self.errors.push(e);
                    }
                }
                Statement::Evidence(evidence) => {
                    if let Err(e) = self.table.define(
                        evidence.id.clone(),
                        SymbolType::Evidence,
                        Type::String, // Evidence is essentially metadata
                        None,
                    ) {
                        self.errors.push(e);
                    }
                }
                Statement::Agent(agent) => {
                    if let Err(e) = self.table.define(
                        agent.name.clone(),
                        SymbolType::Agent,
                        Type::String, // Agents produce evidence
                        None,
                    ) {
                        self.errors.push(e);
                    }
                }
                _ => {}
            }
        }

        // Second pass: find model and track driver usage
        for stmt in &program.statements {
            if let Statement::Model(model) = stmt {
                self.collect_identifiers(&model.expression);
            }
        }

        if self.errors.is_empty() {
            Ok(self.table)
        } else {
            Err(self.errors)
        }
    }

    /// Collect all identifiers from an expression (for tracking driver usage)
    fn collect_identifiers(&mut self, expr: &Expression) {
        match expr {
            Expression::Identifier(name) => {
                // Check if this is a driver
                if let Some(symbol) = self.table.lookup(name) {
                    if symbol.symbol_type == SymbolType::Driver {
                        self.table.mark_driver_used(name.clone());
                    }
                }
            }
            Expression::Add(l, r)
            | Expression::Subtract(l, r)
            | Expression::Multiply(l, r)
            | Expression::Divide(l, r)
            | Expression::Modulo(l, r)
            | Expression::Power(l, r)
            | Expression::Equal(l, r)
            | Expression::NotEqual(l, r)
            | Expression::Greater(l, r)
            | Expression::Less(l, r)
            | Expression::GreaterEqual(l, r)
            | Expression::LessEqual(l, r)
            | Expression::And(l, r)
            | Expression::Or(l, r) => {
                self.collect_identifiers(l);
                self.collect_identifiers(r);
            }
            Expression::Not(e) => {
                self.collect_identifiers(e);
            }
            Expression::If {
                condition,
                then_expr,
                else_expr,
            } => {
                self.collect_identifiers(condition);
                self.collect_identifiers(then_expr);
                self.collect_identifiers(else_expr);
            }
            Expression::FunctionCall { args, .. } => {
                for arg in args {
                    self.collect_identifiers(arg);
                }
            }
            _ => {}
        }
    }
}

impl Default for SymbolTableBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_definition() {
        let mut table = SymbolTable::new();

        assert!(table
            .define("x".to_string(), SymbolType::Driver, Type::Number, None)
            .is_ok());
        assert!(table.contains("x"));
        assert!(!table.contains("y"));
    }

    #[test]
    fn test_duplicate_definition() {
        let mut table = SymbolTable::new();

        assert!(table
            .define("x".to_string(), SymbolType::Driver, Type::Number, None)
            .is_ok());
        assert!(table
            .define("x".to_string(), SymbolType::Driver, Type::Number, None)
            .is_err());
    }

    #[test]
    fn test_driver_tracking() {
        let mut table = SymbolTable::new();

        table
            .define("x".to_string(), SymbolType::Driver, Type::Number, None)
            .unwrap();
        table
            .define("y".to_string(), SymbolType::Driver, Type::Number, None)
            .unwrap();

        table.mark_driver_used("x".to_string());

        assert_eq!(table.drivers_used_in_model().len(), 1);
        assert_eq!(table.unused_drivers().len(), 1);
        assert!(!table.all_drivers_used());
    }
}

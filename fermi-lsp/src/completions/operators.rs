use super::builder::CompletionBuilder;
use tower_lsp::lsp_types::*;

/// Get control flow keyword completions (if, then, else)
pub fn get_control_flow_completions() -> Vec<CompletionItem> {
    vec![
        CompletionBuilder::keyword("if")
            .detail("Conditional expression")
            .docs("Syntax: if condition then true_value else false_value\nExample: if revenue > 1000 then 1.2 else 1.0")
            .snippet("if ${1:condition} then ${2:true_value} else ${3:false_value}")
            .sort("00_if")
            .build(),

        CompletionBuilder::keyword("then")
            .detail("True branch of conditional")
            .docs("Value to use when condition is true")
            .sort("01_then")
            .build(),

        CompletionBuilder::keyword("else")
            .detail("False branch of conditional")
            .docs("Value to use when condition is false")
            .sort("02_else")
            .build(),
    ]
}

/// Get logical operator completions
pub fn get_logical_operator_completions() -> Vec<CompletionItem> {
    vec![
        CompletionBuilder::operator("and")
            .detail("Logical AND")
            .docs("Returns true only if both conditions are true.\nExample: price > 100 and volume > 1000")
            .sort("00_and")
            .build(),

        CompletionBuilder::operator("or")
            .detail("Logical OR")
            .docs("Returns true if either condition is true.\nExample: scenario_a or scenario_b")
            .sort("01_or")
            .build(),

        CompletionBuilder::operator("not")
            .detail("Logical NOT")
            .docs("Inverts a boolean value.\nExample: not failed")
            .sort("02_not")
            .build(),
    ]
}

/// Get arithmetic and comparison operator completions
pub fn get_arithmetic_operator_completions() -> Vec<CompletionItem> {
    vec![
        CompletionBuilder::operator("+")
            .detail("Addition")
            .sort("00_add")
            .build(),
        CompletionBuilder::operator("-")
            .detail("Subtraction")
            .sort("01_sub")
            .build(),
        CompletionBuilder::operator("*")
            .detail("Multiplication")
            .sort("02_mul")
            .build(),
        CompletionBuilder::operator("/")
            .detail("Division")
            .sort("03_div")
            .build(),
        CompletionBuilder::operator("^")
            .detail("Exponentiation")
            .sort("04_pow")
            .build(),
        CompletionBuilder::operator("%")
            .detail("Modulo")
            .sort("05_mod")
            .build(),
        CompletionBuilder::operator("==")
            .detail("Equality")
            .sort("10_eq")
            .build(),
        CompletionBuilder::operator("!=")
            .detail("Inequality")
            .sort("11_neq")
            .build(),
        CompletionBuilder::operator("<")
            .detail("Less than")
            .sort("12_lt")
            .build(),
        CompletionBuilder::operator(">")
            .detail("Greater than")
            .sort("13_gt")
            .build(),
        CompletionBuilder::operator("<=")
            .detail("Less than or equal")
            .sort("14_lte")
            .build(),
        CompletionBuilder::operator(">=")
            .detail("Greater than or equal")
            .sort("15_gte")
            .build(),
    ]
}

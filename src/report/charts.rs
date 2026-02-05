/// Mermaid chart generation
use crate::ast::*;
use crate::executor::ExecutionResults;
use crate::report::mermaid::{
    generate_chart_markdown, generate_image, is_mmdc_available, ImageFormat,
};
use std::path::Path;

/// Generate histogram using XY Chart (experimental)
pub fn generate_histogram(
    results: &ExecutionResults,
) -> Result<String, Box<dyn std::error::Error>> {
    // Create 20 bins for histogram
    let bin_count = 20;
    let range = results.max - results.min;
    let bin_width = range / bin_count as f64;

    let mut bins = vec![0; bin_count];

    // We don't have individual samples, so approximate from percentiles
    // For prototype, create a simple bar chart showing key percentiles

    let mut chart = String::from("```mermaid\n");
    chart.push_str("---\n");
    chart.push_str("config:\n");
    chart.push_str("  themeVariables:\n");
    chart.push_str("    xyChart:\n");
    chart.push_str("      plotColorPalette: \"#4CAF50, #2196F3\"\n");
    chart.push_str("---\n");
    chart.push_str("xychart-beta\n");
    chart.push_str(&format!(
        "  title \"Result Distribution (n={})\"\n",
        results.iterations
    ));
    chart.push_str(&format!(
        "  x-axis \"Value Range\" [{:.1}, {:.1}]\n",
        results.min, results.max
    ));
    chart.push_str("  y-axis \"Relative Frequency\" 0 --> 100\n");

    // Create approximate distribution using known percentiles
    // This is a rough visualization for the prototype
    let values = vec![
        results.min,
        (results.min + results.p5) / 2.0,
        results.p5,
        (results.p5 + results.p25) / 2.0,
        results.p25,
        (results.p25 + results.median) / 2.0,
        results.median,
        (results.median + results.p75) / 2.0,
        results.p75,
        (results.p75 + results.p95) / 2.0,
        results.p95,
        (results.p95 + results.max) / 2.0,
        results.max,
    ];

    // Approximate heights (bell curve-ish)
    let heights = vec![5, 15, 30, 50, 70, 85, 100, 85, 70, 50, 30, 15, 5];

    chart.push_str("  bar [");
    for (i, h) in heights.iter().enumerate() {
        if i > 0 {
            chart.push_str(", ");
        }
        chart.push_str(&h.to_string());
    }
    chart.push_str("]\n");
    chart.push_str("```\n");

    Ok(chart)
}

/// Generate mindmap of forecast structure
pub fn generate_mindmap(
    forecast: &Program,
    question: &str,
    drivers: &[DriverStmt],
) -> Result<String, Box<dyn std::error::Error>> {
    let mut chart = String::from("```mermaid\nmindmap\n");
    chart.push_str("  root((Forecast))\n");

    // Question branch
    let short_question = if question.len() > 40 {
        format!("{}...", &question[..37])
    } else {
        question.to_string()
    };
    chart.push_str(&format!("    Question\n      \"{}\"\n", short_question));

    // Drivers branch
    chart.push_str("    Drivers\n");

    // Group by type
    let continuous: Vec<_> = drivers
        .iter()
        .filter(|d| matches!(d.driver_type, DriverType::Continuous))
        .collect();
    let binary: Vec<_> = drivers
        .iter()
        .filter(|d| matches!(d.driver_type, DriverType::Binary))
        .collect();
    let discrete: Vec<_> = drivers
        .iter()
        .filter(|d| matches!(d.driver_type, DriverType::Discrete))
        .collect();

    if !continuous.is_empty() {
        chart.push_str("      Continuous\n");
        for driver in continuous {
            let name = driver.display_name.as_ref().unwrap_or(&driver.name);
            chart.push_str(&format!("        {}\n", name));
        }
    }

    if !binary.is_empty() {
        chart.push_str("      Binary\n");
        for driver in binary {
            let name = driver.display_name.as_ref().unwrap_or(&driver.name);
            chart.push_str(&format!("        {}\n", name));
        }
    }

    if !discrete.is_empty() {
        chart.push_str("      Discrete\n");
        for driver in discrete {
            let name = driver.display_name.as_ref().unwrap_or(&driver.name);
            chart.push_str(&format!("        {}\n", name));
        }
    }

    // Model branch
    chart.push_str("    Model\n");
    chart.push_str("      Expression\n");

    chart.push_str("```\n");

    Ok(chart)
}

/// Generate flowchart of model computation
pub fn generate_flowchart(
    model_expr: &Expression,
    drivers: &[DriverStmt],
) -> Result<String, Box<dyn std::error::Error>> {
    let mut chart = String::from("```mermaid\nflowchart TD\n");

    // Start node
    chart.push_str("    Start([Model Computation])\n");

    // Driver nodes
    for (i, driver) in drivers.iter().enumerate() {
        let id = format!("D{}", i);
        let name = driver.display_name.as_ref().unwrap_or(&driver.name);
        chart.push_str(&format!("    {}[\"{}\"]\n", id, name));
        chart.push_str(&format!("    Start --> {}\n", id));
    }

    // Expression node
    chart.push_str("    Expr{{Expression}}\n");
    for i in 0..drivers.len() {
        chart.push_str(&format!("    D{} --> Expr\n", i));
    }

    // Conditional check (if model has if-then-else)
    let has_conditional = expr_has_conditional(model_expr);
    if has_conditional {
        chart.push_str("    Cond{Condition?}\n");
        chart.push_str("    Expr --> Cond\n");
        chart.push_str("    Cond -->|True| Then[Then Branch]\n");
        chart.push_str("    Cond -->|False| Else[Else Branch]\n");
        chart.push_str("    Then --> Result\n");
        chart.push_str("    Else --> Result\n");
    } else {
        chart.push_str("    Expr --> Result\n");
    }

    // Result node
    chart.push_str("    Result([Final Result])\n");

    chart.push_str("```\n");

    Ok(chart)
}

/// Check if expression contains conditional
fn expr_has_conditional(expr: &Expression) -> bool {
    match expr {
        Expression::If { .. } => true,
        Expression::Add(left, right)
        | Expression::Subtract(left, right)
        | Expression::Multiply(left, right)
        | Expression::Divide(left, right)
        | Expression::Power(left, right)
        | Expression::Modulo(left, right)
        | Expression::And(left, right)
        | Expression::Or(left, right) => expr_has_conditional(left) || expr_has_conditional(right),
        Expression::Not(inner) => expr_has_conditional(inner),
        Expression::FunctionCall { args, .. } => args.iter().any(expr_has_conditional),
        _ => false,
    }
}

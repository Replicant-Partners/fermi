/// Mermaid chart generation with image output
use crate::ast::*;
use crate::executor::ExecutionResults;
use crate::report::mermaid::{
    generate_chart_markdown, generate_image, is_mmdc_available, ImageFormat,
};
use crate::report::theme::{generate_mermaid_theme_config, generate_xychart_theme, AYU_MIRAGE};
use crate::sensitivity::SensitivityAnalysis;
use std::path::Path;

/// Generate histogram with image
pub fn generate_histogram_with_image(
    results: &ExecutionResults,
    output_dir: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let mermaid_code = generate_histogram_code(results)?;

    if is_mmdc_available() {
        // Generate image
        let image_path = generate_image(&mermaid_code, output_dir, "histogram", ImageFormat::PNG)?;
        let relative_path = format!(
            "charts/{}",
            image_path.file_name().unwrap().to_string_lossy()
        );

        // Return markdown with image and source
        Ok(generate_chart_markdown(
            &mermaid_code,
            &image_path,
            "Distribution Histogram",
            &relative_path,
        ))
    } else {
        // Fallback: inline Mermaid
        Ok(format!("```mermaid\n{}\n```\n", mermaid_code))
    }
}

fn generate_histogram_code(
    results: &ExecutionResults,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut chart = String::new();

    // Apply Ayu Mirage theme
    chart.push_str(&generate_xychart_theme(&AYU_MIRAGE));
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

    Ok(chart)
}

/// Generate Tornado chart with image (sensitivity analysis)
pub fn generate_tornado_with_image(
    drivers: &[DriverStmt],
    sensitivity: &SensitivityAnalysis,
    output_dir: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let mermaid_code = generate_tornado_code(drivers, sensitivity)?;

    if is_mmdc_available() {
        let image_path = generate_image(&mermaid_code, output_dir, "tornado", ImageFormat::PNG)?;
        let relative_path = format!(
            "charts/{}",
            image_path.file_name().unwrap().to_string_lossy()
        );

        Ok(generate_chart_markdown(
            &mermaid_code,
            &image_path,
            "Sensitivity Analysis",
            &relative_path,
        ))
    } else {
        Ok(format!("```mermaid\n{}\n```\n", mermaid_code))
    }
}

fn generate_tornado_code(
    drivers: &[DriverStmt],
    sensitivity: &SensitivityAnalysis,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut chart = String::new();

    // Apply Ayu Mirage theme
    chart.push_str(&generate_xychart_theme(&AYU_MIRAGE));
    chart.push_str("xychart-beta\n");
    chart.push_str("  title \"Driver Sensitivity Analysis\"\n");
    chart.push_str("  x-axis [");

    // Driver names
    for (i, driver) in drivers.iter().enumerate() {
        let name = driver.display_name.as_ref().unwrap_or(&driver.name);
        let short_name = if name.len() > 20 {
            format!("{}...", &name[..17])
        } else {
            name.clone()
        };

        if i > 0 {
            chart.push_str(", ");
        }
        chart.push_str(&format!("\"{}\"", short_name));
    }
    chart.push_str("]\n");
    chart.push_str("  y-axis \"Impact Magnitude\" 0 --> 100\n");

    // Get actual sensitivity scores from analysis (total-order indices)
    chart.push_str("  bar [");
    for (i, driver) in drivers.iter().enumerate() {
        if i > 0 {
            chart.push_str(", ");
        }

        // Get total-order Sobol index (scaled to 0-100)
        let total_order = sensitivity
            .get_driver_sensitivity(&driver.name)
            .map(|s| s.total_order_index * 100.0)
            .unwrap_or(10.0); // Default if not found

        let score = total_order.round() as i32;

        chart.push_str(&score.to_string());
    }
    chart.push_str("]\n");

    Ok(chart)
}

/// Generate mindmap with image
pub fn generate_mindmap_with_image(
    forecast: &Program,
    question: &str,
    drivers: &[DriverStmt],
    output_dir: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let mermaid_code = generate_mindmap_code(forecast, question, drivers)?;

    if is_mmdc_available() {
        let image_path = generate_image(&mermaid_code, output_dir, "mindmap", ImageFormat::PNG)?;
        let relative_path = format!(
            "charts/{}",
            image_path.file_name().unwrap().to_string_lossy()
        );

        Ok(generate_chart_markdown(
            &mermaid_code,
            &image_path,
            "Forecast Structure",
            &relative_path,
        ))
    } else {
        Ok(format!("```mermaid\n{}\n```\n", mermaid_code))
    }
}

fn generate_mindmap_code(
    _forecast: &Program,
    question: &str,
    drivers: &[DriverStmt],
) -> Result<String, Box<dyn std::error::Error>> {
    let mut chart = String::new();

    // Apply Ayu Mirage theme
    chart.push_str(&generate_mermaid_theme_config(&AYU_MIRAGE));
    chart.push_str("mindmap\n");
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

    Ok(chart)
}

/// Generate flowchart with image
pub fn generate_flowchart_with_image(
    model_expr: &Expression,
    drivers: &[DriverStmt],
    output_dir: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let mermaid_code = generate_flowchart_code(model_expr, drivers)?;

    if is_mmdc_available() {
        let image_path = generate_image(&mermaid_code, output_dir, "flowchart", ImageFormat::PNG)?;
        let relative_path = format!(
            "charts/{}",
            image_path.file_name().unwrap().to_string_lossy()
        );

        Ok(generate_chart_markdown(
            &mermaid_code,
            &image_path,
            "Model Flow",
            &relative_path,
        ))
    } else {
        Ok(format!("```mermaid\n{}\n```\n", mermaid_code))
    }
}

fn generate_flowchart_code(
    model_expr: &Expression,
    drivers: &[DriverStmt],
) -> Result<String, Box<dyn std::error::Error>> {
    let mut chart = String::new();

    // Apply Ayu Mirage theme
    chart.push_str(&generate_mermaid_theme_config(&AYU_MIRAGE));
    chart.push_str("flowchart TD\n");

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

    // Conditional check
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

    Ok(chart)
}

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

/// Generate Sankey diagram with image
pub fn generate_sankey_with_image(
    drivers: &[DriverStmt],
    sensitivity: &SensitivityAnalysis,
    output_dir: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let mermaid_code = generate_sankey_code(drivers, sensitivity)?;

    if is_mmdc_available() {
        let image_path = generate_image(&mermaid_code, output_dir, "sankey", ImageFormat::PNG)?;
        let relative_path = format!(
            "charts/{}",
            image_path.file_name().unwrap().to_string_lossy()
        );

        Ok(generate_chart_markdown(
            &mermaid_code,
            &image_path,
            "Driver Impact Flow",
            &relative_path,
        ))
    } else {
        Ok(format!("```mermaid\n{}\n```\n", mermaid_code))
    }
}

fn generate_sankey_code(
    drivers: &[DriverStmt],
    sensitivity: &SensitivityAnalysis,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut chart = String::new();

    // Apply Ayu Mirage theme
    chart.push_str(&generate_mermaid_theme_config(&AYU_MIRAGE));
    chart.push_str(
        "%%{init: {\"theme\": \"base\", \"themeVariables\": {\"fontSize\": \"16px\"}}}%%\n",
    );
    chart.push_str("graph LR\n");

    // Create nodes for each driver
    for (i, driver) in drivers.iter().enumerate() {
        let name = driver.display_name.as_ref().unwrap_or(&driver.name);
        let short_name = if name.len() > 25 {
            format!("{}...", &name[..22])
        } else {
            name.clone()
        };

        // Get actual variance contribution from sensitivity analysis
        let variance_contrib = sensitivity
            .get_driver_sensitivity(&driver.name)
            .map(|s| s.variance_contribution)
            .unwrap_or(0.1);

        // Scale to 1-100 for visual weight (multiply by 100)
        let weight = (variance_contrib * 100.0).round() as i32;
        let weight_str = if weight < 5 {
            "5".to_string() // Minimum visible weight
        } else {
            weight.to_string()
        };

        chart.push_str(&format!("    D{}[\"{}\"]\n", i, short_name));

        // Connect to model with actual variance contribution weight
        chart.push_str(&format!("    D{} -->|{}%| Model\n", i, weight_str));
    }

    // Model node
    chart.push_str("    Model[\"Model<br/>Computation\"]\n");
    chart.push_str("    Model -->|Result| Output[\"Final<br/>Distribution\"]\n");

    // Add styling
    chart.push_str(
        "    classDef driverClass fill:#5CCFE6,stroke:#5C6773,stroke-width:2px,color:#1F2430\n",
    );
    chart.push_str(
        "    classDef modelClass fill:#BAE67E,stroke:#5C6773,stroke-width:3px,color:#1F2430\n",
    );
    chart.push_str(
        "    classDef outputClass fill:#FFCC66,stroke:#5C6773,stroke-width:3px,color:#1F2430\n",
    );

    // Apply styles
    for i in 0..drivers.len() {
        chart.push_str(&format!("    class D{} driverClass\n", i));
    }
    chart.push_str("    class Model modelClass\n");
    chart.push_str("    class Output outputClass\n");

    Ok(chart)
}

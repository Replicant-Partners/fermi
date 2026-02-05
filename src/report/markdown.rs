/// Markdown report generation
use crate::ast::*;
use crate::executor::ExecutionResults;
use crate::report::{charts, charts_image, sparkline};
use chrono::{DateTime, Utc};
use std::path::Path;

pub fn generate(
    forecast: &Program,
    results: &ExecutionResults,
    timestamp: &DateTime<Utc>,
    output_dir: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let question = extract_question(forecast);
    let drivers = extract_drivers(forecast);
    let model = extract_model(forecast);

    let mut md = String::new();

    // Title
    md.push_str(&format!("# Forecast Results: {}\n\n", question));

    // Metadata with sparklines
    let dist_sparkline = sparkline::generate(&[
        results.p5,
        results.p25,
        results.median,
        results.p75,
        results.p95,
    ]);
    let shape = sparkline::distribution_shape(results.mean, results.median, results.std_dev);

    md.push_str(&format!("**Generated:** {}  \n", timestamp.to_rfc3339()));
    md.push_str(&format!(
        "**Mean:** {:.2} | **Median:** {:.2} | **90% CI:** [{:.2}, {:.2}]  \n",
        results.mean, results.median, results.p5, results.p95
    ));
    md.push_str(&format!(
        "**Distribution:** {} {}  \n\n",
        dist_sparkline, shape
    ));

    md.push_str("---\n\n");

    // Statistics section with image
    md.push_str("## 📊 Distribution\n\n");
    md.push_str(&charts_image::generate_histogram_with_image(
        results, output_dir,
    )?);
    md.push_str("\n\n");

    // Generate histogram sparkline from the results
    let histogram = results.histogram(20);
    let hist_sparkline = sparkline::from_histogram(&histogram);

    md.push_str("### Statistics\n\n");
    md.push_str("| Metric | Value | Visualization |\n");
    md.push_str("|--------|-------|---------------|\n");
    md.push_str(&format!("| Mean | {:.2} | |\n", results.mean));
    md.push_str(&format!("| Median | {:.2} | |\n", results.median));
    md.push_str(&format!("| Std Dev | {:.2} | |\n", results.std_dev));
    md.push_str(&format!("| Distribution | | {} |\n", hist_sparkline));
    md.push_str(&format!("| P5 | {:.2} | ├ |\n", results.p5));
    md.push_str(&format!("| P25 | {:.2} | ├ |\n", results.p25));
    md.push_str(&format!("| P50 (Median) | {:.2} | █ |\n", results.median));
    md.push_str(&format!("| P75 | {:.2} | ┤ |\n", results.p75));
    md.push_str(&format!("| P95 | {:.2} | ┤ |\n", results.p95));

    // Percentile range indicator
    let percentile_viz = sparkline::percentile_marker(
        results.p5,
        results.p25,
        results.median,
        results.p75,
        results.p95,
        results.min,
        results.max,
    );
    md.push_str(&format!(
        "| Range | [{:.2}, {:.2}] | {} |\n",
        results.min, results.max, percentile_viz
    ));
    md.push_str("\n\n");

    md.push_str("---\n\n");

    // Forecast structure (mindmap) with image
    md.push_str("## 🧠 Forecast Structure\n\n");
    md.push_str(&charts_image::generate_mindmap_with_image(
        forecast, &question, &drivers, output_dir,
    )?);
    md.push_str("\n\n");

    md.push_str("---\n\n");

    // Model flow with image
    if let Some(model_expr) = model {
        md.push_str("## 🔄 Model Flow\n\n");
        md.push_str(&charts_image::generate_flowchart_with_image(
            &model_expr,
            &drivers,
            output_dir,
        )?);
        md.push_str("\n\n");
        md.push_str("---\n\n");
    }

    // Driver impact flow (Sankey) with image
    md.push_str("## 🌊 Driver Impact Flow\n\n");
    md.push_str(&charts_image::generate_sankey_with_image(
        &drivers, results, output_dir,
    )?);
    md.push_str("\n\n");
    md.push_str("---\n\n");

    // Sensitivity analysis (Tornado) with image
    md.push_str("## 🌪️ Sensitivity Analysis\n\n");
    md.push_str(&charts_image::generate_tornado_with_image(
        &drivers, output_dir,
    )?);
    md.push_str("\n\n");
    md.push_str("---\n\n");

    // Drivers detail
    md.push_str("## 📋 Drivers\n\n");
    for driver in &drivers {
        md.push_str(&format!(
            "### {} ({:?})\n\n",
            driver.name, driver.driver_type
        ));

        if let Some(display_name) = &driver.display_name {
            md.push_str(&format!("**Display Name:** {}\n\n", display_name));
        }

        if let Some(description) = &driver.description {
            md.push_str(&format!("**Description:** {}\n\n", description));
        }

        // Distribution or probability with sparklines
        match &driver.driver_type {
            DriverType::Continuous => {
                if let Some(dist) = &driver.distribution {
                    md.push_str(&format!("**Distribution:** {:?}\n\n", dist));
                }
            }
            DriverType::Binary => {
                if let Some(prob) = driver.probability {
                    let confidence = sparkline::confidence_bar(prob, 10);
                    md.push_str(&format!(
                        "**Probability:** {:.2}% {}\n\n",
                        prob * 100.0,
                        confidence
                    ));
                }
                if let Some(mult) = driver.impact_multiplier {
                    let impact_indicator = if mult > 1.5 {
                        "🔥 High"
                    } else if mult > 1.0 {
                        "↗ Positive"
                    } else if mult > 0.5 {
                        "↘ Negative"
                    } else {
                        "❄️ Strong Negative"
                    };
                    md.push_str(&format!(
                        "**Impact Multiplier:** {:.2}x {}\n\n",
                        mult, impact_indicator
                    ));
                }
            }
            DriverType::Discrete => {
                if let Some(values) = &driver.values {
                    md.push_str("**Values:** ");
                    for (i, v) in values.iter().enumerate() {
                        if i > 0 {
                            md.push_str(", ");
                        }
                        md.push_str(&format!("{:.2}", v));
                    }
                    md.push_str("\n\n");
                }
                if let Some(weights) = &driver.weights {
                    md.push_str("**Weights:** ");
                    for (i, w) in weights.iter().enumerate() {
                        if i > 0 {
                            md.push_str(", ");
                        }
                        let bar = sparkline::confidence_bar(*w, 5);
                        md.push_str(&format!("{:.1}% {}", w * 100.0, bar));
                    }
                    md.push_str("\n\n");

                    // Show sparkline of weight distribution
                    let weight_sparkline = sparkline::generate(weights);
                    md.push_str(&format!("**Distribution:** {}\n\n", weight_sparkline));
                }
            }
        }

        if let Some(unit) = &driver.unit {
            md.push_str(&format!("**Unit:** {}\n\n", unit));
        }

        if let Some(rationale) = &driver.rationale {
            md.push_str(&format!("**Rationale:** {}\n\n", rationale));
        }

        md.push_str("---\n\n");
    }

    Ok(md)
}

fn extract_question(forecast: &Program) -> String {
    for stmt in &forecast.statements {
        if let Statement::Question(q) = stmt {
            return q.text.clone();
        }
    }
    "Unknown Question".to_string()
}

fn extract_drivers(forecast: &Program) -> Vec<DriverStmt> {
    let mut drivers = Vec::new();
    for stmt in &forecast.statements {
        if let Statement::Driver(d) = stmt {
            drivers.push(d.clone());
        }
    }
    drivers
}

fn extract_model(forecast: &Program) -> Option<Expression> {
    for stmt in &forecast.statements {
        if let Statement::Model(m) = stmt {
            return Some(m.expression.clone());
        }
    }
    None
}

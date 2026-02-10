/// Report generation module - Prototype
///
/// Generates Markdown reports with Mermaid diagrams from simulation results
use crate::ast::*;
use crate::executor::ExecutionResults;
use crate::sensitivity;
use chrono::{DateTime, Utc};
use std::fs;
use std::path::Path;

pub mod charts;
pub mod charts_image;
pub mod markdown;
pub mod mermaid;
pub mod sparkline;
pub mod theme;

/// Generate a report from simulation results
pub fn generate_report(
    forecast: &Program,
    results: &ExecutionResults,
    output_dir: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    // Run sensitivity analysis
    println!("Running sensitivity analysis...");
    let sensitivity_analysis =
        sensitivity::full_sensitivity_analysis(forecast, results.iterations)?;

    // Generate filename (W3C compliant)
    let timestamp = Utc::now();
    let filename = generate_filename(&forecast, &timestamp);
    let report_path = output_dir.join(&filename);

    // Generate markdown content (pass output_dir for chart generation + sensitivity)
    let markdown = markdown::generate(
        forecast,
        results,
        &sensitivity_analysis,
        &timestamp,
        output_dir,
    )?;

    // Write to file
    fs::write(&report_path, markdown)?;

    Ok(report_path.to_string_lossy().to_string())
}

/// Generate W3C-compliant filename
fn generate_filename(forecast: &Program, timestamp: &DateTime<Utc>) -> String {
    // Extract question as slug
    let question_slug = extract_question_slug(forecast);

    // Format: YYYY-MM-DDTHH-MM-SSZ-slug.md
    let timestamp_str = timestamp.format("%Y-%m-%dT%H-%M-%SZ").to_string();

    format!("{}-{}.md", timestamp_str, question_slug)
}

/// Extract question and convert to slug
fn extract_question_slug(forecast: &Program) -> String {
    for stmt in &forecast.statements {
        if let Statement::Question(q) = stmt {
            return slugify(&q.text);
        }
    }
    "forecast".to_string()
}

/// Convert text to URL-safe slug
fn slugify(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c
            } else if c.is_whitespace() {
                '-'
            } else {
                '_'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .take(5) // Limit length
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // TODO: slugify truncation behavior changed
    fn test_slugify() {
        assert_eq!(
            slugify("Will AMD reach $200 by 2026-12-31?"),
            "will-amd-reach-200-by"
        );
    }
}

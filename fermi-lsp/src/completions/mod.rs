pub mod builder;
pub mod driver_properties;
pub mod functions;
pub mod keywords;
pub mod operators;

use std::collections::HashMap;
use tower_lsp::lsp_types::*;

/// Context for determining which completions to show
#[derive(Debug, Clone, Default)]
pub struct CompletionContext {
    pub in_driver: bool,
    pub in_evidence: bool,
    pub in_agent: bool,
    pub in_model: bool,
    pub driver_type_position: bool,
    pub top_level: bool,
}

impl CompletionContext {
    /// Analyze text and position to determine completion context
    pub fn analyze(text: &str, position: Position) -> Self {
        let lines: Vec<&str> = text.lines().collect();
        let line_idx = position.line as usize;

        // Get current line and lines before
        let current_line = lines.get(line_idx).unwrap_or(&"").trim();

        // Check if we're in a block by looking backward for block start
        let mut in_driver = false;
        let mut in_evidence = false;
        let mut in_agent = false;
        let mut in_model = false;
        let mut driver_type_position = false;
        let mut brace_depth = 0;

        // Scan backwards from current position to find context
        for i in (0..=line_idx).rev() {
            let line = lines[i].trim();

            // Count braces to track block depth
            let open_braces = line.matches('{').count();
            let close_braces = line.matches('}').count();

            if i == line_idx {
                brace_depth = 0;
            } else {
                brace_depth += close_braces as i32;
                brace_depth -= open_braces as i32;
            }

            // If we have more opening braces than closing (negative depth when scanning backwards), we're inside a block
            if brace_depth < 0 {
                if line.starts_with("driver ") {
                    in_driver = true;
                    break;
                } else if line.starts_with("evidence ") {
                    in_evidence = true;
                    break;
                } else if line.starts_with("agent ") {
                    in_agent = true;
                    break;
                }
            }
        }

        // Check if we're on a model line
        if current_line.starts_with("model:") || current_line.starts_with("model ") {
            in_model = true;
        }

        // Check if we're at a driver type position (right after "driver <name> ")
        if current_line.starts_with("driver ") {
            let parts: Vec<&str> = current_line.split_whitespace().collect();
            if parts.len() == 2 {
                // "driver <name>" - cursor is right after name
                driver_type_position = true;
            } else if parts.len() >= 3 && !current_line.contains('{') {
                // "driver <name> <partial_type>" - user is typing the type
                driver_type_position = true;
            }
        }

        // Check if we're at top level (not in any block)
        let top_level = !in_driver && !in_evidence && !in_agent && brace_depth == 0;

        CompletionContext {
            in_driver,
            in_evidence,
            in_agent,
            in_model,
            driver_type_position,
            top_level,
        }
    }

    pub fn is_top_level(&self) -> bool {
        self.top_level
    }

    pub fn is_driver_type_position(&self) -> bool {
        self.driver_type_position
    }

    pub fn is_in_driver(&self) -> bool {
        self.in_driver
    }

    pub fn is_in_evidence(&self) -> bool {
        self.in_evidence
    }

    pub fn is_in_agent(&self) -> bool {
        self.in_agent
    }

    pub fn is_in_model(&self) -> bool {
        self.in_model
    }
}

/// Main completion function - orchestrates all completion sources
pub fn get_completions(
    context: &CompletionContext,
    driver_names: &HashMap<String, String>,
) -> Vec<CompletionItem> {
    let mut completions = Vec::new();

    // Top-level keywords
    if context.is_top_level() {
        completions.extend(keywords::get_keyword_completions());
    }

    // Driver types (after "driver <name> ")
    if context.is_driver_type_position() {
        completions.extend(keywords::get_driver_type_completions());
    }

    // Driver properties (inside driver blocks)
    if context.is_in_driver() {
        completions.extend(driver_properties::get_driver_property_completions());
    }

    // Evidence properties (inside evidence blocks)
    if context.is_in_evidence() {
        completions.extend(get_evidence_property_completions());
    }

    // Agent properties (inside agent blocks)
    if context.is_in_agent() {
        completions.extend(get_agent_property_completions());
    }

    // In model expressions - add functions, operators, driver names
    if context.is_in_model() {
        completions.extend(functions::get_math_function_completions());
        completions.extend(operators::get_control_flow_completions());
        completions.extend(operators::get_logical_operator_completions());
        completions.extend(operators::get_arithmetic_operator_completions());

        // Add driver names as variables
        for (name, dist_type) in driver_names {
            completions.push(
                builder::CompletionBuilder::variable(name)
                    .detail(format!("Driver variable: {}", name))
                    .docs(format!("Type: {}", dist_type))
                    .build(),
            );
        }
    }

    // Distribution functions (in distribution: context)
    completions.extend(functions::get_distribution_completions());

    completions
}

/// Get evidence property completions
fn get_evidence_property_completions() -> Vec<CompletionItem> {
    vec![
        builder::CompletionBuilder::property("source")
            .detail("Citation or source of the evidence")
            .docs("Example: \"Morgan Stanley Q4 2025 Report\"")
            .snippet("source: \"${1:source}\"")
            .sort("00_source")
            .build(),
        builder::CompletionBuilder::property("summary")
            .detail("Brief summary of the evidence")
            .docs("1-2 sentence summary of key findings")
            .snippet("summary: \"${1:summary}\"")
            .sort("01_summary")
            .build(),
        builder::CompletionBuilder::property("relevance")
            .detail("Relevance score (0-1)")
            .docs("How relevant this evidence is to the forecast")
            .snippet("relevance: ${1:0.8}")
            .sort("02_relevance")
            .build(),
        builder::CompletionBuilder::property("date")
            .detail("Date of evidence (YYYY-MM-DD)")
            .docs("When the evidence was published or observed")
            .snippet("date: ${1:2026-01-01}")
            .sort("03_date")
            .build(),
        builder::CompletionBuilder::property("url")
            .detail("URL link to evidence")
            .docs("Web link for reference")
            .snippet("url: \"${1:https://...}\"")
            .sort("04_url")
            .build(),
        builder::CompletionBuilder::property("strength")
            .detail("Quality/strength score (0-1)")
            .docs("How strong/reliable this evidence is")
            .snippet("strength: ${1:0.8}")
            .sort("05_strength")
            .build(),
    ]
}

/// Get agent property completions
fn get_agent_property_completions() -> Vec<CompletionItem> {
    vec![
        builder::CompletionBuilder::property("query")
            .detail("Search query string")
            .docs("What to search for or monitor")
            .snippet("query: \"${1:search query}\"")
            .sort("00_query")
            .build(),
        builder::CompletionBuilder::property("schedule")
            .detail("Execution schedule (every N unit)")
            .docs("How often to run the agent. Units: day, week, month, year")
            .snippet("schedule: every ${1:1} ${2|day,week,month|}")
            .sort("01_schedule")
            .build(),
    ]
}

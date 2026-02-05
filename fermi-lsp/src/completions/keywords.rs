use super::builder::CompletionBuilder;
use tower_lsp::lsp_types::*;

/// Get top-level keyword completions (question, driver, model, etc.)
pub fn get_keyword_completions() -> Vec<CompletionItem> {
    vec![
        CompletionBuilder::keyword("question")
            .detail("Define the forecast question - the core prediction you want to make")
            .docs("Example:\nquestion \"Will AMD reach $200 by 2026-12-31?\"")
            .snippet("question \"${1:What is your forecast question?}\"")
            .sort("00_question")
            .build(),

        CompletionBuilder::keyword("driver")
            .detail("Define a driver variable - a factor that influences the forecast")
            .docs("Drivers can be continuous, binary, or discrete.\nExample:\ndriver market_size continuous {\n    distribution: triangular(100, 200, 500)\n    unit: \"millions\"\n}")
            .snippet("driver ${1:name} ${2|continuous,binary,discrete|} {\n\t${3:distribution: triangular(${4:min}, ${5:likely}, ${6:max})}\n\t${7:unit: \"${8:units}\"}\n\t${9:rationale: \"${10:reasoning}\"}\n}")
            .sort("01_driver")
            .build(),

        CompletionBuilder::keyword("model")
            .detail("Define the forecast model - mathematical expression combining drivers")
            .docs("Use driver names and math operators.\nExample:\nmodel: revenue_per_user * num_users * growth_rate")
            .snippet("model: ${1:expression}")
            .sort("02_model")
            .build(),

        CompletionBuilder::keyword("simulate")
            .detail("Run Monte Carlo simulation to generate probabilistic outcomes")
            .docs("Higher iteration counts give more accurate results but take longer.\nExample:\nsimulate 10000 iterations")
            .snippet("simulate ${1:10000} iterations")
            .sort("03_simulate")
            .build(),

        CompletionBuilder::keyword("evidence")
            .detail("Document evidence supporting your forecast assumptions")
            .docs("Track sources, summaries, and relevance scores.\nExample:\nevidence analyst_report {\n    source: \"Morgan Stanley Q4 2025\"\n    summary: \"Projected 25% YoY growth\"\n    relevance: 0.85\n}")
            .snippet("evidence ${1:name} {\n\tsource: \"${2:source}\"\n\tsummary: \"${3:summary}\"\n\trelevance: ${4:0.8}\n\tdate: ${5:2026-01-01}\n}")
            .sort("04_evidence")
            .build(),

        CompletionBuilder::keyword("agent")
            .detail("Create an automated research agent to track information over time")
            .docs("Agents can search and monitor topics on a schedule.\nExample:\nagent market_monitor {\n    type: \"research\"\n    query: \"semiconductor market growth\"\n    executor: \"llm\"\n    schedule: every 1 week\n}")
            .snippet("agent ${1:name} {\n\ttype: \"${2|research,sentiment,competitive,market|}\"\n\tquery: \"${3:search query}\"\n\texecutor: \"${4|llm,mcp,manual,skill|}\"\n\tschedule: every ${5:1} ${6|day,week,month|}\n}")
            .sort("05_agent")
            .build(),

        CompletionBuilder::keyword("base_rate")
            .detail("Define base rate (outside view) - Tetlock superforecasting methodology")
            .docs("Establish reference class and historical frequency.\nUsed inside question blocks.\nExample:\nbase_rate {\n    reference_class: \"Similar tech stocks\"\n    historical_frequency: 0.15p\n    source: \"Historical data 2015-2025\"\n    generated_by: human\n}")
            .snippet("base_rate {\n\treference_class: \"${1:similar situations}\"\n\thistorical_frequency: ${2:0.5}p\n\tsample_size: ${3:100}\n\tsource: \"${4:data source}\"\n\treasoning: \"${5:explanation}\"\n\tgenerated_by: ${6|human,agent|}\n}")
            .sort("06_base_rate")
            .build(),
    ]
}

/// Get driver type completions (continuous, binary, discrete)
pub fn get_driver_type_completions() -> Vec<CompletionItem> {
    vec![
        CompletionBuilder::keyword("continuous")
            .detail("Continuous probability distribution - for numeric values across a range")
            .docs("Use for: prices, sizes, counts, rates, percentages\nExample: revenue, customer_count, growth_rate")
            .sort("00_continuous")
            .build(),

        CompletionBuilder::keyword("binary")
            .detail("Binary outcome (yes/no) - for events that either happen or don't")
            .docs("Use for: deal closures, product launches, regulatory approvals\nExample: big_deal_closes, launch_succeeds")
            .sort("01_binary")
            .build(),

        CompletionBuilder::keyword("discrete")
            .detail("Discrete values - specific options with probabilities")
            .docs("Use for: scenarios, market states, categorical outcomes\nExample: market_scenario (bear/normal/bull)")
            .sort("02_discrete")
            .build(),
    ]
}

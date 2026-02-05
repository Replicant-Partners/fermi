use colored::*;
/// Fermi CLI - Interactive FPL REPL and Compiler
///
/// This is the main entry point for the Fermi forecasting language.
use fermi::{execute_program, Lexer, Parser, SemanticAnalyzer, Statement, TokenType};
use std::fs;
use std::io::{self, Write};

fn main() {
    println!(
        "{}",
        "╔═══════════════════════════════════════════╗".bright_cyan()
    );
    println!(
        "{}",
        "║   Fermi - Forecasting Language v0.4.0   ║".bright_cyan()
    );
    println!(
        "{}",
        "║   Agent Fermi's Broca Brain              ║".bright_cyan()
    );
    println!(
        "{}",
        "║   Now with Monte Carlo Execution!       ║".bright_cyan()
    );
    println!(
        "{}",
        "╚═══════════════════════════════════════════╝".bright_cyan()
    );
    println!();

    // Check if a file was provided
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 {
        // File mode
        let filename = &args[1];
        match fs::read_to_string(filename) {
            Ok(source) => {
                println!("📄 Processing file: {}\n", filename.bright_yellow());
                process_source(&source);
            }
            Err(e) => {
                eprintln!(
                    "{} Could not read file '{}': {}",
                    "Error:".bright_red(),
                    filename,
                    e
                );
                std::process::exit(1);
            }
        }
    } else {
        // REPL mode
        println!("Welcome to the Fermi REPL!");
        println!("Type FPL code or 'help' for help, 'exit' to quit.\n");

        repl();
    }
}

fn process_source(source: &str) {
    // Step 1: Lexical Analysis
    println!("{}", "Stage 1: Lexical Analysis".bright_yellow().bold());
    println!("{}", "─".repeat(50));

    let lexer = Lexer::new(source);

    let tokens = match lexer.tokenize() {
        Ok(tokens) => {
            println!("{} Tokenization successful!", "✓".bright_green());
            tokens
        }
        Err(errors) => {
            println!(
                "{} Tokenization failed with {} error(s):",
                "✗".bright_red(),
                errors.len()
            );
            println!();

            for error in errors {
                println!("  {} {}", "Error:".bright_red(), error);
            }

            std::process::exit(1);
        }
    };

    // Display token summary
    let mut token_counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();

    for token in &tokens {
        let category = match &token.token_type {
            TokenType::Question
            | TokenType::Driver
            | TokenType::Evidence
            | TokenType::Agent
            | TokenType::Model
            | TokenType::Simulate => "Statements",
            TokenType::String(_)
            | TokenType::Number(_)
            | TokenType::Probability(_)
            | TokenType::Date(_)
            | TokenType::Boolean(_) => "Literals",
            TokenType::Identifier(_) => "Identifiers",
            TokenType::Triangular
            | TokenType::Normal
            | TokenType::Lognormal
            | TokenType::Uniform
            | TokenType::Beta => "Distributions",
            TokenType::Plus | TokenType::Minus | TokenType::Star | TokenType::Slash => "Operators",
            TokenType::EOF => continue,
            _ => "Other",
        };
        *token_counts.entry(category).or_insert(0) += 1;
    }

    println!("\nToken Summary:");
    for (category, count) in token_counts.iter() {
        println!("  {}: {}", category.bright_blue(), count);
    }

    // Step 2: Parsing
    println!(
        "\n{}",
        "Stage 2: Syntax Analysis (Parsing)".bright_yellow().bold()
    );
    println!("{}", "─".repeat(50));

    let parser = Parser::new(tokens);

    match parser.parse() {
        Ok(program) => {
            println!("{} Parsing successful!", "✓".bright_green());
            println!();

            println!("{}", "Abstract Syntax Tree:".bright_cyan().bold());
            println!("  {} statement(s) parsed\n", program.statements.len());

            for (i, stmt) in program.statements.iter().enumerate() {
                println!(
                    "{}. {}",
                    (i + 1).to_string().bright_white(),
                    format!("{}", stmt).bright_cyan()
                );

                // Show details for each statement type
                match stmt {
                    Statement::Question(q) => {
                        println!("   └─ Text: \"{}\"", q.text.bright_green());
                    }
                    Statement::Driver(d) => {
                        println!("   ├─ Type: {:?}", d.driver_type);
                        if let Some(dist) = &d.distribution {
                            println!(
                                "   ├─ Distribution: {}",
                                format!("{}", dist).bright_yellow()
                            );
                        }
                        if let Some(prob) = d.probability {
                            println!("   ├─ Probability: {}p", prob);
                        }
                        if let Some(mult) = d.impact_multiplier {
                            println!("   ├─ Impact: {}x", mult);
                        }
                        if let Some(unit) = &d.unit {
                            println!("   └─ Unit: \"{}\"", unit);
                        } else {
                            println!("   └─ (end)");
                        }
                    }
                    Statement::Evidence(e) => {
                        println!("   ├─ Source: \"{}\"", e.source);
                        if let Some(rel) = e.relevance {
                            println!("   ├─ Relevance: {}p", rel);
                        }
                        if let Some(date) = &e.date {
                            println!("   └─ Date: {}", date);
                        } else {
                            println!("   └─ (end)");
                        }
                    }
                    Statement::Agent(a) => {
                        println!("   ├─ Query: \"{}\"", a.query);
                        if let Some(sched) = &a.schedule {
                            println!("   └─ Schedule: {:?}", sched);
                        } else {
                            println!("   └─ (end)");
                        }
                    }
                    Statement::Model(m) => {
                        println!(
                            "   └─ Expression: {}",
                            format!("{}", m.expression).bright_yellow()
                        );
                    }
                    Statement::Simulate(s) => {
                        println!(
                            "   └─ Iterations: {}",
                            s.iterations.to_string().bright_yellow()
                        );
                    }
                }
                println!();
            }

            // Step 3: Semantic Analysis
            println!("\n{}", "Stage 3: Semantic Analysis".bright_yellow().bold());
            println!("{}", "─".repeat(50));

            let analyzer = SemanticAnalyzer::new();
            let analysis = analyzer.analyze(&program);

            if analysis.is_valid() {
                println!("{} Semantic analysis passed!", "✓".bright_green());
            } else {
                println!(
                    "{} Semantic analysis found {} error(s)",
                    "✗".bright_red(),
                    analysis.errors.len()
                );
            }
            println!();

            // Show symbol table
            println!("{}", "Symbol Table:".bright_cyan().bold());
            let drivers = analysis.symbol_table.drivers();
            let evidence = analysis.symbol_table.evidence();

            if !drivers.is_empty() {
                println!("  Drivers:");
                for driver in drivers {
                    let used = if analysis
                        .symbol_table
                        .drivers_used_in_model()
                        .contains(&driver.name)
                    {
                        "✓".bright_green()
                    } else {
                        "○".bright_yellow()
                    };
                    println!(
                        "    {} {} : {}",
                        used,
                        driver.name.bright_white(),
                        driver.ty
                    );
                }
            }

            if !evidence.is_empty() {
                println!("  Evidence:");
                for ev in evidence {
                    println!("    • {}", ev.name.bright_white());
                }
            }
            println!();

            // Show errors
            if !analysis.errors.is_empty() {
                println!("{}", "Errors:".bright_red().bold());
                for error in &analysis.errors {
                    println!("  {} {}", "✗".bright_red(), error);
                }
                println!();
            }

            // Show warnings
            if !analysis.warnings.is_empty() {
                println!("{}", "Warnings:".bright_yellow().bold());
                for warning in &analysis.warnings {
                    println!("  {} {}", "⚠".bright_yellow(), warning);
                }
                println!();
            }

            println!("\n{}", "=".repeat(50));
            if analysis.is_valid() {
                println!(
                    "{} {} {}",
                    "✓".bright_green(),
                    "All checks passed!".bright_green().bold(),
                    "Ready for execution.".bright_blue()
                );

                // Step 4: Execution (Monte Carlo Simulation)
                println!(
                    "\n{}",
                    "Stage 4: Execution (Monte Carlo Simulation)"
                        .bright_yellow()
                        .bold()
                );
                println!("{}", "─".repeat(50));

                match execute_program(&program) {
                    Ok(result) => {
                        println!("{} Simulation completed successfully!", "✓".bright_green());
                        println!();

                        // Display results
                        println!("{}", "Simulation Results:".bright_cyan().bold());
                        println!(
                            "  {} {}",
                            "Iterations:".bright_blue(),
                            result.iterations.to_string().bright_white()
                        );
                        println!();

                        println!("{}", "  Statistics:".bright_cyan());
                        println!("    {} {:.2}", "Mean:".bright_blue(), result.mean);
                        println!("    {} {:.2}", "Std Dev:".bright_blue(), result.std_dev);
                        println!();

                        println!("{}", "  Percentiles:".bright_cyan());
                        println!("    {} {:.2}", "5th:".bright_blue(), result.p5);
                        println!(
                            "    {} {:.2}",
                            "50th (Median):".bright_blue(),
                            result.median
                        );
                        println!("    {} {:.2}", "95th:".bright_blue(), result.p95);
                        println!();

                        println!("{}", "  Ranges:".bright_cyan());
                        println!(
                            "    {} {:.2} to {:.2}",
                            "90% CI (p5-p95):".bright_blue(),
                            result.p5,
                            result.p95
                        );
                        println!(
                            "    {} {:.2} to {:.2}",
                            "IQR (p25-p75):".bright_blue(),
                            result.p25,
                            result.p75
                        );
                        println!();

                        // Visual distribution (simple ASCII histogram)
                        println!("{}", "  Distribution:".bright_cyan());
                        print_histogram(&result.samples);

                        println!("\n{}", "=".repeat(50));
                        println!(
                            "{} {} Mean: {:.2}, Median: {:.2}, Range: [{:.2}, {:.2}]",
                            "✓".bright_green(),
                            "Forecast Complete!".bright_green().bold(),
                            result.mean,
                            result.median,
                            result.p5,
                            result.p95
                        );
                    }
                    Err(error) => {
                        println!("{} Execution failed:", "✗".bright_red());
                        println!();
                        println!("  {} {}", "Error:".bright_red(), error);
                        println!();
                        std::process::exit(1);
                    }
                }
            } else {
                println!(
                    "{} {} {}",
                    "✗".bright_red(),
                    "Semantic errors found.".bright_red().bold(),
                    "Please fix the errors above.".bright_yellow()
                );
                std::process::exit(1);
            }
        }
        Err(error) => {
            println!("{} Parsing failed:", "✗".bright_red());
            println!();
            println!("  {} {}", "Error:".bright_red(), error);
            println!();

            std::process::exit(1);
        }
    }
}

fn repl() {
    let mut buffer = String::new();
    let mut line_number = 1;

    loop {
        // Print prompt
        if buffer.is_empty() {
            print!("{} ", "fermi>".bright_cyan().bold());
        } else {
            print!("{} ", "     >".bright_cyan());
        }
        io::stdout().flush().unwrap();

        // Read line
        let mut line = String::new();
        match io::stdin().read_line(&mut line) {
            Ok(_) => {
                let trimmed = line.trim();

                // Check for special commands
                if buffer.is_empty() {
                    match trimmed {
                        "exit" | "quit" => {
                            println!("Goodbye! 👋");
                            break;
                        }
                        "help" => {
                            print_help();
                            continue;
                        }
                        "clear" => {
                            print!("\x1B[2J\x1B[1;1H");
                            continue;
                        }
                        _ => {}
                    }
                }

                // Add to buffer
                buffer.push_str(&line);

                // Check if we should execute (simple heuristic: empty line after content)
                if trimmed.is_empty() && !buffer.trim().is_empty() {
                    execute_repl_input(&buffer);
                    buffer.clear();
                    line_number = 1;
                } else {
                    line_number += 1;
                }
            }
            Err(e) => {
                eprintln!("{} Failed to read line: {}", "Error:".bright_red(), e);
                break;
            }
        }
    }
}

fn execute_repl_input(input: &str) {
    println!();

    let lexer = Lexer::new(input);

    let tokens = match lexer.tokenize() {
        Ok(tokens) => tokens,
        Err(errors) => {
            println!("{} Lexical errors:", "✗".bright_red());
            for error in errors {
                println!("  • {}", error);
            }
            println!();
            return;
        }
    };

    // Filter out EOF for display
    let display_tokens: Vec<_> = tokens
        .iter()
        .filter(|t| !matches!(t.token_type, TokenType::EOF))
        .collect();

    if display_tokens.is_empty() {
        println!("{} No tokens generated", "Info:".bright_blue());
        println!();
        return;
    }

    println!(
        "{} Tokenized {} token(s)",
        "✓".bright_green(),
        display_tokens.len()
    );

    // Try parsing
    let parser = Parser::new(tokens);

    match parser.parse() {
        Ok(program) => {
            println!(
                "{} Parsed {} statement(s)",
                "✓".bright_green(),
                program.statements.len()
            );

            for stmt in &program.statements {
                println!("  • {}", stmt.to_string().bright_cyan());
            }
        }
        Err(error) => {
            println!("{} Parse error: {}", "✗".bright_red(), error);
        }
    }

    println!();
}

fn print_help() {
    println!();
    println!("{}", "Fermi REPL Commands:".bright_yellow().bold());
    println!("  {}  - Show this help message", "help".bright_cyan());
    println!("  {}  - Clear the screen", "clear".bright_cyan());
    println!("  {}  - Exit the REPL", "exit".bright_cyan());
    println!();
    println!("{}", "FPL Language Examples:".bright_yellow().bold());
    println!();
    println!("  {}Define a question:{}", "1. ".bright_green(), "");
    println!(
        "     {}",
        r#"question "Will AMD reach $200 by 2026-12-31?""#.bright_white()
    );
    println!();
    println!("  {}Add a driver:{}", "2. ".bright_green(), "");
    println!("     {}", "driver market_size continuous {".bright_white());
    println!(
        "         {}",
        "distribution: triangular(500, 1200, 2500)".bright_white()
    );
    println!("         {}", r#"unit: "millions USD""#.bright_white());
    println!("     {}", "}".bright_white());
    println!();
    println!("  {}Add evidence:{}", "3. ".bright_green(), "");
    println!("     {}", "evidence market_report {".bright_white());
    println!("         {}", r#"source: "Gartner 2025""#.bright_white());
    println!("         {}", "relevance: 0.9p".bright_white());
    println!("     {}", "}".bright_white());
    println!();
    println!("  {}Run simulation:{}", "4. ".bright_green(), "");
    println!("     {}", "simulate 10000 iterations".bright_white());
    println!();
    println!(
        "{}",
        "To execute multi-line code, press Enter twice after your last line.".bright_blue()
    );
    println!();
}

fn print_histogram(samples: &[f64]) {
    // Create a simple ASCII histogram
    const BINS: usize = 20;
    const BAR_WIDTH: usize = 50;

    // Find min and max
    let min = samples.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = samples.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    // Create bins
    let bin_width = (max - min) / BINS as f64;
    let mut bins = vec![0usize; BINS];

    // Fill bins
    for &sample in samples {
        let bin_idx = ((sample - min) / bin_width).floor() as usize;
        let bin_idx = bin_idx.min(BINS - 1); // Handle edge case where sample == max
        bins[bin_idx] += 1;
    }

    // Find max count for scaling
    let max_count = *bins.iter().max().unwrap_or(&1);

    // Print histogram
    for (i, &count) in bins.iter().enumerate() {
        let bin_start = min + i as f64 * bin_width;
        let bin_end = bin_start + bin_width;
        let bar_len = (count as f64 / max_count as f64 * BAR_WIDTH as f64) as usize;
        let bar = "█".repeat(bar_len);

        println!(
            "    {:8.1} - {:8.1} │ {:<50} {}",
            bin_start,
            bin_end,
            bar.bright_green(),
            count
        );
    }
}

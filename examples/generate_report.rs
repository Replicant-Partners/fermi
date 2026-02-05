/// Example: Generate a Markdown report from a forecast
use fermi::*;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load and parse the forecast
    let fpl_code = std::fs::read_to_string("refactor_test.fpl")?;

    // Lexical analysis
    let lexer = Lexer::new(&fpl_code);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(e) => return Err(format!("Lexer error: {:?}", e).into()),
    };

    // Syntax analysis
    let parser = Parser::new(tokens);
    let program = match parser.parse() {
        Ok(p) => p,
        Err(e) => return Err(format!("Parse error: {}", e).into()),
    };

    // Semantic analysis
    let analyzer = SemanticAnalyzer::new();
    let analysis = analyzer.analyze(&program);

    if !analysis.errors.is_empty() {
        eprintln!("Semantic errors:");
        for error in analysis.errors {
            eprintln!("  - {:?}", error);
        }
        return Err("Semantic analysis failed".into());
    }

    // Execute simulation
    println!("Running simulation...");
    let mut executor = Executor::new(10_000);
    let results = executor.execute(&program)?;

    println!("Mean: {:.2}, Median: {:.2}", results.mean, results.median);

    // Generate report
    println!("\nGenerating report...");
    let output_dir = Path::new("results/prototype");
    std::fs::create_dir_all(output_dir)?;

    let report_path = fermi::report::generate_report(&program, &results, output_dir)?;

    println!("✅ Report generated: {}", report_path);
    println!("\nOpen in Zed to see Mermaid diagrams!");

    Ok(())
}

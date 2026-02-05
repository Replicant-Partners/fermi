use regex::Regex;
/// Standalone Markdown Renderer
///
/// Renders any markdown file with Mermaid diagrams to images
///
/// Usage:
///   cargo run --release --example render_markdown input.md [output_dir]
///
/// Features:
/// - Extracts Mermaid code blocks from markdown
/// - Generates PNG images using mmdc (Mermaid CLI)
/// - Creates output markdown with image references
/// - Preserves original Mermaid code in collapsible sections
/// - Applies Ayu Mirage theme to all diagrams
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <input.md> [output_dir]", args[0]);
        eprintln!("\nExample:");
        eprintln!("  {} my_document.md", args[0]);
        eprintln!("  {} my_document.md ./rendered", args[0]);
        std::process::exit(1);
    }

    let input_path = PathBuf::from(&args[1]);
    let output_dir = if args.len() > 2 {
        PathBuf::from(&args[2])
    } else {
        PathBuf::from("rendered_output")
    };

    // Validate input file
    if !input_path.exists() {
        eprintln!("Error: Input file '{}' not found", input_path.display());
        std::process::exit(1);
    }

    if !input_path.extension().map_or(false, |ext| ext == "md") {
        eprintln!("Warning: Input file doesn't have .md extension");
    }

    println!("📖 Rendering markdown file: {}", input_path.display());
    println!("📁 Output directory: {}", output_dir.display());

    // Create output directory structure
    fs::create_dir_all(&output_dir)?;
    // Don't create charts_dir here - mermaid::generate_image will create it
    let charts_dir = output_dir.clone();

    // Read input markdown
    let markdown_content = fs::read_to_string(&input_path)?;

    // Extract and render Mermaid diagrams
    let rendered_markdown = render_mermaid_diagrams(&markdown_content, &charts_dir)?;

    // Generate output filename
    let output_filename = input_path
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .to_string()
        + "-rendered.md";
    let output_path = output_dir.join(output_filename);

    // Write rendered markdown
    fs::write(&output_path, rendered_markdown)?;

    println!("\n✅ Rendering complete!");
    println!("📄 Output file: {}", output_path.display());
    println!("🖼️  Charts saved to: {}", charts_dir.display());
    println!("\n💡 Open the rendered markdown file in Zed or your favorite markdown viewer!");

    Ok(())
}

fn render_mermaid_diagrams(
    markdown: &str,
    charts_dir: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    use fermi::report::mermaid::{generate_image, is_mmdc_available, ImageFormat};

    // Regex to find Mermaid code blocks
    let mermaid_regex = Regex::new(r"```mermaid\n([\s\S]*?)```")?;

    let mut result = markdown.to_string();
    let mut chart_index = 0;

    // Check if mmdc is available
    if !is_mmdc_available() {
        println!("⚠️  Warning: Mermaid CLI (mmdc) not found.");
        println!("   Install with: npm install -g @mermaid-js/mermaid-cli");
        println!("   Mermaid diagrams will remain as code blocks.");
        return Ok(result);
    }

    // Find all Mermaid blocks and store their info
    let mut diagrams: Vec<(usize, usize, String)> = Vec::new();
    for capture in mermaid_regex.captures_iter(markdown) {
        if let (Some(full_match), Some(code_match)) = (capture.get(0), capture.get(1)) {
            diagrams.push((
                full_match.start(),
                full_match.end(),
                code_match.as_str().to_string(),
            ));
        }
    }

    if diagrams.is_empty() {
        println!("ℹ️  No Mermaid diagrams found in the markdown file.");
        return Ok(result);
    }

    println!(
        "\n🎨 Found {} Mermaid diagram(s), rendering...",
        diagrams.len()
    );

    // Process each Mermaid block in reverse order (to preserve string indices)
    for (start, end, mermaid_code) in diagrams.iter().rev() {
        // Apply theme if not already themed
        let themed_code = if !mermaid_code.contains("%%{init") {
            apply_theme_to_mermaid(mermaid_code)
        } else {
            mermaid_code.to_string()
        };

        // Generate image
        let chart_name = format!("diagram-{}", chart_index);
        match generate_image(&themed_code, charts_dir, &chart_name, ImageFormat::PNG) {
            Ok(image_path) => {
                let relative_path = format!(
                    "charts/{}",
                    image_path.file_name().unwrap().to_string_lossy()
                );

                // Create replacement with image and collapsible source
                let replacement = format!(
                    "![{}]({})\n\n<details>\n<summary>📝 View Mermaid Source</summary>\n\n```mermaid\n{}\n```\n\n</details>",
                    chart_name,
                    relative_path,
                    themed_code
                );

                // Replace in result string
                result.replace_range(*start..*end, &replacement);

                println!("  ✓ Rendered: {}.png", chart_name);
                chart_index += 1;
            }
            Err(e) => {
                eprintln!("  ✗ Failed to render diagram {}: {}", chart_index, e);
                // Leave the original code block in place
            }
        }
    }

    Ok(result)
}

fn apply_theme_to_mermaid(mermaid_code: &str) -> String {
    use fermi::report::theme::{generate_mermaid_theme_config, generate_xychart_theme, AYU_MIRAGE};

    let trimmed = mermaid_code.trim();

    // Detect chart type and apply appropriate theme
    if trimmed.starts_with("xychart") {
        format!("{}{}", generate_xychart_theme(&AYU_MIRAGE), mermaid_code)
    } else {
        format!(
            "{}{}",
            generate_mermaid_theme_config(&AYU_MIRAGE),
            mermaid_code
        )
    }
}

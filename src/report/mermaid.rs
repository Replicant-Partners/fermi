/// Mermaid CLI integration for generating chart images
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Generate an image from Mermaid code using mermaid-cli
pub fn generate_image(
    mermaid_code: &str,
    output_dir: &Path,
    chart_name: &str,
    format: ImageFormat,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    // Create charts subdirectory
    let charts_dir = output_dir.join("charts");
    fs::create_dir_all(&charts_dir)?;

    // Create puppeteer config for Linux compatibility
    let puppeteer_config = r#"{
  "args": ["--no-sandbox", "--disable-setuid-sandbox"]
}"#;
    let config_path = charts_dir.join("puppeteer-config.json");
    fs::write(&config_path, puppeteer_config)?;

    // Write .mmd file
    let mmd_path = charts_dir.join(format!("{}.mmd", chart_name));
    fs::write(&mmd_path, mermaid_code)?;

    // Generate image
    let image_ext = match format {
        ImageFormat::PNG => "png",
        ImageFormat::SVG => "svg",
    };
    let image_path = charts_dir.join(format!("{}.{}", chart_name, image_ext));

    // Run mmdc (with puppeteer args for Linux compatibility)
    let output = Command::new("mmdc")
        .arg("-i")
        .arg(&mmd_path)
        .arg("-o")
        .arg(&image_path)
        .arg("-b")
        .arg("transparent") // Transparent background
        .arg("-p")
        .arg(&config_path) // Use our puppeteer config
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("mmdc failed: {}", stderr).into());
    }

    Ok(image_path)
}

/// Image format options
#[derive(Debug, Clone, Copy)]
pub enum ImageFormat {
    PNG,
    SVG,
}

/// Generate markdown with image and collapsible source
pub fn generate_chart_markdown(
    mermaid_code: &str,
    image_path: &Path,
    chart_title: &str,
    relative_path: &str,
) -> String {
    let mut md = String::new();

    // Image reference
    md.push_str(&format!("![{}]({})\n\n", chart_title, relative_path));

    // Collapsible Mermaid source
    md.push_str("<details>\n");
    md.push_str("<summary>📝 View Mermaid Source</summary>\n\n");
    md.push_str("```mermaid\n");
    md.push_str(mermaid_code);
    md.push_str("```\n\n");
    md.push_str("</details>\n");

    md
}

/// Check if mermaid-cli is installed
pub fn is_mmdc_available() -> bool {
    Command::new("mmdc")
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mmdc_available() {
        // This will fail in CI if mmdc not installed
        // but that's okay - it's an optional feature
        if is_mmdc_available() {
            println!("✓ mmdc is available");
        } else {
            println!("✗ mmdc not available - install with: npm install -g @mermaid-js/mermaid-cli");
        }
    }
}

//! Local manifest read/write helpers and Issue rendering for the CLI.
//!
//! The actual validation logic lives in `abw_apps_core` — this module
//! is just a thin presentation layer that finds the manifest file, parses
//! it, and renders the builder's `Issue` list as ANSI-coloured CLI output.

use anyhow::{anyhow, Context, Result};
use colored::*;
use abw_apps_core::{BuildResult, Issue, PartialManifest, Severity};
use std::path::{Path, PathBuf};

/// Locate manifest.json from the working directory or from an explicit path.
/// Searches CWD first, then walks up to 3 parents for a `manifest.json` —
/// matches the ergonomics of `cargo` / `npm` / `git`.
pub fn find_manifest(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        if p.is_file() {
            return Ok(p.to_path_buf());
        }
        // If explicit is a directory, look inside it.
        if p.is_dir() {
            let inside = p.join("manifest.json");
            if inside.is_file() {
                return Ok(inside);
            }
        }
        return Err(anyhow!("no manifest.json at {}", p.display()));
    }
    let mut cur = std::env::current_dir().context("getting current directory")?;
    for _ in 0..4 {
        let candidate = cur.join("manifest.json");
        if candidate.is_file() {
            return Ok(candidate);
        }
        if !cur.pop() {
            break;
        }
    }
    Err(anyhow!(
        "no manifest.json found in this directory or its parents. \
         Run `abw app new <slug>` to scaffold one, or pass --manifest <path>."
    ))
}

pub fn read_manifest(path: &Path) -> Result<PartialManifest> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .with_context(|| format!("parsing {} as JSON", path.display()))?;
    Ok(PartialManifest::from_value(&value))
}

pub fn write_manifest(path: &Path, manifest: &serde_json::Value) -> Result<()> {
    let pretty = serde_json::to_string_pretty(manifest)
        .context("serializing manifest")?;
    std::fs::write(path, format!("{}\n", pretty))
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Render a single Issue as a CLI line.
pub fn render_issue(issue: &Issue) {
    let prefix = match issue.severity {
        Severity::Error => "error".red().bold().to_string(),
        Severity::Warning => "warn ".yellow().bold().to_string(),
        Severity::Info => "info ".cyan().bold().to_string(),
        Severity::Suggestion => "tip  ".green().bold().to_string(),
    };
    eprintln!("  {} {}: {}", prefix, issue.field.dimmed(), issue.message);
    if let Some(fix) = &issue.fix {
        eprintln!("       {} {}", "fix:".bold(), fix.label);
    }
}

/// Render a full BuildResult as a CLI summary.
/// Returns true if the build passed (no errors), false otherwise.
pub fn render_build_result(result: &BuildResult) -> bool {
    let errors = result.errors();
    let non_blocking = result.non_blocking();

    if errors.is_empty() && non_blocking.is_empty() {
        eprintln!("  {} manifest looks clean.", "✓".green().bold());
        return true;
    }

    if !errors.is_empty() {
        eprintln!("\n{}", "Errors:".red().bold());
        for issue in &errors {
            render_issue(issue);
        }
    }

    if !non_blocking.is_empty() {
        eprintln!("\n{}", "Recommendations:".yellow().bold());
        for issue in &non_blocking {
            render_issue(issue);
        }
    }

    errors.is_empty()
}

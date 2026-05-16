//! `abw app new <slug>` — scaffold a new App directory.

use super::Ctx;
use anyhow::{anyhow, Context, Result};
use clap::Args as ClapArgs;
use colored::*;
use abw_apps_core::{default_name_from_slug, validate_slug};
use std::path::PathBuf;

const MANIFEST_TEMPLATE: &str = include_str!("../../templates/manifest.json");
const README_TEMPLATE: &str = include_str!("../../templates/README.md");
const ENV_EXAMPLE: &str = include_str!("../../templates/.env.example");
const GITIGNORE: &str = include_str!("../../templates/.gitignore");

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Slug for the new App. Lowercase letters, digits, underscores, 3–64
    /// chars, starting with a letter. Must not collide with a reserved
    /// platform origin (`rabble_swarm`, `bestiary_workspace`, etc.).
    pub slug: String,

    /// Directory to create. Defaults to the slug.
    #[arg(long)]
    pub dir: Option<PathBuf>,

    /// Pre-fill the App display name. Defaults to a Title Cased slug.
    #[arg(long)]
    pub name: Option<String>,

    /// Pre-fill the one-line tagline.
    #[arg(long)]
    pub tagline: Option<String>,

    /// Pre-fill the longer description.
    #[arg(long)]
    pub description: Option<String>,

    /// Overwrite an existing directory if it already exists.
    #[arg(long)]
    pub force: bool,
}

pub async fn run(ctx: &Ctx, args: Args) -> Result<()> {
    // Validate the slug up front — better to fail before creating directories.
    validate_slug(&args.slug)
        .map_err(|msg| anyhow!("invalid slug: {}", msg))?;

    let dir = args.dir.clone().unwrap_or_else(|| PathBuf::from(&args.slug));
    if dir.exists() {
        if !args.force {
            return Err(anyhow!(
                "{} already exists. Pass --force to overwrite, or pick a different --dir.",
                dir.display()
            ));
        }
        // Refuse to nuke a directory that isn't ours — only overwrite if it
        // looks like a previous abw-cli scaffold (manifest.json present).
        if !dir.join("manifest.json").exists() {
            return Err(anyhow!(
                "{} exists but doesn't look like an abw-cli scaffold (no manifest.json). \
                 Refusing to overwrite to be safe. Move or delete it manually.",
                dir.display()
            ));
        }
    }

    let name = args.name.unwrap_or_else(|| default_name_from_slug(&args.slug));
    let tagline = args.tagline.unwrap_or_else(|| String::new());
    let description = args.description.unwrap_or_else(|| String::new());

    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating {}", dir.display()))?;

    let manifest = MANIFEST_TEMPLATE
        .replace("{{SLUG}}", &args.slug)
        .replace("{{NAME}}", &name)
        .replace("{{TAGLINE}}", &escape_json_string(&tagline))
        .replace("{{DESCRIPTION}}", &escape_json_string(&description));

    let readme = README_TEMPLATE
        .replace("{{SLUG}}", &args.slug)
        .replace("{{NAME}}", &name);

    write_file(dir.join("manifest.json"), &manifest)?;
    write_file(dir.join("README.md"), &readme)?;
    write_file(dir.join(".env.example"), ENV_EXAMPLE)?;
    write_file(dir.join(".gitignore"), GITIGNORE)?;

    if !ctx.quiet {
        println!();
        println!("  {} Created {}", "✓".green().bold(), dir.display().to_string().bold());
        println!();
        println!("  Files:");
        println!("    {} manifest.json      — App manifest", "·".dimmed());
        println!("    {} README.md          — quick reference", "·".dimmed());
        println!("    {} .env.example       — env var template", "·".dimmed());
        println!("    {} .gitignore", "·".dimmed());
        println!();
        println!("  Next steps:");
        println!("    {} cd {}", ">".cyan().bold(), dir.display());
        println!("    {} $EDITOR manifest.json", ">".cyan().bold());
        println!("    {} abw app validate", ">".cyan().bold());
        println!("    {} abw app deploy", ">".cyan().bold());
        println!();
    }
    Ok(())
}

fn write_file(path: PathBuf, content: &str) -> Result<()> {
    std::fs::write(&path, content)
        .with_context(|| format!("writing {}", path.display()))
}

/// Minimal JSON-string escaping for substitution into the template.
/// The template fields are already inside quoted strings, so we only
/// need to escape \, ", and newlines.
fn escape_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

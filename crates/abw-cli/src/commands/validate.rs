//! `abw app validate` — run the platform's manifest validator locally.
//!
//! No network calls. Same validation the server will run on deploy, so any
//! issues surfaced here mean deploy will reject too. Exits non-zero on
//! `Error`-severity issues; `Warning`/`Suggestion`/`Info` are advisory.

use super::Ctx;
use abw_apps_core::build_manifest;
use anyhow::{anyhow, Result};
use clap::Args as ClapArgs;
use colored::*;
use std::path::PathBuf;

use crate::manifest_io::{find_manifest, read_manifest, render_build_result};

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Path to manifest.json. Defaults to a search of CWD and its parents.
    #[arg(long)]
    pub manifest: Option<PathBuf>,

    /// Suppress non-error output (errors still print). Useful in CI.
    #[arg(long)]
    pub errors_only: bool,
}

pub async fn run(ctx: &Ctx, args: Args) -> Result<()> {
    let path = find_manifest(args.manifest.as_deref())?;
    if !ctx.quiet {
        eprintln!(
            "  {} {}",
            "Validating".bold(),
            path.display().to_string().dimmed()
        );
    }

    let partial = read_manifest(&path)?;
    let result = build_manifest(partial);

    let passed = if args.errors_only {
        // Print only errors; nothing else.
        let errors = result.errors();
        for issue in &errors {
            crate::manifest_io::render_issue(issue);
        }
        errors.is_empty()
    } else {
        render_build_result(&result)
    };

    if !passed {
        return Err(anyhow!(
            "manifest has blocking errors — fix and re-run `abw app validate`"
        ));
    }
    Ok(())
}

//! `abw app deploy` — validate + POST/PUT to /api/apps.
//!
//! If the slug already exists, this is treated as an update via PUT. Otherwise
//! the App is created via POST. The CLI doesn't try to be clever about diffs:
//! the full manifest is sent every time, which is the same shape the auto-seed
//! path uses.

use super::Ctx;
use anyhow::{anyhow, Context, Result};
use clap::Args as ClapArgs;
use colored::*;
use abw_apps_core::build_manifest;
use std::path::PathBuf;

use crate::config::resolve_api_key;
use crate::manifest_io::{find_manifest, read_manifest, render_build_result};

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Path to manifest.json. Defaults to a search of CWD and its parents.
    #[arg(long)]
    pub manifest: Option<PathBuf>,

    /// Skip the local validation step (the server still validates).
    /// Mostly useful if the CLI is older than the server and the local
    /// rules are out of sync.
    #[arg(long)]
    pub skip_local_validation: bool,

    /// Don't actually POST; just print what would be sent.
    #[arg(long)]
    pub dry_run: bool,
}

pub async fn run(ctx: &Ctx, args: Args) -> Result<()> {
    let path = find_manifest(args.manifest.as_deref())?;
    let partial = read_manifest(&path)?;

    // Step 1: local validation.
    let result = build_manifest(partial.clone());
    if !args.skip_local_validation {
        if !ctx.quiet {
            eprintln!("  {} {}", "Validating".bold(), path.display().to_string().dimmed());
        }
        let passed = render_build_result(&result);
        if !passed {
            return Err(anyhow!(
                "deploy aborted — manifest has blocking errors. Pass --skip-local-validation \
                 only if you're sure the server-side rules differ."
            ));
        }
    }

    let manifest = result
        .manifest
        .ok_or_else(|| anyhow!("manifest builder returned no manifest (this is a bug)"))?;

    let slug = manifest["slug"]
        .as_str()
        .ok_or_else(|| anyhow!("manifest is missing slug after build"))?
        .to_string();

    if args.dry_run {
        println!("{}", "Dry run — would send:".bold());
        println!("{}", serde_json::to_string_pretty(&manifest)?);
        return Ok(());
    }

    let api_key = resolve_api_key()?;
    let http = ctx.http();

    // Probe: does this slug already exist?
    let probe_url = ctx.url(&format!("/api/apps/{}", slug));
    let probe = http
        .get(&probe_url)
        .bearer_auth(&api_key)
        .send()
        .await
        .with_context(|| format!("probing {}", probe_url))?;

    let exists = match probe.status().as_u16() {
        200 => true,
        404 => false,
        401 | 403 => {
            return Err(anyhow!(
                "authentication failed (status {}). Try `abw logout && abw login`.",
                probe.status()
            ));
        }
        other => {
            let body = probe.text().await.unwrap_or_default();
            return Err(anyhow!("unexpected status {} probing existence: {}", other, body));
        }
    };

    let (method, url, expect_status) = if exists {
        ("PUT", ctx.url(&format!("/api/apps/{}", slug)), 200u16)
    } else {
        ("POST", ctx.url("/api/apps"), 201u16)
    };

    if !ctx.quiet {
        let verb = if exists { "Updating".yellow() } else { "Registering".green() }.bold();
        eprintln!("  {} App '{}' at {}", verb, slug.bold(), ctx.base_url.dimmed());
    }

    let req = match method {
        "PUT" => http.put(&url),
        _ => http.post(&url),
    };

    let resp = req
        .bearer_auth(&api_key)
        .json(&manifest)
        .send()
        .await
        .with_context(|| format!("sending {} {}", method, url))?;

    let status = resp.status();
    let body_text = resp.text().await.unwrap_or_default();

    if status.as_u16() != expect_status {
        return Err(anyhow!(
            "server returned {} (expected {}): {}",
            status,
            expect_status,
            body_text
        ));
    }

    if !ctx.quiet {
        println!();
        println!("  {} App {} {} on {}", "✓".green().bold(), slug.bold(), if exists { "updated" } else { "registered" }, ctx.base_url.dimmed());
        println!();
        println!("  Next steps:");
        println!("    {} {}/apps/{}", "Catalogue:".bold(), ctx.base_url, slug);
        println!("    {} abw app spawn {}", "Spawn:    ".bold(), slug);
        println!();
    }
    Ok(())
}

//! `abw app publish <slug>` — promote an App's visibility to `public`.
//!
//! Maps directly to POST /api/apps/:slug/publish on the server. Useful
//! immediately after `abw app deploy` (which defaults to private) when
//! you want the App to show up in the catalogue.

use super::Ctx;
use anyhow::{anyhow, Context, Result};
use clap::Args as ClapArgs;
use colored::*;

use crate::config::resolve_api_key;

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Slug of the App to publish.
    pub slug: String,
}

pub async fn run(ctx: &Ctx, args: Args) -> Result<()> {
    let api_key = resolve_api_key()?;
    let http = ctx.http();
    let url = ctx.url(&format!("/api/apps/{}/publish", args.slug));

    if !ctx.quiet {
        eprintln!(
            "  {} '{}' to {}",
            "Publishing".bold(),
            args.slug.bold(),
            ctx.base_url.dimmed()
        );
    }

    let resp = http
        .post(&url)
        .bearer_auth(&api_key)
        .send()
        .await
        .with_context(|| format!("POST {}", url))?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        return Err(anyhow!("server returned {}: {}", status, body));
    }

    if !ctx.quiet {
        println!();
        println!(
            "  {} App '{}' is now {}",
            "✓".green().bold(),
            args.slug.bold(),
            "public".green().bold()
        );
        println!(
            "  {} {}/apps/{}",
            "Catalogue:".bold(),
            ctx.base_url,
            args.slug
        );
        println!();
    }
    Ok(())
}

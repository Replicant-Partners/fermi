//! `abw app spawn <slug>` — spawn a workspace from a deployed App.

use super::Ctx;
use anyhow::{anyhow, Context, Result};
use clap::Args as ClapArgs;
use colored::*;
use serde_json::json;

use crate::config::resolve_api_key;

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Slug of the App to spawn a workspace from.
    pub slug: String,

    /// Optional workspace name. Defaults to the App's default_name_pattern.
    #[arg(long)]
    pub name: Option<String>,

    /// Extra credits to deposit on the workspace beyond initial_budget.
    #[arg(long)]
    pub extra_budget: Option<i32>,

    /// Open the workspace URL in a browser after spawning.
    #[arg(long)]
    pub open: bool,
}

pub async fn run(ctx: &Ctx, args: Args) -> Result<()> {
    let api_key = resolve_api_key()?;
    let http = ctx.http();
    let url = ctx.url(&format!("/api/apps/{}/workspaces", args.slug));

    let mut body = serde_json::Map::new();
    if let Some(name) = &args.name {
        body.insert("name".into(), json!(name));
    }
    if let Some(eb) = args.extra_budget {
        body.insert("extra_budget".into(), json!(eb));
    }

    if !ctx.quiet {
        eprintln!("  {} workspace from App '{}'…", "Spawning".bold(), args.slug.bold());
    }

    let resp = http
        .post(&url)
        .bearer_auth(&api_key)
        .json(&json!(body))
        .send()
        .await
        .with_context(|| format!("POST {}", url))?;

    let status = resp.status();
    let body_text = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        return Err(anyhow!(
            "server returned {}: {}",
            status,
            body_text
        ));
    }

    let value: serde_json::Value = serde_json::from_str(&body_text)
        .with_context(|| format!("parsing response body: {}", body_text))?;
    let workspace_id = value
        .get("workspace_id")
        .or_else(|| value.get("id"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("response missing workspace_id: {}", body_text))?;

    let workspace_url = format!("{}/workspace/{}", ctx.base_url, workspace_id);

    if !ctx.quiet {
        println!();
        println!("  {} Workspace {} created", "✓".green().bold(), workspace_id.bold());
        println!("  {} {}", "URL:".bold(), workspace_url);
        println!();
    } else {
        println!("{}", workspace_url);
    }

    if args.open {
        // open::that returns Result<()>; treat failure as non-fatal.
        if let Err(e) = open::that(&workspace_url) {
            eprintln!("  {} could not open browser: {}", "warn".yellow().bold(), e);
        }
    }
    Ok(())
}

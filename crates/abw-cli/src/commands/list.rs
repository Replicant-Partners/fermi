//! `abw app list` — list Apps registered on the platform.
//!
//! Defaults to the caller's own Apps (any visibility); --public shows the
//! public catalogue instead. Authenticated calls also see the caller's
//! private/unlisted Apps regardless of which filter is set.

use super::Ctx;
use anyhow::{anyhow, Context, Result};
use clap::Args as ClapArgs;
use colored::*;
use serde_json::Value;

use crate::config::resolve_api_key;

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Show only public Apps (the catalogue view). Without this, lists
    /// every App you own at any visibility.
    #[arg(long)]
    pub public: bool,

    /// Include archived Apps in the listing (default: hide them).
    #[arg(long)]
    pub include_archived: bool,

    /// Filter by owner user_id (admin-style filter; pairs with `--public`
    /// to find someone else's public Apps).
    #[arg(long)]
    pub owner: Option<String>,

    /// Print one slug per line, machine-readable. Useful in shell pipelines.
    #[arg(long)]
    pub slugs_only: bool,
}

pub async fn run(ctx: &Ctx, args: Args) -> Result<()> {
    // Auth is optional for /api/apps reads, but if we have a key we send it
    // so the response includes the caller's private/unlisted Apps too.
    let api_key = resolve_api_key().ok();

    let mut params: Vec<String> = Vec::new();
    if args.public {
        params.push("visibility=public".into());
    }
    if args.include_archived {
        params.push("include_archived=true".into());
    }
    if let Some(o) = &args.owner {
        params.push(format!("owner={}", urlencode(o)));
    }
    let query = if params.is_empty() {
        String::new()
    } else {
        format!("?{}", params.join("&"))
    };

    let url = ctx.url(&format!("/api/apps{}", query));
    let http = ctx.http();
    let mut req = http.get(&url);
    if let Some(key) = &api_key {
        req = req.bearer_auth(key);
    }

    let resp = req.send().await.with_context(|| format!("GET {}", url))?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        return Err(anyhow!("server returned {}: {}", status, body));
    }

    let value: Value = serde_json::from_str(&body)
        .with_context(|| format!("parsing response: {}", body))?;
    let apps = value
        .get("apps")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    if args.slugs_only {
        for app in &apps {
            if let Some(slug) = app.get("slug").and_then(|v| v.as_str()) {
                println!("{}", slug);
            }
        }
        return Ok(());
    }

    if apps.is_empty() {
        if !ctx.quiet {
            println!();
            let scope = if args.public { "public Apps" } else { "Apps registered" };
            println!("  {} No {} match this query.", "·".dimmed(), scope);
            if !args.public && api_key.is_none() {
                println!("  {} Run `abw login` to see your own private Apps.", "tip:".green().bold());
            }
            println!();
        }
        return Ok(());
    }

    if !ctx.quiet {
        let scope = if args.public { "Public catalogue" } else { "Your Apps" };
        println!();
        println!("  {} {} ({}):", "★".yellow().bold(), scope.bold(), apps.len());
        println!();
    }

    // Render each app on three lines (slug, name+visibility, tagline+homepage)
    for app in &apps {
        let slug = app.get("slug").and_then(|v| v.as_str()).unwrap_or("(no-slug)");
        let name = app.get("name").and_then(|v| v.as_str()).unwrap_or(slug);
        let visibility = app
            .get("visibility")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let tagline = app.get("tagline").and_then(|v| v.as_str());
        let homepage = app.get("homepage_url").and_then(|v| v.as_str());
        let owner = app.get("owner_user_id").and_then(|v| v.as_str()).unwrap_or("?");

        let vis_pill = match visibility {
            "public" => visibility.green().to_string(),
            "unlisted" => visibility.yellow().to_string(),
            "private" => visibility.dimmed().to_string(),
            _ => visibility.into(),
        };

        println!("  {}  {}", slug.bold(), format!("[{}]", vis_pill).dimmed());
        println!("       {}  {}", name, format!("owner: {}", owner).dimmed());
        if let Some(t) = tagline {
            if !t.is_empty() {
                println!("       {}", t.dimmed());
            }
        }
        if let Some(h) = homepage {
            if !h.is_empty() {
                println!("       {}", h.dimmed());
            }
        }
        println!();
    }
    Ok(())
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        let c = b as char;
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
            out.push(c);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

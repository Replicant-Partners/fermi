//! `abw admin` — platform-admin operations.
//!
//! Currently one subcommand:
//!
//!   abw admin agents legacy-slugs           # dry-run audit
//!   abw admin agents legacy-slugs --apply   # execute rename + JSONB backfill
//!
//! Requires a platform-admin API key. The server-side handler rejects
//! non-admin callers with 403 before doing any work.

use anyhow::{anyhow, Context, Result};
use clap::{Args as ClapArgs, Subcommand};
use colored::*;
use serde_json::Value;

use super::Ctx;
use crate::config::resolve_api_key;

#[derive(Subcommand, Debug)]
pub enum AdminCmd {
    /// Admin operations against the `agents` table.
    #[command(subcommand)]
    Agents(AgentsCmd),
}

#[derive(Subcommand, Debug)]
pub enum AgentsCmd {
    /// Audit or rename agents whose names predate the platform slug
    /// rule (contain `-` or `/` and are therefore un-routable at
    /// /agent/<name>). Without `--apply`, prints a dry-run report.
    /// With `--apply`, renames each non-colliding agent in a
    /// transaction and backfills fermi_forecasts.agents_used JSONB.
    LegacySlugs(LegacySlugsArgs),
}

#[derive(ClapArgs, Debug)]
pub struct LegacySlugsArgs {
    /// Execute the rename. Without this flag, the command is
    /// audit-only and no rows change. Every rename lands in
    /// admin_bypass_events with the old → new mapping.
    #[arg(long)]
    pub apply: bool,

    /// Print the raw JSON response instead of the pretty table.
    /// Useful when chaining into `jq`.
    #[arg(long)]
    pub json: bool,
}

pub async fn run(ctx: &Ctx, cmd: AdminCmd) -> Result<()> {
    match cmd {
        AdminCmd::Agents(AgentsCmd::LegacySlugs(args)) => legacy_slugs(ctx, args).await,
    }
}

async fn legacy_slugs(ctx: &Ctx, args: LegacySlugsArgs) -> Result<()> {
    let api_key = resolve_api_key()
        .context("this command requires authentication — run `abw login` first")?;

    let path = if args.apply {
        "/api/admin/agents/legacy-slugs?apply=true"
    } else {
        "/api/admin/agents/legacy-slugs"
    };
    let url = ctx.url(path);

    // POST when applying (so it's obvious in server logs that this
    // is a mutation), GET otherwise (matches REST semantics for a
    // pure audit).
    let http = ctx.http();
    let req = if args.apply {
        http.post(&url).bearer_auth(&api_key)
    } else {
        http.get(&url).bearer_auth(&api_key)
    };

    let resp = req
        .send()
        .await
        .with_context(|| format!("{} {}", if args.apply { "POST" } else { "GET" }, url))?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        return Err(anyhow!("server returned {}: {}", status, body));
    }

    if args.json {
        println!("{}", body);
        return Ok(());
    }

    let value: Value =
        serde_json::from_str(&body).with_context(|| format!("parsing response: {}", body))?;

    render_report(ctx, &value, args.apply);
    Ok(())
}

fn render_report(ctx: &Ctx, report: &Value, applied: bool) {
    let total_legacy = report
        .get("total_legacy")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let would_rename = report
        .get("would_rename")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let collisions = report
        .get("collisions")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let entries = report
        .get("entries")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    if !ctx.quiet {
        println!();
        let title = if applied {
            "Legacy-slug rename — APPLIED"
        } else {
            "Legacy-slug audit — DRY RUN"
        };
        let title_col = if applied {
            title.green().bold()
        } else {
            title.yellow().bold()
        };
        println!("  {} {}", "★".yellow().bold(), title_col);
        println!();
    }

    if total_legacy == 0 {
        if !ctx.quiet {
            println!(
                "  {} No legacy-shape agent names in the DB. Nothing to do.",
                "·".dimmed()
            );
            println!();
        }
        return;
    }

    // Column widths derived from the actual entry set — nicer than a
    // fixed layout when the longest name might be 8 chars or 50.
    let name_width = entries
        .iter()
        .filter_map(|e| e.get("old_name").and_then(|v| v.as_str()).map(|s| s.len()))
        .max()
        .unwrap_or(20)
        .clamp(20, 60);
    let new_width = entries
        .iter()
        .filter_map(|e| {
            e.get("proposed_new_name")
                .and_then(|v| v.as_str())
                .map(|s| s.len())
        })
        .max()
        .unwrap_or(20)
        .clamp(20, 60);

    println!(
        "  {}",
        format!(
            "  {:name_w$}   →   {:new_w$}   {:>6}   {}",
            "OLD_NAME",
            "PROPOSED_NEW_NAME",
            "REFS",
            "STATUS",
            name_w = name_width,
            new_w = new_width,
        )
        .dimmed()
    );
    println!(
        "  {}",
        "  ".to_string() + &"─".repeat(name_width + new_width + 30)
    );

    for entry in &entries {
        let old = entry
            .get("old_name")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let new = entry
            .get("proposed_new_name")
            .and_then(|v| v.as_str())
            .unwrap_or("—");
        let refs = entry
            .get("forecast_refs")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let action = entry
            .get("action_taken")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let collision = entry.get("collision").and_then(|v| v.as_str());

        let status_col = match action {
            "renamed" => "renamed".green().to_string(),
            "audit_only" => "audit".dimmed().to_string(),
            "skipped:collision" => "SKIPPED (collision)".red().to_string(),
            "skipped:unrecoverable" => "SKIPPED (manual rename needed)".red().to_string(),
            _ => action.into(),
        };

        println!(
            "    {:name_w$}   →   {:new_w$}   {:>6}   {}",
            old,
            new,
            refs,
            status_col,
            name_w = name_width,
            new_w = new_width,
        );
        if let Some(msg) = collision {
            println!(
                "    {:name_w$}       {}",
                "",
                format!("  ↳ {}", msg).red().dimmed(),
                name_w = name_width,
            );
        }
    }

    println!();

    // Summary line.
    if applied {
        let applied_n = report.get("applied").and_then(|v| v.as_u64()).unwrap_or(0);
        let skipped_c = report
            .get("skipped_collisions")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let skipped_u = report
            .get("skipped_unrecoverable")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        println!(
            "  {} {} renamed, {} skipped (collisions), {} skipped (unrecoverable), out of {} legacy names.",
            "summary:".bold(),
            applied_n.to_string().green(),
            skipped_c.to_string().yellow(),
            skipped_u.to_string().yellow(),
            total_legacy,
        );
    } else {
        println!(
            "  {} {} legacy names, {} would rename cleanly, {} blocked by collisions.",
            "summary:".bold(),
            total_legacy.to_string().yellow(),
            would_rename.to_string().green(),
            collisions.to_string().red(),
        );
        if would_rename > 0 {
            println!(
                "  {} run with `--apply` to execute the rename in a transaction.",
                "tip:".green().bold()
            );
        }
        if collisions > 0 {
            println!(
                "  {} collisions must be resolved manually (rename the target agent or the source).",
                "note:".yellow().bold()
            );
        }
    }
    println!();
}

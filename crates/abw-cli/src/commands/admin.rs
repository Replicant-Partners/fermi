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

    /// Audit or DELETE orphan test-fixture agents (rows with names
    /// like `test_agent_<uuid>` that accumulate from unclosed test
    /// runs). Without `--apply`, prints a dry-run report. With
    /// `--apply`, deletes each row in a transaction (cascades to
    /// episodes / entities / facts / etc.) and logs a snapshot to
    /// admin_bypass_events. Safety criteria — always enforced,
    /// server-side — protect real agents from accidental deletion:
    ///   * total_executions = 0 (never ran real workload)
    ///   * created_at older than the grace period (default 24h)
    ///   * tier not in ('curated', 'system')
    ///   * visibility != 'public' and status != 'published'
    CleanupTestCruft(CleanupTestCruftArgs),
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

    /// Restrict the audit / apply to agents whose current name starts
    /// with this prefix. Essential for targeted rewrites when the
    /// full legacy set is large or mixed (e.g. `--prefix efra-ai/`
    /// to touch only Mario's real EFRA agents, leaving `test_agent_*`
    /// fixtures alone). Pushed down to SQL via LIKE, so it's fast.
    #[arg(long)]
    pub prefix: Option<String>,

    /// Cap the batch size (post-filter). Safe with `--apply` to keep
    /// each run well below the 60-second client timeout — apply runs
    /// ~3 statements per rename in one transaction, so 574 in one
    /// go blew past the timeout on Ivan's first attempt. Use e.g.
    /// `--limit 50` and repeat until the report says `truncated=false`.
    #[arg(long)]
    pub limit: Option<usize>,
}

#[derive(ClapArgs, Debug)]
pub struct CleanupTestCruftArgs {
    /// Execute the DELETE. Without this flag, the command is
    /// audit-only and no rows change. Every deletion lands in
    /// admin_bypass_events with a full row snapshot.
    #[arg(long)]
    pub apply: bool,

    /// Print the raw JSON response instead of the pretty table.
    #[arg(long)]
    pub json: bool,

    /// Name prefix identifying test cruft. Default `test_agent_`
    /// matches the fixture pattern in `agent-bestiary/memory/src/`.
    #[arg(long)]
    pub prefix: Option<String>,

    /// Grace period in hours — rows created within this window are
    /// protected. Default 24. Set to 0 for an aggressive sweep.
    #[arg(long)]
    pub older_than_hours: Option<i64>,

    /// Cap batch size (post-filter). Cleanup runs 2 statements per
    /// row in one transaction (audit INSERT + DELETE), so this is
    /// safety against the 60s client timeout on very large batches.
    #[arg(long)]
    pub limit: Option<usize>,
}

pub async fn run(ctx: &Ctx, cmd: AdminCmd) -> Result<()> {
    match cmd {
        AdminCmd::Agents(AgentsCmd::LegacySlugs(args)) => legacy_slugs(ctx, args).await,
        AdminCmd::Agents(AgentsCmd::CleanupTestCruft(args)) => cleanup_test_cruft(ctx, args).await,
    }
}

async fn legacy_slugs(ctx: &Ctx, args: LegacySlugsArgs) -> Result<()> {
    let api_key = resolve_api_key()
        .context("this command requires authentication — run `abw login` first")?;

    // Build the query string. `apply` toggles the mutation path;
    // `prefix` and `limit` are v0.10.24 filter/batch controls.
    let mut params: Vec<String> = Vec::new();
    if args.apply {
        params.push("apply=true".into());
    }
    if let Some(p) = &args.prefix {
        params.push(format!("prefix={}", urlencode(p)));
    }
    if let Some(n) = args.limit {
        params.push(format!("limit={}", n));
    }
    let query = if params.is_empty() {
        String::new()
    } else {
        format!("?{}", params.join("&"))
    };
    let path = format!("/api/admin/agents/legacy-slugs{}", query);
    let url = ctx.url(&path);

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
    // v0.10.24 metadata for the filter/batch controls.
    let total_matched = report
        .get("total_matched")
        .and_then(|v| v.as_u64())
        .unwrap_or(total_legacy);
    let truncated = report
        .get("truncated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let prefix = report.get("prefix").and_then(|v| v.as_str());
    let limit = report.get("limit").and_then(|v| v.as_u64());

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
        // Surface the active filter/batch controls so the caller
        // knows the scope of this run at a glance.
        if let Some(p) = prefix {
            if !p.is_empty() {
                println!("  {} prefix filter: {}", "⤷".dimmed(), p.cyan());
            }
        }
        if let Some(n) = limit {
            println!(
                "  {} limit: {} (this batch)",
                "⤷".dimmed(),
                n.to_string().cyan()
            );
        }
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
            "  {} {} renamed, {} skipped (collisions), {} skipped (unrecoverable), out of {} legacy names in this batch.",
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

    // v0.10.24: surface the truncation state so multi-batch runs
    // are obvious. When `--limit` clipped the tail, tell the caller
    // how many more legacy names remain outside the batch and how
    // to fetch the next one.
    if truncated {
        let remaining = total_matched.saturating_sub(total_legacy);
        println!(
            "  {} this batch shows {} of {} matching legacy names (remaining: {}).",
            "batch:".cyan().bold(),
            total_legacy,
            total_matched,
            remaining.to_string().yellow(),
        );
        println!(
            "  {} re-run with the same flags to pick up the next batch (same deterministic order).",
            "tip:".green().bold()
        );
    }
    println!();
}

async fn cleanup_test_cruft(ctx: &Ctx, args: CleanupTestCruftArgs) -> Result<()> {
    let api_key = resolve_api_key()
        .context("this command requires authentication — run `abw login` first")?;

    let mut params: Vec<String> = Vec::new();
    if args.apply {
        params.push("apply=true".into());
    }
    if let Some(p) = &args.prefix {
        params.push(format!("prefix={}", urlencode(p)));
    }
    if let Some(h) = args.older_than_hours {
        params.push(format!("older_than_hours={}", h));
    }
    if let Some(n) = args.limit {
        params.push(format!("limit={}", n));
    }
    let query = if params.is_empty() {
        String::new()
    } else {
        format!("?{}", params.join("&"))
    };
    let path = format!("/api/admin/agents/cleanup-test-cruft{}", query);
    let url = ctx.url(&path);

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
    render_cleanup_report(ctx, &value, args.apply);
    Ok(())
}

fn render_cleanup_report(ctx: &Ctx, report: &Value, applied: bool) {
    let total_matched = report
        .get("total_matched")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let in_this_batch = report
        .get("in_this_batch")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let deleted = report.get("deleted").and_then(|v| v.as_u64()).unwrap_or(0);
    let truncated = report
        .get("truncated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let prefix = report
        .get("prefix")
        .and_then(|v| v.as_str())
        .unwrap_or("test_agent_");
    let grace = report
        .get("older_than_hours")
        .and_then(|v| v.as_i64())
        .unwrap_or(24);
    let entries = report
        .get("entries")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let failures = report
        .get("failures")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    if !ctx.quiet {
        println!();
        let title = if applied {
            "Test-cruft cleanup — APPLIED"
        } else {
            "Test-cruft cleanup — DRY RUN"
        };
        let title_col = if applied {
            title.green().bold()
        } else {
            title.yellow().bold()
        };
        println!("  {} {}", "★".yellow().bold(), title_col);
        println!(
            "  {} prefix filter: {}   |   grace period: {}h",
            "⤷".dimmed(),
            prefix.cyan(),
            grace.to_string().cyan()
        );
        println!();
    }

    if in_this_batch == 0 {
        if !ctx.quiet {
            println!(
                "  {} No orphan test-fixture rows matching criteria. Clean.",
                "·".dimmed()
            );
            println!();
        }
        return;
    }

    // Show a compact preview: first 10, last 5 if the batch is large.
    let n_entries = entries.len();
    let (head, tail): (Vec<&Value>, Vec<&Value>) = if n_entries > 15 {
        (
            entries.iter().take(10).collect(),
            entries
                .iter()
                .rev()
                .take(5)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect(),
        )
    } else {
        (entries.iter().collect(), Vec::new())
    };

    let name_width = entries
        .iter()
        .filter_map(|e| {
            e.get("agent_name")
                .and_then(|v| v.as_str())
                .map(|s| s.len())
        })
        .max()
        .unwrap_or(30)
        .clamp(30, 70);

    println!(
        "  {}",
        format!(
            "  {:name_w$}   {:20}   {:9}   {}",
            "AGENT_NAME",
            "CREATED_AT",
            "TIER",
            "STATUS",
            name_w = name_width,
        )
        .dimmed()
    );
    println!("  {}", "  ".to_string() + &"─".repeat(name_width + 45));

    for entry in &head {
        render_row(entry, name_width, applied);
    }
    if !tail.is_empty() {
        println!(
            "    {} ({} more)",
            "…".dimmed(),
            (n_entries - head.len() - tail.len()).to_string().dimmed()
        );
        for entry in &tail {
            render_row(entry, name_width, applied);
        }
    }
    println!();

    // Summary.
    if applied {
        println!(
            "  {} {} deleted out of {} in this batch (total matched: {}).",
            "summary:".bold(),
            deleted.to_string().green(),
            in_this_batch,
            total_matched.to_string().yellow(),
        );
        if !failures.is_empty() {
            println!(
                "  {} {} row(s) failed — see --json for details.",
                "warning:".red().bold(),
                failures.len().to_string().red()
            );
        }
    } else {
        println!(
            "  {} {} orphan rows match cleanup criteria; {} shown in this batch.",
            "summary:".bold(),
            total_matched.to_string().yellow(),
            in_this_batch.to_string().green(),
        );
        println!(
            "  {} run with `--apply` to delete. Cascades to all FK-linked rows (episodes, entities, versions, …).",
            "tip:".green().bold()
        );
    }

    if truncated {
        let remaining = total_matched.saturating_sub(in_this_batch);
        println!(
            "  {} this batch is capped; {} more match beyond it. Re-run to continue.",
            "batch:".cyan().bold(),
            remaining.to_string().yellow()
        );
    }
    println!();
}

fn render_row(entry: &Value, name_width: usize, applied: bool) {
    let name = entry
        .get("agent_name")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let created = entry
        .get("created_at")
        .and_then(|v| v.as_str())
        .map(|s| s.split('T').next().unwrap_or(s).to_string())
        .unwrap_or_else(|| "?".to_string());
    let tier = entry.get("tier").and_then(|v| v.as_str()).unwrap_or("?");
    let action = entry
        .get("action_taken")
        .and_then(|v| v.as_str())
        .unwrap_or("?");

    let status_col = if applied {
        "deleted".green().to_string()
    } else {
        match action {
            "audit_only" => "would delete".yellow().to_string(),
            other => other.dimmed().to_string(),
        }
    };

    println!(
        "    {:name_w$}   {:20}   {:9}   {}",
        name,
        created,
        tier,
        status_col,
        name_w = name_width,
    );
}

/// Minimal percent-encoder for query values. Matches the pattern in
/// commands/list.rs — keeps the surface small and dependency-free.
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

//! `abw workspace` — interact with a workspace: send messages, read/write
//! files, and dispatch the generalised action protocol.
//!
//! Subcommands:
//!
//!   abw workspace message <ws-id> "text" [--agent <id>] [--json]
//!   abw workspace files get <ws-id> <path>
//!   abw workspace files put <ws-id> <path> [--content "..."] [--stdin]
//!   abw workspace actions list <ws-id>
//!   abw workspace actions pending <ws-id>
//!   abw workspace actions accept <ws-id> <action-id> [--content "..."]
//!   abw workspace actions reject <ws-id> <action-id> [--note "..."]
//!   abw workspace actions annotate <ws-id> --kind insight|critique|risk|decision
//!                                          --target "stage:fermentation"
//!                                          "body text"
//!   abw workspace actions mutate <ws-id> --path simops/process.yaml
//!                                        --content @file.yaml [--auto]
//!   abw workspace actions fork <ws-id> --name "co2-capture"
//!                                      --patch '{"stages":[...]}' [--hypothesis "..."]

use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand};
use colored::*;
use serde_json::{json, Value};

use super::Ctx;
use crate::config;

// ─── Top-level workspace subcommand ─────────────────────────────────────────

#[derive(Subcommand, Debug)]
pub enum WorkspaceCmd {
    /// Send a message to the workspace (optionally @mention an agent).
    Message(MessageArgs),
    /// Read and write workspace files.
    #[command(subcommand)]
    Files(FilesCmd),
    /// Workspace action protocol — dispatch, review, and confirm actions.
    #[command(subcommand)]
    Actions(ActionsCmd),
}

pub async fn run(ctx: &Ctx, cmd: WorkspaceCmd) -> Result<()> {
    match cmd {
        WorkspaceCmd::Message(args) => message(ctx, args).await,
        WorkspaceCmd::Files(sub) => files(ctx, sub).await,
        WorkspaceCmd::Actions(sub) => actions(ctx, sub).await,
    }
}

// ─── workspace message ───────────────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct MessageArgs {
    /// Workspace UUID or URL.
    pub workspace: String,

    /// Message text. Use quotes for multi-word messages.
    pub text: String,

    /// Agent to @-mention (e.g. simops_companion). Without this flag,
    /// the message is sent as plain workspace chat.
    #[arg(long, short = 'a')]
    pub agent: Option<String>,

    /// Print the full JSON response instead of just the reply text.
    #[arg(long)]
    pub json: bool,
}

async fn message(ctx: &Ctx, args: MessageArgs) -> Result<()> {
    let api_key = config::resolve_api_key()?;
    let ws_id = resolve_workspace_id(&args.workspace);

    let content = match &args.agent {
        Some(a) => format!("@{} {}", a, args.text),
        None => args.text.clone(),
    };

    let body = json!({
        "content": content,
        "agent": args.agent,
        "message_type": if args.agent.is_some() { "agent_invocation" } else { "chat" },
    });

    let resp = ctx
        .http()
        .post(ctx.url(&format!("/api/workspaces/{}/messages", ws_id)))
        .bearer_auth(&api_key)
        .json(&body)
        .send()
        .await
        .context("sending message")?;

    let status = resp.status();
    let data: Value = resp.json().await.context("parsing response")?;

    if !status.is_success() {
        bail!("server returned {}: {}", status, data);
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&data)?);
    } else {
        // Print just the message_id confirmation
        if let Some(id) = data.get("message_id").and_then(|v| v.as_str()) {
            println!("{} message sent ({})", "✓".green(), id);
        } else {
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
    }
    Ok(())
}

// ─── workspace files ─────────────────────────────────────────────────────────

#[derive(Subcommand, Debug)]
pub enum FilesCmd {
    /// Print a workspace file to stdout.
    Get(FilesGetArgs),
    /// Write a workspace file from --content or stdin.
    Put(FilesPutArgs),
    /// List files in a workspace directory.
    List(FilesListArgs),
}

async fn files(ctx: &Ctx, cmd: FilesCmd) -> Result<()> {
    match cmd {
        FilesCmd::Get(a) => files_get(ctx, a).await,
        FilesCmd::Put(a) => files_put(ctx, a).await,
        FilesCmd::List(a) => files_list(ctx, a).await,
    }
}

#[derive(Args, Debug)]
pub struct FilesGetArgs {
    pub workspace: String,
    pub path: String,
}

async fn files_get(ctx: &Ctx, args: FilesGetArgs) -> Result<()> {
    let api_key = config::resolve_api_key()?;
    let ws_id = resolve_workspace_id(&args.workspace);

    let resp = ctx
        .http()
        .get(ctx.url(&format!(
            "/api/workspaces/{}/files/{}",
            ws_id,
            args.path.trim_start_matches('/')
        )))
        .bearer_auth(&api_key)
        .send()
        .await
        .context("fetching file")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("{}: {}", status, body);
    }

    // Try JSON envelope first (some endpoints wrap in { content })
    let text = resp.text().await.context("reading response")?;
    if let Ok(val) = serde_json::from_str::<Value>(&text) {
        if let Some(content) = val.get("content").and_then(|v| v.as_str()) {
            print!("{}", content);
            return Ok(());
        }
    }
    print!("{}", text);
    Ok(())
}

#[derive(Args, Debug)]
pub struct FilesPutArgs {
    pub workspace: String,
    pub path: String,

    /// File content as a string. Use @filename to read from a file,
    /// or omit to read from stdin.
    #[arg(long, short = 'c')]
    pub content: Option<String>,

    /// Commit message.
    #[arg(long, short = 'm')]
    pub message: Option<String>,

    /// Auto-apply without confirmation (default: true for direct file writes).
    #[arg(long)]
    pub auto: bool,
}

async fn files_put(ctx: &Ctx, args: FilesPutArgs) -> Result<()> {
    let api_key = config::resolve_api_key()?;
    let ws_id = resolve_workspace_id(&args.workspace);
    let content = resolve_content(args.content.as_deref())?;

    // Server route: PUT /api/workspaces/:workspace_id/files/*path
    // Body shape:   { content, is_base64?, message? }   (see WriteFileBody)
    // Previous version used POST /api/workspaces/:id/files (no such route)
    // with body {path, content, commit_message} — silently returned 405 and
    // the CLI's resp.json() saw an empty body ("EOF at line 1 column 0").
    let body = json!({
        "content": content,
        "message": args.message,
    });

    let resp = ctx
        .http()
        .put(ctx.url(&format!(
            "/api/workspaces/{}/files/{}",
            ws_id,
            args.path.trim_start_matches('/')
        )))
        .bearer_auth(&api_key)
        .json(&body)
        .send()
        .await
        .context("writing file")?;

    let status = resp.status();
    if !status.is_success() {
        // Surface the raw body — server errors come back as plain text via
        // (StatusCode, String); JSON-only error parsing would hide them.
        let body = resp.text().await.unwrap_or_default();
        bail!("server returned {}: {}", status, body);
    }

    // Success body: { path, commit: { sha, message, timestamp } }
    let data: Value = resp.json().await.context("parsing response")?;
    let sha = data
        .get("commit")
        .and_then(|c| c.get("sha"))
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let short_sha = &sha[..8.min(sha.len())];
    println!("{} {} @ {}", "✓".green(), args.path, short_sha);
    Ok(())
}

#[derive(Args, Debug)]
pub struct FilesListArgs {
    pub workspace: String,
    #[arg(default_value = "")]
    pub prefix: String,
}

async fn files_list(ctx: &Ctx, args: FilesListArgs) -> Result<()> {
    let api_key = config::resolve_api_key()?;
    let ws_id = resolve_workspace_id(&args.workspace);

    let url = if args.prefix.is_empty() {
        ctx.url(&format!("/api/workspaces/{}/files", ws_id))
    } else {
        ctx.url(&format!("/api/workspaces/{}/files/{}", ws_id, args.prefix))
    };

    let resp = ctx
        .http()
        .get(&url)
        .bearer_auth(&api_key)
        .send()
        .await
        .context("listing files")?;

    let status = resp.status();
    let data: Value = resp.json().await.context("parsing response")?;

    if !status.is_success() {
        bail!("server returned {}: {}", status, data);
    }

    let files = data
        .get("files")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    for f in &files {
        let path = f.get("path").and_then(|v| v.as_str()).unwrap_or("?");
        let is_dir = f.get("is_dir").and_then(|v| v.as_bool()).unwrap_or(false);
        if is_dir {
            println!("  {}/", path.blue());
        } else {
            println!("  {}", path);
        }
    }
    if files.is_empty() {
        println!("{}", "(empty)".dimmed());
    }
    Ok(())
}

// ─── workspace actions ───────────────────────────────────────────────────────

#[derive(Subcommand, Debug)]
pub enum ActionsCmd {
    /// List recent actions for this workspace.
    List(ActionsListArgs),
    /// List actions pending human confirmation.
    Pending(ActionsPendingArgs),
    /// Accept a pending action (applies it server-side).
    Accept(ActionsAcceptArgs),
    /// Reject a pending action.
    Reject(ActionsRejectArgs),
    /// Record a typed annotation on the process or a stage.
    Annotate(ActionsAnnotateArgs),
    /// Propose a document mutation (mutate_document action).
    Mutate(ActionsMutateArgs),
    /// Fork a named process variation (fork_state action).
    Fork(ActionsForkArgs),
}

async fn actions(ctx: &Ctx, cmd: ActionsCmd) -> Result<()> {
    match cmd {
        ActionsCmd::List(a) => actions_list(ctx, a).await,
        ActionsCmd::Pending(a) => actions_pending(ctx, a).await,
        ActionsCmd::Accept(a) => actions_accept(ctx, a).await,
        ActionsCmd::Reject(a) => actions_reject(ctx, a).await,
        ActionsCmd::Annotate(a) => actions_annotate(ctx, a).await,
        ActionsCmd::Mutate(a) => actions_mutate(ctx, a).await,
        ActionsCmd::Fork(a) => actions_fork(ctx, a).await,
    }
}

// ─ list ─

#[derive(Args, Debug)]
pub struct ActionsListArgs {
    pub workspace: String,
    #[arg(long)]
    pub json: bool,
}

async fn actions_list(ctx: &Ctx, args: ActionsListArgs) -> Result<()> {
    let api_key = config::resolve_api_key()?;
    let ws_id = resolve_workspace_id(&args.workspace);

    let resp = ctx
        .http()
        .get(ctx.url(&format!("/api/workspaces/{}/actions", ws_id)))
        .bearer_auth(&api_key)
        .send()
        .await?;

    let status = resp.status();
    let data: Value = resp.json().await?;
    if !status.is_success() {
        bail!("{}: {}", status, data);
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&data)?);
        return Ok(());
    }

    let actions = data
        .get("actions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if actions.is_empty() {
        println!("{}", "No actions recorded yet.".dimmed());
        return Ok(());
    }

    for a in &actions {
        let id = a.get("action_id").and_then(|v| v.as_str()).unwrap_or("?");
        let kind = a.get("action_type").and_then(|v| v.as_str()).unwrap_or("?");
        let conf = a
            .get("confirmation")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let applied = a.get("applied").and_then(|v| v.as_bool()).unwrap_or(false);
        let ts = a.get("created_at").and_then(|v| v.as_str()).unwrap_or("?");

        let status_label = match conf {
            "pending" => "pending".yellow().to_string(),
            "accepted" => "accepted".green().to_string(),
            "rejected" => "rejected".red().to_string(),
            "auto" => {
                if applied {
                    "applied".green().to_string()
                } else {
                    "auto".dimmed().to_string()
                }
            }
            other => other.to_string(),
        };

        println!(
            "  {} {} [{}] {} {}",
            &id[..8.min(id.len())],
            kind.cyan(),
            status_label,
            ts.get(..16).unwrap_or(ts),
            if applied {
                "✓".green().to_string()
            } else {
                String::new()
            },
        );
    }
    Ok(())
}

// ─ pending ─

#[derive(Args, Debug)]
pub struct ActionsPendingArgs {
    pub workspace: String,
    #[arg(long)]
    pub json: bool,
}

async fn actions_pending(ctx: &Ctx, args: ActionsPendingArgs) -> Result<()> {
    let api_key = config::resolve_api_key()?;
    let ws_id = resolve_workspace_id(&args.workspace);

    let resp = ctx
        .http()
        .get(ctx.url(&format!("/api/workspaces/{}/actions/pending", ws_id)))
        .bearer_auth(&api_key)
        .send()
        .await?;

    let status = resp.status();
    let data: Value = resp.json().await?;
    if !status.is_success() {
        bail!("{}: {}", status, data);
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&data)?);
        return Ok(());
    }

    let pending = data
        .get("pending")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if pending.is_empty() {
        println!("{}", "No pending actions.".dimmed());
        return Ok(());
    }

    println!(
        "{} pending action(s) awaiting confirmation:\n",
        pending.len()
    );
    for a in &pending {
        let id = a.get("action_id").and_then(|v| v.as_str()).unwrap_or("?");
        let kind = a.get("action_type").and_then(|v| v.as_str()).unwrap_or("?");
        let ts = a.get("created_at").and_then(|v| v.as_str()).unwrap_or("?");
        let by = a
            .get("emitted_by_id")
            .and_then(|v| v.as_str())
            .unwrap_or("?");

        let payload_preview = a
            .get("payload")
            .map(|p| {
                let s = serde_json::to_string(p).unwrap_or_default();
                if s.len() > 80 {
                    format!("{}…", &s[..80])
                } else {
                    s
                }
            })
            .unwrap_or_default();

        println!(
            "  {} {} by {} @ {}",
            &id[..8.min(id.len())],
            kind.cyan(),
            by.dimmed(),
            ts.get(..16).unwrap_or(ts)
        );
        println!("    {}", payload_preview.dimmed());
        println!(
            "  accept: abw workspace actions accept {} {}",
            args.workspace, id
        );
        println!(
            "  reject: abw workspace actions reject {} {}\n",
            args.workspace, id
        );
    }
    Ok(())
}

// ─ accept ─

#[derive(Args, Debug)]
pub struct ActionsAcceptArgs {
    pub workspace: String,
    pub action_id: String,

    /// Final document content to write (for mutate_document actions).
    /// Use @filename to read from a file, or omit to use the patch as-is.
    #[arg(long, short = 'c')]
    pub content: Option<String>,
}

async fn actions_accept(ctx: &Ctx, args: ActionsAcceptArgs) -> Result<()> {
    let api_key = config::resolve_api_key()?;
    let ws_id = resolve_workspace_id(&args.workspace);

    let content = args
        .content
        .as_deref()
        .map(|c| resolve_content(Some(c)))
        .transpose()?;

    let body = json!({ "content": content });

    let resp = ctx
        .http()
        .post(ctx.url(&format!(
            "/api/workspaces/{}/actions/{}/accept",
            ws_id, args.action_id
        )))
        .bearer_auth(&api_key)
        .json(&body)
        .send()
        .await?;

    let status = resp.status();
    let data: Value = resp.json().await?;
    if !status.is_success() {
        bail!("{}: {}", status, data);
    }

    let result = data
        .get("apply_result")
        .map(|v| serde_json::to_string(v).unwrap_or_default())
        .unwrap_or_default();
    println!("{} accepted — {}", "✓".green(), result);
    Ok(())
}

// ─ reject ─

#[derive(Args, Debug)]
pub struct ActionsRejectArgs {
    pub workspace: String,
    pub action_id: String,

    #[arg(long, short = 'n')]
    pub note: Option<String>,
}

async fn actions_reject(ctx: &Ctx, args: ActionsRejectArgs) -> Result<()> {
    let api_key = config::resolve_api_key()?;
    let ws_id = resolve_workspace_id(&args.workspace);

    let body = json!({ "note": args.note });

    let resp = ctx
        .http()
        .post(ctx.url(&format!(
            "/api/workspaces/{}/actions/{}/reject",
            ws_id, args.action_id
        )))
        .bearer_auth(&api_key)
        .json(&body)
        .send()
        .await?;

    let status = resp.status();
    let data: Value = resp.json().await?;
    if !status.is_success() {
        bail!("{}: {}", status, data);
    }

    println!("{} rejected", "✓".green());
    Ok(())
}

// ─ annotate ─

#[derive(Args, Debug)]
pub struct ActionsAnnotateArgs {
    pub workspace: String,

    /// Body text of the annotation.
    pub body: String,

    /// Kind: critique | insight | risk | decision
    #[arg(long, short = 'k', default_value = "insight")]
    pub kind: String,

    /// Target: e.g. "stage:fermentation", "process", "variation:co2-capture"
    #[arg(long, short = 't', default_value = "process")]
    pub target: String,

    /// Severity: info | warn | block
    #[arg(long, short = 's', default_value = "info")]
    pub severity: String,

    /// App schema slug (e.g. kask_simops)
    #[arg(long)]
    pub app_schema: Option<String>,
}

async fn actions_annotate(ctx: &Ctx, args: ActionsAnnotateArgs) -> Result<()> {
    let api_key = config::resolve_api_key()?;
    let ws_id = resolve_workspace_id(&args.workspace);

    let body = json!({
        "kind":       args.kind,
        "target":     args.target,
        "body":       args.body,
        "severity":   args.severity,
        "app_schema": args.app_schema,
    });

    let resp = ctx
        .http()
        .post(ctx.url(&format!("/api/workspaces/{}/actions/annotate", ws_id)))
        .bearer_auth(&api_key)
        .json(&body)
        .send()
        .await?;

    let status = resp.status();
    let data: Value = resp.json().await?;
    if !status.is_success() {
        bail!("{}: {}", status, data);
    }

    let ann_id = data
        .get("annotation_id")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    println!(
        "{} annotation recorded ({}) [{}] → {}",
        "✓".green(),
        &ann_id[..8.min(ann_id.len())],
        args.kind,
        args.target
    );
    Ok(())
}

// ─ mutate ─

#[derive(Args, Debug)]
pub struct ActionsMutateArgs {
    pub workspace: String,

    /// Document path (e.g. simops/process.yaml)
    #[arg(long, short = 'p')]
    pub path: String,

    /// New document content. Use @filename to read from a file, or omit for stdin.
    #[arg(long, short = 'c')]
    pub content: Option<String>,

    /// Rationale for the change.
    #[arg(long, short = 'r')]
    pub rationale: Option<String>,

    /// Apply immediately without confirmation modal.
    #[arg(long)]
    pub auto: bool,

    /// App schema slug.
    #[arg(long)]
    pub app_schema: Option<String>,
}

async fn actions_mutate(ctx: &Ctx, args: ActionsMutateArgs) -> Result<()> {
    let api_key = config::resolve_api_key()?;
    let ws_id = resolve_workspace_id(&args.workspace);

    let content = resolve_content(args.content.as_deref())?;

    let body = json!({
        "path":        args.path,
        "patch":       {},          // full-replace via content
        "content":     content,
        "rationale":   args.rationale,
        "confirmation": if args.auto { "auto" } else { "ask" },
        "app_schema":  args.app_schema,
    });

    let resp = ctx
        .http()
        .post(ctx.url(&format!(
            "/api/workspaces/{}/actions/mutate_document",
            ws_id
        )))
        .bearer_auth(&api_key)
        .json(&body)
        .send()
        .await?;

    let status = resp.status();
    let data: Value = resp.json().await?;
    if !status.is_success() {
        bail!("{}: {}", status, data);
    }

    let action_id = data
        .get("action_id")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let applied = data
        .get("applied")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let confirmation = data
        .get("confirmation")
        .and_then(|v| v.as_str())
        .unwrap_or("?");

    if applied {
        println!(
            "{} {} applied ({})",
            "✓".green(),
            args.path,
            &action_id[..8.min(action_id.len())]
        );
    } else {
        println!(
            "{} {} pending confirmation ({})",
            "⏳".yellow(),
            args.path,
            &action_id[..8.min(action_id.len())]
        );
        println!(
            "  accept: abw workspace actions accept {} {}",
            args.workspace, action_id
        );
        println!(
            "  reject: abw workspace actions reject {} {}",
            args.workspace, action_id
        );
        let _ = confirmation;
    }
    Ok(())
}

// ─ fork ─

#[derive(Args, Debug)]
pub struct ActionsForkArgs {
    pub workspace: String,

    /// Human-readable variation name.
    #[arg(long, short = 'n')]
    pub name: String,

    /// Source variation to fork from (default: base).
    #[arg(long, default_value = "base")]
    pub from: String,

    /// JSON patch to apply over the source. Use @filename or inline JSON.
    #[arg(long, short = 'p')]
    pub patch: Option<String>,

    /// What you expect this variation to achieve.
    #[arg(long, short = 'h')]
    pub hypothesis: Option<String>,

    /// App schema slug.
    #[arg(long)]
    pub app_schema: Option<String>,
}

async fn actions_fork(ctx: &Ctx, args: ActionsForkArgs) -> Result<()> {
    let api_key = config::resolve_api_key()?;
    let ws_id = resolve_workspace_id(&args.workspace);

    let patch: Value = match &args.patch {
        Some(p) => {
            let raw = resolve_content(Some(p))?;
            serde_json::from_str(&raw).context("parsing --patch as JSON")?
        }
        None => json!({}),
    };

    let body = json!({
        "name":       args.name,
        "from":       args.from,
        "patch":      patch,
        "hypothesis": args.hypothesis,
        "app_schema": args.app_schema,
    });

    let resp = ctx
        .http()
        .post(ctx.url(&format!("/api/workspaces/{}/actions/fork_state", ws_id)))
        .bearer_auth(&api_key)
        .json(&body)
        .send()
        .await?;

    let status = resp.status();
    let data: Value = resp.json().await?;
    if !status.is_success() {
        bail!("{}: {}", status, data);
    }

    let slug = data
        .get("variant_slug")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let path = data.get("path").and_then(|v| v.as_str()).unwrap_or("?");
    println!("{} forked '{}' → {}", "✓".green(), slug, path);
    Ok(())
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Strip workspace URL down to just the UUID if someone pastes a full URL.
fn resolve_workspace_id(input: &str) -> String {
    // Accept full URLs like https://agent-bestiary.world/workspace/<uuid>
    if let Some(pos) = input.rfind('/') {
        let tail = &input[pos + 1..];
        if tail.len() == 36 && tail.contains('-') {
            return tail.to_string();
        }
    }
    input.to_string()
}

/// Resolve content from:
///   @filename   — read the file
///   -           — read stdin
///   "literal"   — use as-is
///   None        — read stdin
fn resolve_content(input: Option<&str>) -> Result<String> {
    match input {
        Some(s) if s.starts_with('@') => {
            let path = &s[1..];
            std::fs::read_to_string(path).with_context(|| format!("reading file {}", path))
        }
        Some("-") | None => {
            use std::io::Read;
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .context("reading stdin")?;
            Ok(buf)
        }
        Some(s) => Ok(s.to_string()),
    }
}

//! `abw` — Agent Bestiary Workspace CLI.
//!
//! App lifecycle:
//!   abw login                 # one-time OAuth login
//!   abw app new <slug>        # scaffold a new App directory
//!   abw app validate          # validate manifest locally
//!   abw app deploy            # register App on ABW
//!   abw app spawn <slug>      # spawn a workspace from an App
//!
//! Workspace interaction:
//!   abw workspace message <ws-id> "text" [--agent simops_companion]
//!   abw workspace files get <ws-id> simops/process.yaml
//!   abw workspace files put <ws-id> simops/process.yaml --content @file.yaml
//!   abw workspace actions list <ws-id>
//!   abw workspace actions pending <ws-id>
//!   abw workspace actions accept <ws-id> <action-id>
//!   abw workspace actions reject <ws-id> <action-id>
//!   abw workspace actions annotate <ws-id> --kind insight --target process "note"
//!   abw workspace actions mutate <ws-id> --path simops/process.yaml --content @new.yaml
//!   abw workspace actions fork <ws-id> --name "co2-capture" --patch '{"stages":[...]}'
//!
//! See `crates/abw-cli/README.md` for the full surface.

use clap::{Parser, Subcommand};
use colored::*;
use std::process::ExitCode;

mod commands;
mod config;
mod manifest_io;

#[derive(Parser, Debug)]
#[command(
    name = "abw",
    version,
    about = "Agent Bestiary Workspace CLI — build Apps on ABW",
    long_about = None,
)]
struct Cli {
    #[command(subcommand)]
    cmd: Top,

    /// Override the ABW API base URL. Defaults to $ABW_BASE_URL or
    /// https://agent-bestiary.world.
    #[arg(long, global = true)]
    base_url: Option<String>,

    /// Silence non-essential output.
    #[arg(long, global = true)]
    quiet: bool,
}

#[derive(Subcommand, Debug)]
enum Top {
    /// Authenticate this machine against ABW. Opens a browser to complete
    /// OAuth and stores a per-machine API key at ~/.abw/credentials.
    Login(commands::login::Args),

    /// Show the current authentication and API base URL.
    Whoami,

    /// Log out — remove stored credentials from this machine.
    Logout,

    /// App primitive — build, validate, deploy, and spawn Apps.
    #[command(subcommand)]
    App(AppCmd),

    /// Workspace interaction — messages, files, and action protocol.
    #[command(subcommand)]
    Workspace(commands::workspace::WorkspaceCmd),
}

#[derive(Subcommand, Debug)]
enum AppCmd {
    /// Scaffold a new App directory with sensible defaults.
    New(commands::new::Args),

    /// Validate the local manifest.json against the platform rules — no
    /// network calls. Exits non-zero if there are blocking errors.
    Validate(commands::validate::Args),

    /// Validate + register (or update) the App on the ABW platform.
    Deploy(commands::deploy::Args),

    /// Spawn a workspace from a deployed App.
    Spawn(commands::spawn::Args),

    /// List Apps. Without --public, lists your own Apps (any visibility).
    /// Add --public to browse the catalogue instead.
    List(commands::list::Args),

    /// Promote an App's visibility to `public` — makes it browsable in the
    /// catalogue. Apps default to `private` on `abw app deploy`; this is
    /// the explicit promotion step.
    Publish(commands::publish::Args),
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    // Set base URL precedence: --base-url > $ABW_BASE_URL > default.
    let base_url = cli
        .base_url
        .clone()
        .or_else(|| std::env::var("ABW_BASE_URL").ok())
        .unwrap_or_else(|| "https://agent-bestiary.world".to_string());
    let base_url = base_url.trim_end_matches('/').to_string();

    let ctx = commands::Ctx {
        base_url,
        quiet: cli.quiet,
    };

    let result: anyhow::Result<()> = match cli.cmd {
        Top::Login(args) => commands::login::run(&ctx, args).await,
        Top::Whoami => commands::login::whoami(&ctx).await,
        Top::Logout => commands::login::logout(&ctx).await,
        Top::App(app_cmd) => match app_cmd {
            AppCmd::New(args) => commands::new::run(&ctx, args).await,
            AppCmd::Validate(args) => commands::validate::run(&ctx, args).await,
            AppCmd::Deploy(args) => commands::deploy::run(&ctx, args).await,
            AppCmd::Spawn(args) => commands::spawn::run(&ctx, args).await,
            AppCmd::List(args) => commands::list::run(&ctx, args).await,
            AppCmd::Publish(args) => commands::publish::run(&ctx, args).await,
        },
        Top::Workspace(ws_cmd) => commands::workspace::run(&ctx, ws_cmd).await,
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{} {}", "error:".red().bold(), e);
            // Surface the chain so users see *why*, not just the top error.
            let mut source = e.source();
            while let Some(s) = source {
                eprintln!("  {} {}", "caused by:".dimmed(), s);
                source = s.source();
            }
            ExitCode::FAILURE
        }
    }
}

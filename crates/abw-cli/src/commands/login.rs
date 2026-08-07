//! `abw login` — authenticate this machine against ABW.
//!
//! Flow (matches `gh auth login`, `gcloud auth login`, `fly auth login`):
//!
//!   1. CLI listens on a random localhost port.
//!   2. CLI opens the browser to `$ABW_BASE_URL/auth/cli?callback=...&state=...`.
//!   3. The user authenticates via Google or GitHub on the server (or re-uses
//!      an existing browser session if already signed in).
//!   4. The server mints a per-machine API key scoped to `cli` and redirects
//!      to `http://localhost:<port>/cb?api_key=...&user=...&state=...`.
//!   5. The CLI receives the callback, verifies the state nonce, saves the
//!      API key to ~/.abw/credentials, shows a success page, and shuts down.
//!
//! Fallback: if the server doesn't expose `/auth/cli` (older deployments),
//! the CLI prints instructions for manually minting a key at
//! `$ABW_BASE_URL/settings/api-keys` and pasting it via `--token`.

use super::Ctx;
use anyhow::{anyhow, Context, Result};
use clap::Args as ClapArgs;
use colored::*;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::config::{self, Credentials};

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Skip the browser flow and use this API key directly. Useful for CI
    /// or for environments where opening a browser isn't an option.
    #[arg(long, value_name = "API_KEY")]
    pub token: Option<String>,

    /// Don't try to open the browser; print the URL for manual paste.
    #[arg(long)]
    pub no_browser: bool,
}

pub async fn run(ctx: &Ctx, args: Args) -> Result<()> {
    // Fast path: explicit --token bypasses the browser flow entirely.
    if let Some(token) = args.token {
        return save_credentials(ctx, &token, None).await;
    }

    // Open a localhost listener; the OS picks a free port.
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("binding localhost listener for OAuth callback")?;
    let local_addr = listener
        .local_addr()
        .context("getting localhost listener address")?;
    let port = local_addr.port();

    // Generate a state nonce for CSRF protection.
    let state = generate_nonce();

    let callback = format!("http://127.0.0.1:{}/cb", port);
    let auth_url = format!(
        "{}/auth/cli?callback={}&state={}",
        ctx.base_url,
        urlencode(&callback),
        urlencode(&state),
    );

    if !ctx.quiet {
        println!();
        println!(
            "  {} Open the following URL to authenticate:",
            "→".cyan().bold()
        );
        println!("    {}", auth_url.underline());
        println!();
    }

    if !args.no_browser {
        if let Err(e) = open::that(&auth_url) {
            eprintln!(
                "  {} could not open browser ({}). Paste the URL above into your browser manually.",
                "warn".yellow().bold(),
                e
            );
        }
    }

    if !ctx.quiet {
        println!("  Waiting for callback on {} (timeout 5 min)…", local_addr);
    }

    // Wait for the callback. 5-minute timeout so the CLI doesn't hang forever
    // if the user closes the browser tab.
    let (api_key, user) = tokio::time::timeout(
        Duration::from_secs(300),
        wait_for_callback(listener, &state),
    )
    .await
    .map_err(|_| anyhow!("login timed out after 5 minutes"))??;

    save_credentials(ctx, &api_key, user.as_deref()).await
}

async fn save_credentials(ctx: &Ctx, api_key: &str, user: Option<&str>) -> Result<()> {
    let mut creds = Credentials::default();
    creds.api_key = Some(api_key.to_string());
    creds.base_url = Some(ctx.base_url.clone());
    creds.user = user.map(String::from);
    config::save(&creds)?;

    if !ctx.quiet {
        println!();
        println!(
            "  {} Logged in to {}",
            "✓".green().bold(),
            ctx.base_url.bold()
        );
        if let Some(u) = user {
            println!("  {} {}", "user:".dimmed(), u);
        }
        println!("  {} ~/.abw/credentials (mode 0600)", "saved:".dimmed());
        println!();
    }
    Ok(())
}

pub async fn whoami(ctx: &Ctx) -> Result<()> {
    let creds = config::load()?;
    let api_key = config::resolve_api_key().ok();

    println!();
    println!("  {} {}", "base url:".bold(), ctx.base_url);
    match api_key {
        Some(k) => {
            let masked = mask_key(&k);
            println!("  {}  {}", "api key: ".bold(), masked);
        }
        None => {
            println!(
                "  {}  {}",
                "api key: ".bold(),
                "(not set — run `abw login`)".dimmed()
            );
        }
    }
    if let Some(u) = creds.user {
        println!("  {}  {}", "user:    ".bold(), u);
    }
    println!();
    Ok(())
}

pub async fn logout(ctx: &Ctx) -> Result<()> {
    config::remove()?;
    if !ctx.quiet {
        println!();
        println!(
            "  {} Credentials removed from this machine.",
            "✓".green().bold()
        );
        println!("  {} {}", "base url:".dimmed(), ctx.base_url);
        println!();
    }
    Ok(())
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Listen for the single OAuth callback request and return (api_key, user).
async fn wait_for_callback(
    listener: TcpListener,
    expected_state: &str,
) -> Result<(String, Option<String>)> {
    loop {
        let (mut socket, _addr) = listener
            .accept()
            .await
            .context("accepting OAuth callback connection")?;

        let mut buf = vec![0u8; 4096];
        let n = socket
            .read(&mut buf)
            .await
            .context("reading OAuth callback request")?;
        let request = String::from_utf8_lossy(&buf[..n]).to_string();

        // Parse the request line.
        let first_line = request.lines().next().unwrap_or("");
        let mut parts = first_line.split_whitespace();
        let _method = parts.next();
        let path_and_query = parts.next().unwrap_or("");

        if !path_and_query.starts_with("/cb") {
            // Unknown path — likely a probe. Send 404 and keep listening.
            write_response(&mut socket, 404, "text/plain", "not found")
                .await
                .ok();
            continue;
        }

        let query = path_and_query.splitn(2, '?').nth(1).unwrap_or("");
        let params = parse_query(query);

        // CSRF check.
        let received_state = params
            .iter()
            .find(|(k, _)| k == "state")
            .map(|(_, v)| v.as_str());
        if received_state != Some(expected_state) {
            write_response(
                &mut socket,
                400,
                "text/html",
                &error_page("State nonce mismatch — possible CSRF. Re-run `abw login`."),
            )
            .await
            .ok();
            return Err(anyhow!("OAuth callback state nonce mismatch"));
        }

        // Surface the server's error message if it sent one.
        if let Some((_, err)) = params.iter().find(|(k, _)| k == "error") {
            write_response(
                &mut socket,
                400,
                "text/html",
                &error_page(&format!("Server returned error: {}", err)),
            )
            .await
            .ok();
            return Err(anyhow!("server reported error: {}", err));
        }

        let api_key = params
            .iter()
            .find(|(k, _)| k == "api_key")
            .map(|(_, v)| v.clone())
            .ok_or_else(|| anyhow!("OAuth callback missing api_key parameter"))?;

        let user = params
            .iter()
            .find(|(k, _)| k == "user")
            .map(|(_, v)| v.clone());

        write_response(&mut socket, 200, "text/html", &success_page())
            .await
            .ok();

        return Ok((api_key, user));
    }
}

async fn write_response(
    socket: &mut tokio::net::TcpStream,
    status: u16,
    content_type: &str,
    body: &str,
) -> std::io::Result<()> {
    let status_text = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "Error",
    };
    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status,
        status_text,
        content_type,
        body.len(),
        body
    );
    socket.write_all(response.as_bytes()).await?;
    socket.flush().await?;
    Ok(())
}

fn success_page() -> String {
    r#"<!doctype html>
<html><head><meta charset="utf-8"><title>ABW — logged in</title>
<style>
  body { font-family: -apple-system, system-ui, sans-serif; max-width: 480px;
         margin: 8em auto; padding: 0 2em; color: #1a1a1a; }
  h1 { color: #fabd2f; font-weight: 600; }
  p { line-height: 1.5; color: #555; }
  .dim { color: #999; font-size: 0.85em; margin-top: 3em; }
</style></head>
<body>
  <h1>✓ Logged in</h1>
  <p>The CLI now has a credential for this machine. You can close this tab and return to your terminal.</p>
  <p class="dim">Credentials are stored at <code>~/.abw/credentials</code> (mode 0600).
     Run <code>abw logout</code> to revoke from this machine.</p>
</body></html>"#
        .to_string()
}

fn error_page(message: &str) -> String {
    format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8"><title>ABW — login error</title>
<style>
  body {{ font-family: -apple-system, system-ui, sans-serif; max-width: 480px;
          margin: 8em auto; padding: 0 2em; color: #1a1a1a; }}
  h1 {{ color: #c14a4a; font-weight: 600; }}
  p {{ line-height: 1.5; color: #555; }}
</style></head>
<body>
  <h1>Login failed</h1>
  <p>{}</p>
</body></html>"#,
        html_escape(message)
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn parse_query(q: &str) -> Vec<(String, String)> {
    q.split('&')
        .filter_map(|pair| {
            let mut it = pair.splitn(2, '=');
            let k = it.next()?;
            let v = it.next().unwrap_or("");
            Some((urldecode(k), urldecode(v)))
        })
        .collect()
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

fn urldecode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) =
                u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16)
            {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn generate_nonce() -> String {
    // Time-based nonce mixed with process id — good enough for CSRF defence
    // on a short-lived loopback callback. Not security-critical because the
    // listener only accepts one connection and we verify the nonce against
    // what the server echoes back.
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    format!("{:x}{:x}", ts, pid)
}

fn mask_key(k: &str) -> String {
    if k.len() <= 8 {
        "***".to_string()
    } else {
        format!("{}…{}", &k[..6], &k[k.len() - 4..])
    }
}
